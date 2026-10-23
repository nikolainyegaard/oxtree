//! Read a whole NTFS volume from its Master File Table, the way WizTree
//! does, instead of walking directories. The MFT is one array of fixed
//! size records, one per file or directory, each naming its parent, so a
//! sequential read of it gives the tree without a single directory
//! listing. On Windows the volume is opened raw as `\\.\C:`, which needs
//! administrator rights; the parser itself only needs `Read + Seek` and
//! is tested on Linux against a small image.
//!
//! What is read from each record: the names and parents from every
//! `$FILE_NAME`, the modification time and attributes from
//! `$STANDARD_INFORMATION`, the allocated size of the unnamed `$DATA`
//! stream and, for directories, of `$INDEX_ALLOCATION`. Extension records
//! (a file with so many fragments that its attributes spill over) are
//! folded into their base record. A file with several names is a hard
//! link set and each link gets an equal share of the size, as the walker
//! does. Named streams, reparse point targets and security are not read.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use oxtree_core::{DirRecord, FileRecord, Kind, Tree};

const RECORD_MAGIC: &[u8; 4] = b"FILE";
const ROOT_RECORD: u32 = 5;
const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_INDEX_ALLOCATION: u32 = 0xA0;
const ATTR_END: u32 = 0xFFFF_FFFF;
const FLAG_IN_USE: u16 = 0x01;
const FLAG_DIRECTORY: u16 = 0x02;
const ATTR_FLAG_COMPRESSED: u16 = 0x0001;
const ATTR_FLAG_SPARSE: u16 = 0x8000;
const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const NAMESPACE_DOS: u8 = 2;
/// Seconds between 1601-01-01 and 1970-01-01.
const EPOCH_DIFF: i64 = 11_644_473_600;
/// Bytes read from the volume at a time; rounded up to whole clusters.
const CHUNK: usize = 4 << 20;
/// Records between two progress reports.
const PROGRESS_EVERY: usize = 20_000;

/// The drive letter and the path components below its root, for a path
/// such as `C:\Users\me`. `None` for anything else: a UNC path, a Unix
/// path, a bare relative name.
pub fn split_drive(path: &Path) -> Option<(char, Vec<String>)> {
    let s = path.to_str()?;
    let mut chars = s.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    if !letter.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }
    let rest = chars.as_str();
    if !(rest.is_empty() || rest.starts_with(['\\', '/'])) {
        return None;
    }
    let below = rest
        .split(['\\', '/'])
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .collect();
    Some((letter, below))
}

/// Read the tree below `path` from its volume's MFT. `None` when the path
/// is not on a drive letter, the volume cannot be opened (not running as
/// administrator) or it is not NTFS; the caller walks the directories
/// instead. `progress` gets the item count and byte total now and then.
pub fn scan(
    path: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, u64),
) -> Option<Result<Tree, String>> {
    let (drive, below) = split_drive(path)?;
    let mut file = File::open(format!(r"\\.\{drive}:")).ok()?;
    let volume = Volume::open(&mut file).ok()?;
    let mut root_name = path.to_string_lossy().into_owned();
    while root_name.len() > 3 && root_name.ends_with(['\\', '/']) {
        root_name.pop();
    }
    Some(volume.read(&mut file, root_name, &below, cancel, progress))
}

/// The volume geometry from the boot sector and the extents of `$MFT`
/// from its own record.
pub struct Volume {
    cluster: u64,
    record: usize,
    /// `(first cluster, cluster count)` of each `$MFT` extent, in order.
    mft_runs: Vec<(u64, u64)>,
    /// Bytes of `$MFT` in use.
    mft_len: u64,
}

