//! The mount table (/proc/self/mounts, `mount` on macOS, drive letters
//! on Windows), used to decide which other
//! filesystems a scan may enter when it is allowed to cross them. Kernel
//! tables (proc, sysfs, cgroup, tmpfs under /run) and network mounts are
//! never worth reading; QDirStat's MountPoints.cpp has the same rules.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    pub path: PathBuf,
    pub device: String,
    pub fs_type: String,
}

impl Mount {
    /// Kernel and pseudo filesystems: the device is not a path, or the
    /// mount lives under /dev, /proc or /sys.
    pub fn is_system(&self) -> bool {
        !self.device.contains('/')
            || self.path.starts_with("/dev")
            || self.path.starts_with("/proc")
            || self.path.starts_with("/sys")
    }

    pub fn is_network(&self) -> bool {
        let t = self.fs_type.to_lowercase();
        t.starts_with("nfs")
            || t.starts_with("cifs")
            || t.starts_with("smb")
            || t == "fuse.sshfs"
            || t == "9p"
            || t == "afs"
    }

    /// Worth reading when crossing filesystems: a real disk, local, mounted.
    pub fn is_readable_storage(&self) -> bool {
        !self.is_system() && !self.is_network() && self.fs_type != "autofs"
    }
}

#[derive(Clone, Debug, Default)]
pub struct MountTable {
    pub mounts: Vec<Mount>,
}

impl MountTable {
    pub fn load() -> MountTable {
        if cfg!(windows) {
            return MountTable {
                mounts: drive_letters()
                    .map(|d| Mount {
                        path: PathBuf::from(format!("{d}:\\")),
                        device: format!("{d}:"),
                        fs_type: String::new(),
                    })
                    .collect(),
            };
        }
        if cfg!(target_os = "macos") {
            let out = std::process::Command::new("mount")
                .stdin(std::process::Stdio::null())
                .output();
            let text = out
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            return MountTable::parse_bsd(&text);
        }
        let text = std::fs::read_to_string("/proc/self/mounts")
            .or_else(|_| std::fs::read_to_string("/etc/mtab"))
            .unwrap_or_default();
        MountTable::parse(&text)
    }

    /// `mount` on macOS: `device on /path (type, options)` per line.
    pub fn parse_bsd(text: &str) -> MountTable {
        let mounts = text
            .lines()
            .filter_map(|line| {
                let (device, rest) = line.split_once(" on ")?;
                let (path, opts) = rest.rsplit_once(" (")?;
                let fs_type = opts.split([',', ')']).next()?.trim().to_string();
                Some(Mount {
                    path: PathBuf::from(path),
                    device: device.to_string(),
                    fs_type,
                })
            })
            .collect();
        MountTable { mounts }
    }

    /// One mount per line: device, path, type, options, dump, pass. Spaces
    /// in paths are written as `\040`.
    pub fn parse(text: &str) -> MountTable {
        let mounts = text
            .lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let device = f.next()?.to_string();
                let path = PathBuf::from(unescape(f.next()?));
                let fs_type = f.next()?.to_string();
                Some(Mount {
                    path,
                    device,
                    fs_type,
                })
            })
            .collect();
        MountTable { mounts }
    }

    /// The mount entry for exactly this path, the last one if several.
    pub fn at(&self, path: &Path) -> Option<&Mount> {
        self.mounts.iter().rev().find(|m| m.path == path)
    }

    /// Whether a directory that turned out to be on another filesystem
    /// should be entered. A boundary with no entry in the table (a btrfs
    /// subvolume that is not itself mounted) is treated as local storage.
    pub fn should_enter(&self, path: &Path) -> bool {
        self.at(path).is_none_or(Mount::is_readable_storage)
    }
}

/// A mounted local filesystem with its sizes, for the Open Directory list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    pub mount: Mount,
    pub size: u64,
    pub used: u64,
    pub avail: u64,
}

/// Available and total bytes of the filesystem holding `path`: `df` on
/// Linux and macOS, `fsutil volume diskfree` on Windows.
pub fn free_space(path: &Path) -> Option<(u64, u64)> {
    if cfg!(windows) {
        let drive = path.to_str()?.get(..2)?.to_string();
        return parse_fsutil(&capture("fsutil", &["volume", "diskfree", &drive])?);
    }
    let args: &[&str] = if cfg!(target_os = "macos") {
        &["-k"]
    } else {
        &["-B1", "--output=avail,size"]
    };
    let out = std::process::Command::new("df")
        .args(args)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut fields = text.lines().nth(1)?.split_whitespace();
    if cfg!(target_os = "macos") {
        let size: u64 = fields.nth(1)?.parse().ok()?;
        let avail: u64 = fields.nth(1)?.parse().ok()?;
        return Some((avail * 1024, size * 1024));
    }
    let avail = fields.next()?.parse().ok()?;
    let size = fields.next()?.parse().ok()?;
    Some((avail, size))
}

