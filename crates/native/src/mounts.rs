//! The mount table from /proc/self/mounts, used to decide which other
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
        let text = std::fs::read_to_string("/proc/self/mounts")
            .or_else(|_| std::fs::read_to_string("/etc/mtab"))
            .unwrap_or_default();
        MountTable::parse(&text)
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

/// Every readable local filesystem with the sizes `df` reports, sorted by
/// mount path. `df` runs as a child process; without it the list is empty
/// and the dialog still has its text field.
pub fn volumes() -> Vec<Volume> {
    let table = MountTable::load();
    let out = std::process::Command::new("df")
        .args(["-B1", "--output=target,fstype,size,used,avail"])
        .stdin(std::process::Stdio::null())
        .output();
    let text = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return Vec::new(),
    };
    let mut volumes = parse_df(&text, &table);
    volumes.sort_by(|a, b| a.mount.path.cmp(&b.mount.path));
    volumes
}

/// `df` output: a header, then `target fstype size used avail` per line.
/// The target may contain spaces, so the four numbers are taken from the
/// right.
fn parse_df(text: &str, table: &MountTable) -> Vec<Volume> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.rsplitn(5, ' ').filter(|f| !f.is_empty());
            let avail: u64 = fields.next()?.parse().ok()?;
            let used: u64 = fields.next()?.parse().ok()?;
            let size: u64 = fields.next()?.parse().ok()?;
            let _fstype = fields.next()?;
            let target = fields.next()?.trim();
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
        let df = "Mounted on Type 1B-blocks Used Avail\n\
/ btrfs 1000 400 600\n\
/run tmpfs 100 1 99\n\
/home btrfs 1000 400 600\n\
/mnt/nas nfs4 5000 1 4999\n\
/run/media/me/My Stick exfat 64 32 32\n";
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
    fn the_live_table_parses() {
        let t = MountTable::load();
        assert!(t.mounts.iter().any(|m| m.path == Path::new("/")));
    }
}