impl Volume {
    /// # Errors
    /// Not an NTFS volume, or its `$MFT` record cannot be read.
    pub fn open(src: &mut (impl Read + Seek)) -> Result<Volume, String> {
        // One 4 KiB read covers the boot sector whatever the sector size,
        // and stays aligned for raw volume reads.
        let mut boot = vec![0u8; 4096];
        read_at(src, 0, &mut boot).map_err(|e| format!("Cannot read the boot sector: {e}"))?;
        if boot.get(3..11) != Some(b"NTFS    ") {
            return Err("not an NTFS volume".into());
        }
        let bytes_per_sector = u64::from(u16_at(&boot, 0x0B).ok_or("short boot sector")?);
        let spc = u64::from(*boot.get(0x0D).ok_or("short boot sector")?);
        let cluster = if spc >= 0x80 {
            1u64 << (256 - spc)
        } else {
            bytes_per_sector.saturating_mul(spc)
        };
        let mft_lcn = u64_at(&boot, 0x30).ok_or("short boot sector")?;
        let cpr = *boot.get(0x40).ok_or("short boot sector")? as i8;
        let record = if cpr < 0 {
            1usize << (-i32::from(cpr))
        } else {
            usize::try_from(cluster.saturating_mul(cpr as u64)).map_err(|_| "record size")?
        };
        if cluster == 0 || !(512..=1 << 16).contains(&record) || cluster > 1 << 24 {
            return Err("unreasonable NTFS geometry".into());
        }
        // Record 0 describes $MFT itself. One cluster, or one record when
        // records are the larger, keeps the read aligned.
        let want = usize::try_from(cluster).unwrap_or(usize::MAX).max(record);
        let mut first = vec![0u8; want];
        read_at(src, mft_lcn.saturating_mul(cluster), &mut first)
            .map_err(|e| format!("Cannot read the $MFT record: {e}"))?;
        first.truncate(record);
        if !fixup(&mut first) {
            return Err("the $MFT record is damaged".into());
        }
        let mut mft_runs = Vec::new();
        let mut mft_len = 0;
        for attr in attributes(&first) {
            if attr.kind == ATTR_DATA && !attr.named {
                if let Body::NonResident {
                    lowest_vcn: 0,
                    data_size,
                    runs,
                    ..
                } = attr.body
                {
                    mft_runs = data_runs(runs);
                    mft_len = data_size;
                }
            }
        }
        if mft_runs.is_empty() {
            return Err("the $MFT record has no data runs".into());
        }
        Ok(Volume {
            cluster,
            record,
            mft_runs,
            mft_len,
        })
    }

    /// Read every record and assemble the tree below `below`, a list of
    /// path components under the volume root. `root_name` becomes the root
    /// node's name, so it should be the path as the user wrote it.
    ///
    /// # Errors
    /// Reading fails, the scan is cancelled, or `below` names a path that
    /// is not on the volume.
    pub fn read(
        &self,
        src: &mut (impl Read + Seek),
        root_name: String,
        below: &[String],
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(usize, u64),
    ) -> Result<Tree, String> {
        let entries = self.read_entries(src, cancel, progress)?;
        let mut start = ROOT_RECORD;
        for component in below {
            start = child_dir_named(&entries, start, component)
                .ok_or_else(|| format!("{component} was not found on the volume"))?;
        }
        let dirs = assemble(entries, start, root_name);
        Tree::build(
            dirs.first().map(|d| d.name.clone()).unwrap_or_default(),
            dirs,
        )
    }

