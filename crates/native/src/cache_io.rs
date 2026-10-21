//! Read and write QDirStat cache files on disk. Files ending in `.gz` go
//! through the system `gzip`, so no compression library is linked.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use oxtree_core::{cache, Tree};

/// Limit on the uncompressed text, so a hostile file cannot exhaust memory.
pub const MAX_BYTES: u64 = 1 << 30;

pub fn read(path: &Path) -> Result<Tree, String> {
    let text = if path.extension().is_some_and(|e| e == "gz") {
        let out = Command::new("gzip")
            .arg("-dc")
            .arg("--")
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Cannot run gzip: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "gzip failed for {}: {}",
                path.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        if out.stdout.len() as u64 > MAX_BYTES {
            return Err(format!(
                "{} is larger than {} bytes uncompressed",
                path.display(),
                MAX_BYTES
            ));
        }
        String::from_utf8_lossy(&out.stdout).into_owned()
    } else {
        let md =
            std::fs::metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
        if md.len() > MAX_BYTES {
            return Err(format!(
                "{} is larger than {} bytes",
                path.display(),
                MAX_BYTES
            ));
        }
        std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?
    };
    let (root, dirs) = cache::parse(&text)?;
    Tree::build(root, dirs)
}

pub fn write(tree: &Tree, path: &Path) -> Result<(), String> {
    let text = cache::write(tree);
    if path.extension().is_some_and(|e| e == "gz") {
        let file = std::fs::File::create(path)
            .map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
        let mut child = Command::new("gzip")
            .arg("-c")
            .stdin(Stdio::piped())
            .stdout(file)
            .spawn()
            .map_err(|e| format!("Cannot run gzip: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| format!("Cannot write to gzip: {e}"))?;
        }
        let status = child.wait().map_err(|e| format!("gzip failed: {e}"))?;
        if !status.success() {
            return Err("gzip failed".into());
        }
        Ok(())
    } else {
        std::fs::write(path, text).map_err(|e| format!("Cannot write {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxtree_core::{DirRecord, FileRecord, Kind};

    #[test]
    fn roundtrip_plain_and_gzip() {
        let dirs = vec![DirRecord {
            parent: None,
            name: "/r".into(),
            own_size: 4096,
            kind: Some(Kind::Dir),
            files: vec![FileRecord {
                name: "a b".into(),
                size: 1024,
                mtime: 7,
                mode: 0o100644,
                kind: Kind::File,
            }],
            ..Default::default()
        }];
        let tree = Tree::build("/r".into(), dirs).unwrap();
        let dir = std::env::temp_dir().join(format!("oxtree-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["t.cache", "t.cache.gz"] {
            let p = dir.join(name);
            write(&tree, &p).unwrap();
            let back = read(&p).unwrap();
            assert_eq!(back.len(), tree.len());
            assert_eq!(back.name(1), "a b");
        }
        assert!(read(&dir.join("missing.cache")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
