//! Configuration files under `$XDG_CONFIG_HOME/oxtree`. Only cleanups are
//! stored for now. When OxTree has no cleanups file yet but QDirStat does,
//! QDirStat's is read, since the format is the same.

use std::path::PathBuf;

use oxtree_core::cleanup::{self, Cleanup};
use oxtree_core::ini::Ini;

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("oxtree")
}

pub fn cleanups_path() -> PathBuf {
    config_dir().join("cleanups.conf")
}

fn qdirstat_cleanups_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/QDirStat/QDirStat-cleanup.conf"))
}

/// Load cleanups: ours, else QDirStat's, else the standard set, which is
/// then written so the user has a file to edit.
pub fn load_cleanups() -> (Vec<Cleanup>, Option<String>) {
    if let Ok(text) = std::fs::read_to_string(cleanups_path()) {
        let v = cleanup::from_ini(&Ini::parse(&text));
        if !v.is_empty() {
            return (v, None);
        }
    }
    if let Some(q) = qdirstat_cleanups_path() {
        if let Ok(text) = std::fs::read_to_string(&q) {
            let v = cleanup::from_ini(&Ini::parse(&text));
            if !v.is_empty() {
                return (v, Some(format!("Cleanups read from {}", q.display())));
            }
        }
    }
    let std_set = cleanup::standard();
    let note = save_cleanups(&std_set).err();
    (std_set, note)
}

/// Write through a temporary file and rename, with owner-only permissions
/// on the directory.
pub fn save_cleanups(cleanups: &[Cleanup]) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    set_private(&dir);
    let path = cleanups_path();
    let tmp = dir.join("cleanups.conf.tmp");
    std::fs::write(&tmp, cleanup::to_ini(cleanups).to_text())
        .map_err(|e| format!("Cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Cannot replace {}: {e}", path.display()))
}

fn set_private(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(md) = std::fs::metadata(dir) {
        let mut perms = md.permissions();
        perms.set_mode(0o700);
        let _ = std::fs::set_permissions(dir, perms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_follows_xdg() {
        let dir = config_dir();
        assert!(dir.ends_with("oxtree"));
        assert!(dir.is_absolute() || dir.starts_with("."));
    }
}