    fn read_entries(
        &self,
        src: &mut (impl Read + Seek),
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(usize, u64),
    ) -> Result<Vec<Entry>, String> {
        let cluster = usize::try_from(self.cluster).map_err(|_| "cluster size")?;
        let chunk = CHUNK.div_ceil(cluster) * cluster;
        let count = usize::try_from(self.mft_len / self.record as u64).unwrap_or(0);
        // ponytail: one Entry per record, about 80 bytes plus the name;
        // a name arena would halve it if a volume with many million files
        // ever matters.
        let mut entries: Vec<Entry> = Vec::with_capacity(count);
        let mut buf = vec![0u8; chunk];
        let mut carry: Vec<u8> = Vec::new();
        let mut index: u32 = 0;
        let mut bytes: u64 = 0;
        let mut remaining = self.mft_len;
        let mut since_report = 0;
        for &(lcn, len) in &self.mft_runs {
            let mut offset = lcn.saturating_mul(self.cluster);
            let mut run_left = len.saturating_mul(self.cluster).min(remaining);
            while run_left > 0 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Reading aborted.".into());
                }
                // Reads stay whole clusters; the tail past `mft_len` is
                // ignored after the read.
                let want = usize::try_from(run_left.min(chunk as u64)).unwrap_or(chunk);
                let aligned = want.div_ceil(cluster) * cluster;
                let slice = buf.get_mut(..aligned).ok_or("chunk size")?;
                read_at(src, offset, slice).map_err(|e| format!("Cannot read the MFT: {e}"))?;
                let data = slice.get(..want).ok_or("chunk size")?;
                offset = offset.saturating_add(aligned as u64);
                run_left -= want as u64;
                remaining -= want as u64;

                // A record split across two reads is finished from the carry.
                let mut data = data;
                if !carry.is_empty() {
                    let need = self.record - carry.len();
                    let (head, rest) = data.split_at(need.min(data.len()));
                    carry.extend_from_slice(head);
                    data = rest;
                    if carry.len() == self.record {
                        let mut rec = std::mem::take(&mut carry);
                        bytes += parse_record(&mut rec, index, &mut entries);
                        index = index.saturating_add(1);
                        since_report += 1;
                    }
                }
                let mut records = data.chunks_exact(self.record);
                let mut rec = vec![0u8; self.record];
                for r in &mut records {
                    rec.copy_from_slice(r);
                    bytes += parse_record(&mut rec, index, &mut entries);
                    index = index.saturating_add(1);
                    since_report += 1;
                }
                carry.extend_from_slice(records.remainder());
                if since_report >= PROGRESS_EVERY {
                    since_report = 0;
                    progress(entries.iter().filter(|e| e.in_use).count(), bytes);
                }
            }
        }
        Ok(entries)
    }
}

/// One MFT record, kept only as long as the tree is being assembled.
#[derive(Clone, Default)]
struct Entry {
    name: String,
    /// Parent record and the sequence number the link expects it to have.
    parent: u32,
    parent_seq: u16,
    /// Further names of the same record: hard links.
    extra: Vec<(u32, u16, String)>,
    size: u64,
    /// Directories: the allocated size of the index.
    own: u64,
    mtime: i64,
    attributes: u32,
    seq: u16,
    in_use: bool,
    is_dir: bool,
}