/// `fsutil volume diskfree C:` prints `Total free bytes : N (...)` and
/// `Total bytes : N (...)`, with thousands separators on newer Windows.
fn parse_fsutil(text: &str) -> Option<(u64, u64)> {
    let mut avail = None;
    let mut size = None;
    for line in text.lines() {
        let (key, value) = line.split_once(':')?;
        let key = key.trim().to_lowercase();
        let digits: String = value
            .split('(')
            .next()
            .unwrap_or("")
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let n: u64 = digits.parse().ok()?;
        if key == "total free bytes" {
            avail = Some(n);
        } else if key == "total bytes" {
            size = Some(n);
        }
    }
    Some((avail?, size?))
}

fn capture(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Drive letters with something mounted, `C` first.
fn drive_letters() -> impl Iterator<Item = char> {
    ('C'..='Z')
        .chain('A'..='B')
        .filter(|d| std::fs::metadata(format!("{d}:\\")).is_ok())
}

/// Every readable local filesystem with the sizes `df` reports, sorted by
/// mount path. `df` runs as a child process; without it the list is empty
/// and the dialog still has its text field.
pub fn volumes() -> Vec<Volume> {
    let table = MountTable::load();
    if cfg!(windows) {
        return table
            .mounts
            .iter()
            .filter_map(|m| {
                let (avail, size) = free_space(&m.path)?;
                let mut mount = m.clone();
                let info = capture("fsutil", &["fsinfo", "volumeinfo", &m.device]);
                mount.fs_type = info
                    .as_deref()
                    .and_then(|t| t.lines().find(|l| l.starts_with("File System Name")))
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, v)| v.trim().to_string())
                    .unwrap_or_default();
                Some(Volume {
                    mount,
                    size,
                    used: size.saturating_sub(avail),
                    avail,
                })
            })
            .collect();
    }
    if cfg!(target_os = "macos") {
        let text = capture("df", &["-k"]).unwrap_or_default();
        let mut volumes = parse_df_bsd(&text, &table);
        volumes.sort_by(|a, b| a.mount.path.cmp(&b.mount.path));
        return volumes;
    }
    let out = std::process::Command::new("df")
        .args(["-B1", "--output=target,fstype,size,used,avail"])
        .stdin(std::process::Stdio::null())
        .output();
    // df exits 1 when one mount cannot be read (a document portal, a stale
    // network mount) and still prints the others, so the exit code is
    // ignored and the output is what counts.
    let text = match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => return Vec::new(),
    };
    let mut volumes = parse_df(&text, &table);
    volumes.sort_by(|a, b| a.mount.path.cmp(&b.mount.path));
    volumes
}

/// `df` output: a header, then `target fstype size used avail` per line,
/// padded into columns. The target may contain spaces, so the four fields
/// on the right are peeled off one at a time and the rest is the target.
fn parse_df(text: &str, table: &MountTable) -> Vec<Volume> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut rest = line.trim_end();
            let mut take = || -> Option<&str> {
                let r = rest.trim_end();
                let i = r.rfind(char::is_whitespace)?;
                let token = r.get(i + 1..)?;
                rest = r.get(..i)?;
                Some(token)
            };
            let avail: u64 = take()?.parse().ok()?;
            let used: u64 = take()?.parse().ok()?;
            let size: u64 = take()?.parse().ok()?;
            let _fstype = take()?;
            let target = rest.trim();
            let mount = table.at(Path::new(target))?.clone();
            if !mount.is_readable_storage() || size == 0 {
                return None;
            }
            Some(Volume {
                mount,
                size,
                used,
                avail,
            })
        })
        .collect()
}