/// Decode one record into `entries`, growing the list as needed. Returns
/// the allocated bytes found, for the progress total.
fn parse_record(rec: &mut [u8], index: u32, entries: &mut Vec<Entry>) -> u64 {
    if rec.get(..4) != Some(RECORD_MAGIC) || !fixup(rec) {
        return 0;
    }
    let Some(flags) = u16_at(rec, 0x16) else {
        return 0;
    };
    let Some(seq) = u16_at(rec, 0x10) else {
        return 0;
    };
    let base = u64_at(rec, 0x20).unwrap_or(0) & 0xFFFF_FFFF_FFFF;
    let target = if base == 0 {
        index
    } else {
        u32::try_from(base).unwrap_or(u32::MAX)
    };
    let target_i = target as usize;
    if entries.len() <= target_i {
        entries.resize(target_i + 1, Entry::default());
    }
    if entries.len() <= index as usize {
        entries.resize(index as usize + 1, Entry::default());
    }
    let Some(entry) = entries.get_mut(target_i) else {
        return 0;
    };
    let mut found = 0;
    let mut names: Vec<(u32, u16, String)> = Vec::new();
    let mut dos: Option<(u32, u16, String)> = None;
    for attr in attributes(rec) {
        match (attr.kind, attr.body) {
            (ATTR_STANDARD_INFORMATION, Body::Resident(v)) if base == 0 => {
                if let Some(ft) = u64_at(v, 0x08) {
                    entry.mtime = filetime_to_unix(ft);
                }
                entry.attributes = u32_at(v, 0x20).unwrap_or(0);
            }
            (ATTR_FILE_NAME, Body::Resident(v)) if base == 0 => {
                let Some(parent_ref) = u64_at(v, 0) else {
                    continue;
                };
                let Some(&len) = v.get(0x40) else { continue };
                let Some(&namespace) = v.get(0x41) else {
                    continue;
                };
                let Some(raw) = v.get(0x42..0x42 + usize::from(len) * 2) else {
                    continue;
                };
                let units: Vec<u16> = raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                let name = String::from_utf16_lossy(&units);
                let link = (
                    u32::try_from(parent_ref & 0xFFFF_FFFF_FFFF).unwrap_or(u32::MAX),
                    (parent_ref >> 48) as u16,
                    name,
                );
                if namespace == NAMESPACE_DOS {
                    dos = Some(link);
                } else {
                    names.push(link);
                }
            }
            (ATTR_DATA, Body::Resident(v)) if !attr.named => {
                // Resident data lives inside the record; its bytes are what
                // the user sees as the file's size.
                entry.size = v.len() as u64;
                found = entry.size;
            }
            (
                ATTR_DATA,
                Body::NonResident {
                    lowest_vcn: 0,
                    allocated,
                    compressed,
                    ..
                },
            ) if !attr.named => {
                entry.size = compressed.unwrap_or(allocated);
                found = entry.size;
            }
            (
                ATTR_INDEX_ALLOCATION,
                Body::NonResident {
                    lowest_vcn: 0,
                    allocated,
                    ..
                },
            ) => {
                entry.own = allocated;
            }
            _ => {}
        }
    }
    if base == 0 {
        entry.seq = seq;
        entry.in_use = flags & FLAG_IN_USE != 0;
        entry.is_dir = flags & FLAG_DIRECTORY != 0;
        if names.is_empty() {
            names.extend(dos);
        }
        let mut names = names.into_iter();
        if let Some((parent, parent_seq, name)) = names.next() {
            entry.parent = parent;
            entry.parent_seq = parent_seq;
            entry.name = name;
        } else {
            entry.in_use = false;
        }
        entry.extra = names.collect();
    }
    found
}

/// Whether `link` of `child` hangs below a live directory.
fn link_is_live(entries: &[Entry], parent: u32, parent_seq: u16) -> bool {
    entries
        .get(parent as usize)
        .is_some_and(|p| p.in_use && p.is_dir && (p.seq == parent_seq || parent == ROOT_RECORD))
}

fn child_dir_named(entries: &[Entry], dir: u32, name: &str) -> Option<u32> {
    entries.iter().enumerate().find_map(|(i, e)| {
        let i = u32::try_from(i).ok()?;
        let hit = e.in_use
            && e.is_dir
            && i != ROOT_RECORD
            && std::iter::once((e.parent, e.parent_seq, &e.name))
                .chain(e.extra.iter().map(|(p, s, n)| (*p, *s, n)))
                .any(|(p, s, n)| p == dir && link_is_live(entries, p, s) && eq_fold(n, name));
        hit.then_some(i)
    })
}

/// Case-insensitive comparison the way NTFS names are matched.
fn eq_fold(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

/// Directory records in parent-before-child order, from `start` down,
/// the way `Tree::build` wants them.
fn assemble(mut entries: Vec<Entry>, start: u32, root_name: String) -> Vec<DirRecord> {
    // Children by parent, as a compact adjacency list: `(child, slot)`
    // where slot 0 is the record's first name and k its k-th hard link.
    let n = entries.len();
    let mut count = vec![0u32; n + 1];
    let mut links: Vec<(u32, u32, u32)> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        if !e.in_use || i as u32 == ROOT_RECORD {
            continue;
        }
        let own = std::iter::once((e.parent, e.parent_seq));
        let more = e.extra.iter().map(|(p, s, _)| (*p, *s));
        for (slot, (p, s)) in own.chain(more).enumerate() {
            if link_is_live(&entries, p, s) {
                links.push((p, i as u32, slot as u32));
                if let Some(c) = count.get_mut(p as usize + 1) {
                    *c += 1;
                }
            }
        }
    }
    links.sort_unstable();
    let mut first = count;
    for i in 1..first.len() {
        let prev = first.get(i - 1).copied().unwrap_or(0);
        if let Some(v) = first.get_mut(i) {
            *v += prev;
        }
    }
    let children = |p: u32| -> &[(u32, u32, u32)] {
        let a = first.get(p as usize).copied().unwrap_or(0) as usize;
        let b = first.get(p as usize + 1).copied().unwrap_or(0) as usize;
        links.get(a..b).unwrap_or(&[])
    };

    let mut dirs: Vec<DirRecord> = Vec::new();
    let root = entries.get(start as usize).cloned().unwrap_or_default();
    dirs.push(DirRecord {
        parent: None,
        name: root_name,
        mtime: root.mtime,
        mode: 0o040755,
        own_size: root.own,
        kind: Some(Kind::Dir),
        ..Default::default()
    });
    let mut queue = VecDeque::from([(start, 0usize)]);
    while let Some((rec, id)) = queue.pop_front() {
        for &(_, child, slot) in children(rec) {
            let Some(e) = entries.get_mut(child as usize) else {
                continue;
            };
            let name = if slot == 0 {
                std::mem::take(&mut e.name)
            } else {
                e.extra
                    .get_mut(slot as usize - 1)
                    .map(|(_, _, n)| std::mem::take(n))
                    .unwrap_or_default()
            };
            let reparse = e.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
            if e.is_dir {
                let cid = dirs.len();
                // A junction or mounted volume has no children in this MFT.
                let kind = if reparse { Kind::MountPoint } else { Kind::Dir };
                dirs.push(DirRecord {
                    parent: Some(id),
                    name,
                    mtime: e.mtime,
                    mode: 0o040755,
                    own_size: e.own,
                    kind: Some(kind),
                    ..Default::default()
                });
                if let Some(d) = dirs.get_mut(id) {
                    d.subdirs.push(cid);
                }
                if kind == Kind::Dir {
                    queue.push_back((child, cid));
                }
            } else {
                let links = 1 + e.extra.len() as u64;
                let (kind, mode) = if reparse {
                    (Kind::Symlink, 0o120777)
                } else if e.attributes & FILE_ATTRIBUTE_READONLY != 0 {
                    (Kind::File, 0o100444)
                } else {
                    (Kind::File, 0o100644)
                };
                if let Some(d) = dirs.get_mut(id) {
                    d.files.push(FileRecord {
                        name,
                        size: e.size / links,
                        mtime: e.mtime,
                        mode,
                        kind,
                    });
                }
            }
        }
    }
    dirs
}

/// Apply the update sequence: NTFS writes the sequence number into the
/// last two bytes of every sector of a record and keeps the real bytes in
/// an array; a mismatch means a torn write. False when the record is bad.
fn fixup(rec: &mut [u8]) -> bool {
    let (Some(usa_off), Some(usa_count)) = (u16_at(rec, 4), u16_at(rec, 6)) else {
        return false;
    };
    let usa_off = usize::from(usa_off);
    let sectors = usize::from(usa_count).saturating_sub(1);
    if sectors == 0 || !rec.len().is_multiple_of(sectors) {
        return false;
    }
    let stride = rec.len() / sectors;
    let Some(usn) = u16_at(rec, usa_off) else {
        return false;
    };
    for i in 1..=sectors {
        let Some(fix) = u16_at(rec, usa_off + 2 * i) else {
            return false;
        };
        let end = i * stride - 2;
        if u16_at(rec, end) != Some(usn) {
            return false;
        }
        let Some(slot) = rec.get_mut(end..end + 2) else {
            return false;
        };
        slot.copy_from_slice(&fix.to_le_bytes());
    }
    true
}

struct Attr<'a> {
    kind: u32,
    named: bool,
    body: Body<'a>,
}