/// `df -k` on macOS: `Filesystem 1024-blocks Used Available Capacity
/// iused ifree %iused Mounted on`; the mount path is everything after the
/// eighth field and may contain spaces.
fn parse_df_bsd(text: &str, table: &MountTable) -> Vec<Volume> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let _device = f.next()?;
            let size: u64 = f.next()?.parse().ok()?;
            let used: u64 = f.next()?.parse().ok()?;
            let avail: u64 = f.next()?.parse().ok()?;
            let target = f.skip(4).collect::<Vec<_>>().join(" ");
            let mount = table.at(Path::new(&target))?.clone();
            if !mount.is_readable_storage() || size == 0 {
                return None;
            }
            Some(Volume {
                mount,
                size: size * 1024,
                used: used * 1024,
                avail: avail * 1024,
            })
        })
        .collect()
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let digits: String = chars.clone().take(3).collect();
        if digits.len() == 3 {
            if let Ok(v) = u8::from_str_radix(&digits, 8) {
                out.push(v as char);
                chars.nth(2);
                continue;
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test -p oxtree-native -- --ignored --nocapture volumes_on_this_machine`
    #[test]
    #[ignore]
    fn volumes_on_this_machine() {
        let v = volumes();
        for vol in &v {
            println!(
                "{} {} {}",
                vol.mount.path.display(),
                vol.mount.fs_type,
                vol.size
            );
        }
        assert!(!v.is_empty(), "df listed nothing readable");
    }

    const SAMPLE: &str = "\
/dev/nvme0n1p3 / btrfs rw,subvol=/root 0 0
devtmpfs /dev devtmpfs rw 0 0
tmpfs /run tmpfs rw 0 0
sysfs /sys sysfs rw 0 0
proc /proc proc rw 0 0
/dev/nvme0n1p3 /home btrfs rw,subvol=/home 0 0
/dev/nvme0n1p1 /boot/efi vfat rw 0 0
nas:/export /mnt/nas nfs4 rw 0 0
//server/share /mnt/smb cifs rw 0 0
systemd-1 /proc/sys/fs/binfmt_misc autofs rw 0 0
/dev/sdb1 /run/media/me/My\\040Stick exfat rw 0 0
";

    #[test]
    fn classifies_mounts_like_qdirstat() {
        let t = MountTable::parse(SAMPLE);
        assert_eq!(t.mounts.len(), 11);
        assert!(t.should_enter(Path::new("/home")));
        assert!(t.should_enter(Path::new("/boot/efi")));
        assert!(!t.should_enter(Path::new("/proc")));
        assert!(!t.should_enter(Path::new("/sys")));
        assert!(!t.should_enter(Path::new("/run")));
        assert!(!t.should_enter(Path::new("/dev")));
        assert!(!t.should_enter(Path::new("/mnt/nas")));
        assert!(!t.should_enter(Path::new("/mnt/smb")));
        assert!(!t.should_enter(Path::new("/proc/sys/fs/binfmt_misc")));
        assert!(t.should_enter(Path::new("/run/media/me/My Stick")));
        assert!(t.should_enter(Path::new("/home/me/subvolume-without-entry")));
    }

    #[test]
    fn df_lines_join_the_mount_table() {
        let t = MountTable::parse(SAMPLE);
        // Padded into columns as df prints it, numbers right-aligned.
        let df = "Mounted on              Type       1B-blocks  Used Avail\n\
/                       btrfs           1000   400   600\n\
/run                    tmpfs            100     1    99\n\
/home                   btrfs           1000   400   600\n\
/mnt/nas                nfs4            5000     1  4999\n\
/run/media/me/My Stick  exfat             64    32    32\n";
        let v = parse_df(df, &t);
        let paths: Vec<String> = v
            .iter()
            .map(|v| v.mount.path.display().to_string())
            .collect();
        assert_eq!(paths, vec!["/", "/home", "/run/media/me/My Stick"]);
        assert_eq!(v[0].used, 400);
        assert_eq!(v[2].size, 64);
    }

    #[test]
    fn bsd_mount_and_df_parse() {
        let t = MountTable::parse_bsd(
            "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)\n\
devfs on /dev (devfs, local, nobrowse)\n\
/dev/disk3s5 on /System/Volumes/Data (apfs, local, journaled, nobrowse)\n\
map auto_home on /System/Volumes/Data/home (autofs, automounted, nobrowse)\n\
//me@nas/share on /Volumes/share (smbfs, nodev, nosuid, mounted by me)\n\
/dev/disk5s1 on /Volumes/My Stick (exfat, local, nodev, nosuid, noowners)\n",
        );
        assert_eq!(t.mounts.len(), 6);
        assert!(t.should_enter(Path::new("/Volumes/My Stick")));
        assert!(!t.should_enter(Path::new("/dev")));
        assert!(!t.should_enter(Path::new("/Volumes/share")));
        assert!(!t.should_enter(Path::new("/System/Volumes/Data/home")));
        let df = "Filesystem 1024-blocks Used Available Capacity iused ifree %iused Mounted on\n\
/dev/disk3s1s1 971350180 10000000 500000000 2% 400000 5000000000 0% /\n\
devfs 200 200 0 100% 700 0 100% /dev\n\
/dev/disk5s1 62000 31000 31000 50% 0 0 100% /Volumes/My Stick\n";
        let v = parse_df_bsd(df, &t);
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].mount.path, Path::new("/Volumes/My Stick"));
        assert_eq!(v[1].size, 62000 * 1024);
        assert_eq!(v[0].avail, 500_000_000 * 1024);
    }

    #[test]
    fn fsutil_diskfree_parses_with_and_without_separators() {
        let old = "Total # of free bytes        : 123456789\nTotal # of bytes             : 999999999\nTotal # of avail free bytes  : 123456789\n";
        assert_eq!(parse_fsutil(old), None, "the old wording is not read");
        let new = "Total free bytes                :  61,234,567,168 ( 57.0 GB)\nTotal bytes                     : 511,054,434,304 (475.9 GB)\nTotal quota free bytes          :  61,234,567,168 ( 57.0 GB)\n";
        assert_eq!(parse_fsutil(new), Some((61_234_567_168, 511_054_434_304)));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn the_live_table_parses() {
        let t = MountTable::load();
        assert!(t.mounts.iter().any(|m| m.path == Path::new("/")));
    }
}