enum Body<'a> {
    Resident(&'a [u8]),
    NonResident {
        lowest_vcn: u64,
        allocated: u64,
        data_size: u64,
        /// The real allocation of a compressed or sparse stream.
        compressed: Option<u64>,
        runs: &'a [u8],
    },
}

/// The attributes of a record, from its header's offset to the end marker.
fn attributes(rec: &[u8]) -> impl Iterator<Item = Attr<'_>> {
    let mut off = u16_at(rec, 0x14).map(usize::from).unwrap_or(rec.len());
    std::iter::from_fn(move || {
        let kind = u32_at(rec, off)?;
        if kind == ATTR_END {
            return None;
        }
        let len = u32_at(rec, off + 4)? as usize;
        if len < 0x18 {
            return None;
        }
        let a = rec.get(off..off + len)?;
        off += len;
        let non_resident = *a.get(8)? != 0;
        let named = *a.get(9)? != 0;
        let flags = u16_at(a, 0x0C)?;
        let body = if non_resident {
            let runs_off = usize::from(u16_at(a, 0x20)?);
            let compressed = (flags & (ATTR_FLAG_COMPRESSED | ATTR_FLAG_SPARSE) != 0
                && runs_off >= 0x48)
                .then(|| u64_at(a, 0x40))
                .flatten();
            Body::NonResident {
                lowest_vcn: u64_at(a, 0x10)?,
                allocated: u64_at(a, 0x28)?,
                data_size: u64_at(a, 0x30)?,
                compressed,
                runs: a.get(runs_off..)?,
            }
        } else {
            let vlen = u32_at(a, 0x10)? as usize;
            let voff = usize::from(u16_at(a, 0x14)?);
            Body::Resident(a.get(voff..voff + vlen)?)
        };
        Some(Attr { kind, named, body })
    })
}

/// Decode a data run list into `(first cluster, cluster count)` extents.
/// Sparse runs (no offset) hold no data and are skipped.
fn data_runs(mut b: &[u8]) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut lcn: i64 = 0;
    while let Some(&h) = b.first() {
        if h == 0 {
            break;
        }
        let len_n = usize::from(h & 0xF);
        let off_n = usize::from(h >> 4);
        let Some(len_bytes) = b.get(1..1 + len_n) else {
            break;
        };
        let Some(off_bytes) = b.get(1 + len_n..1 + len_n + off_n) else {
            break;
        };
        let len = le_unsigned(len_bytes);
        if off_n > 0 {
            lcn = lcn.saturating_add(le_signed(off_bytes));
            if lcn >= 0 {
                out.push((lcn as u64, len));
            }
        }
        b = b.get(1 + len_n + off_n..).unwrap_or(&[]);
    }
    out
}

fn le_unsigned(b: &[u8]) -> u64 {
    b.iter()
        .rev()
        .fold(0u64, |acc, &x| (acc << 8) | u64::from(x))
}

fn le_signed(b: &[u8]) -> i64 {
    let v = le_unsigned(b);
    let bits = b.len() * 8;
    if bits == 0 || bits >= 64 {
        return v as i64;
    }
    if v & (1 << (bits - 1)) != 0 {
        (v | (!0u64 << bits)) as i64
    } else {
        v as i64
    }
}

fn filetime_to_unix(ft: u64) -> i64 {
    (ft / 10_000_000) as i64 - EPOCH_DIFF
}

fn read_at(src: &mut (impl Read + Seek), offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
    src.seek(SeekFrom::Start(offset))?;
    src.read_exact(buf)
}

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    b.get(off..off + 2)
        .and_then(|s| s.try_into().ok())
        .map(u16::from_le_bytes)
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
}

fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    b.get(off..off + 8)
        .and_then(|s| s.try_into().ok())
        .map(u64::from_le_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_paths_split_and_others_do_not() {
        assert_eq!(
            split_drive(Path::new("C:\\")),
            Some(('C', Vec::<String>::new()))
        );
        assert_eq!(
            split_drive(Path::new("d:/Users/me/")),
            Some(('D', vec!["Users".to_string(), "me".to_string()]))
        );
        assert_eq!(split_drive(Path::new("/usr")), None);
        assert_eq!(split_drive(Path::new("\\\\server\\share")), None);
        assert_eq!(split_drive(Path::new("C:relative")), None);
    }

    #[test]
    fn data_runs_decode_signed_offsets() {
        // 0x21 = 1 byte length, 2 byte offset: 4 clusters at 0x1234,
        // then 0x11: 2 clusters 3 before that, then a sparse run, then end.
        let b = [0x21, 0x04, 0x34, 0x12, 0x11, 0x02, 0xFD, 0x01, 0x05, 0x00];
        assert_eq!(data_runs(&b), vec![(0x1234, 4), (0x1231, 2)]);
    }

    #[test]
    fn filetime_epoch() {
        assert_eq!(filetime_to_unix(116_444_736_000_000_000), 0);
    }

    /// A 16 MiB image made with mkntfs and filled through ntfs-3g; see
    /// docs/gotchas.md for how to rebuild it. Skipped where `xz` is missing.
    #[test]
    fn reads_the_fixture_image() {
        let xz = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/ntfs.img.xz");
        let out = match std::process::Command::new("xz").args(["-dc", xz]).output() {
            Ok(o) if o.status.success() => o.stdout,
            _ => return,
        };
        let mut src = std::io::Cursor::new(out);
        let volume = Volume::open(&mut src).unwrap();
        assert_eq!(volume.cluster, 4096);
        assert_eq!(volume.record, 1024);
        let cancel = AtomicBool::new(false);
        let mut reports = 0;
        let tree = volume
            .read(&mut src, "X:\\".into(), &[], &cancel, &mut |_, _| {
                reports += 1
            })
            .unwrap();

        let names = |n| -> Vec<String> {
            let mut v: Vec<String> = tree.children(n).map(|c| tree.name(c).to_string()).collect();
            v.sort();
            v
        };
        let top = names(0);
        for want in ["docs", "empty", "många filer", "<Files>", "$Extend"] {
            assert!(top.contains(&want.to_string()), "{top:?}");
        }
        let find = |dir, name: &str| {
            tree.children(dir)
                .find(|&c| tree.name(c) == name)
                .or_else(|| {
                    tree.children(dir)
                        .find(|&c| tree.kind(c) == Kind::DotEntry)
                        .and_then(|d| tree.children(d).find(|&c| tree.name(c) == name))
                })
                .unwrap()
        };
        let many = find(0, "många filer");
        assert_eq!(tree.total_files(many), 400);
        let empty = find(0, "empty");
        assert_eq!(tree.child_count(empty), 0);
        let docs = find(0, "docs");
        let deep = find(docs, "deep");
        let deeper = find(deep, "deeper");
        let big = find(deeper, "big.bin");
        assert_eq!(tree.size(big), 288_894u64.div_ceil(4096) * 4096);
        let tiny = find(docs, "tiny.txt");
        assert_eq!(tree.size(tiny), 5, "resident data counts its bytes");
        let linked = find(docs, "linked.bin");
        let link_two = find(deep, "link-two.bin");
        assert_eq!(tree.size(linked), tree.size(link_two));
        assert_eq!(tree.size(linked) * 2, 60_894u64.div_ceil(4096) * 4096);
        assert_eq!(tree.path(big), "X:\\docs\\deep\\deeper\\big.bin");
        assert_eq!(tree.size(find(0, "zeros.bin")), 1 << 20);
        assert!(
            tree.size(find(0, "$MFT")) >= 400 * 1024,
            "the MFT counts itself"
        );

        // A subtree start resolves case-insensitively.
        let mut src2 = std::io::Cursor::new(src.into_inner());
        let sub = volume
            .read(
                &mut src2,
                "X:\\Docs\\DEEP".into(),
                &["Docs".into(), "DEEP".into()],
                &cancel,
                &mut |_, _| {},
            )
            .unwrap();
        assert_eq!(sub.root_path(), "X:\\Docs\\DEEP");
        assert_eq!(sub.total_files(0), 3);
    }
}
