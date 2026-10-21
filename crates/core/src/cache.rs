//! QDirStat's cache file format (`doc/cache-file-format.txt` in QDirStat),
//! versions 1.0 and 2.0. Reading produces directory records for
//! `Tree::build`; writing emits version 1.0, which carries exactly what the
//! arena stores: type, path, size and mtime.

use std::collections::HashMap;

use crate::tree::{DirRecord, FileRecord, Kind, Tree};

pub const MAX_LINE: usize = 1024;

/// Parse cache text. Returns the root path and the records.
pub fn parse(text: &str) -> Result<(String, Vec<DirRecord>), String> {
    let mut lines = text.lines().enumerate();
    let version = loop {
        let Some((_, line)) = lines.next() else {
            return Err("not a QDirStat cache file: empty".into());
        };
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let inner = line
            .strip_prefix('[')
            .and_then(|l| l.strip_suffix(']'))
            .ok_or("not a QDirStat cache file: no header")?;
        let mut parts = inner.split_whitespace();
        if parts.next() != Some("qdirstat") {
            return Err("not a QDirStat cache file".into());
        }
        break match parts.next() {
            Some("1.0") => 1,
            Some("2.0") => 2,
            Some(v) => return Err(format!("unsupported cache file version {v}")),
            None => return Err("cache file header has no version".into()),
        };
    };

    let mut dirs: Vec<DirRecord> = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut root_path = String::new();
    let mut current: Option<usize> = None;

    for (i, raw) in lines {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.len() > MAX_LINE {
            return Err(format!("line {}: longer than {MAX_LINE} bytes", i + 1));
        }
        let mut f = line.split_whitespace();
        let kind_s = f.next().unwrap_or("");
        let path = decode(
            f.next()
                .ok_or_else(|| format!("line {}: missing path", i + 1))?,
        );
        let size = parse_size(
            f.next()
                .ok_or_else(|| format!("line {}: missing size", i + 1))?,
        )
        .ok_or_else(|| format!("line {}: bad size", i + 1))?;
        let mut perm = 0u32;
        if version == 2 {
            f.next();
            f.next();
            perm = u32::from_str_radix(f.next().unwrap_or("0"), 8).unwrap_or(0);
        }
        let mtime = parse_mtime(f.next().unwrap_or("0"))
            .ok_or_else(|| format!("line {}: bad mtime", i + 1))?;
        let mut blocks: Option<u64> = None;
        while let Some(key) = f.next() {
            let value = f.next().unwrap_or("0");
            if key.eq_ignore_ascii_case("blocks:") {
                blocks = value.parse().ok();
            }
        }
        let alloc = blocks.map(|b| b * 512).unwrap_or(size.div_ceil(512) * 512);

        let kind = match kind_s.to_ascii_lowercase().as_str() {
            "d" => Kind::Dir,
            "f" => Kind::File,
            "l" => Kind::Symlink,
            "blockdev" | "chardev" | "fifo" | "socket" => Kind::Special,
            other => return Err(format!("line {}: unknown type {other}", i + 1)),
        };

        if kind == Kind::Dir {
            if !path.starts_with('/') {
                return Err(format!("line {}: directory without absolute path", i + 1));
            }
            let parent = if dirs.is_empty() {
                root_path = path.clone();
                None
            } else {
                let parent_path = parent_of(&path);
                Some(
                    *by_path
                        .get(parent_path)
                        .ok_or_else(|| format!("line {}: parent of {path} not seen yet", i + 1))?,
                )
            };
            let id = dirs.len();
            let name = if parent.is_none() {
                path.clone()
            } else {
                path.rsplit('/').next().unwrap_or("").to_string()
            };
            dirs.push(DirRecord {
                parent,
                name,
                mtime,
                mode: 0o040000 | perm,
                own_size: alloc,
                kind: Some(Kind::Dir),
                files: Vec::new(),
                subdirs: Vec::new(),
            });
            if let Some(p) = parent {
                if let Some(pd) = dirs.get_mut(p) {
                    pd.subdirs.push(id);
                }
            }
            by_path.insert(path, id);
            current = Some(id);
            continue;
        }

        let (dir, name) = if let Some(rest) = path.strip_prefix('/') {
            let full = format!("/{rest}");
            let d = *by_path
                .get(parent_of(&full))
                .ok_or_else(|| format!("line {}: directory of {full} not seen yet", i + 1))?;
            (d, full.rsplit('/').next().unwrap_or("").to_string())
        } else {
            (
                current.ok_or_else(|| format!("line {}: file before any directory", i + 1))?,
                path,
            )
        };
        let type_bits = match kind {
            Kind::Symlink => 0o120000,
            Kind::Special => 0o060000,
            _ => 0o100000,
        };
        if let Some(d) = dirs.get_mut(dir) {
            d.files.push(FileRecord {
                name,
                size: alloc,
                mtime,
                mode: type_bits | perm,
                kind,
            });
        }
    }
    if dirs.is_empty() {
        return Err("cache file has no directories".into());
    }
    Ok((root_path, dirs))
}

/// Write the tree in cache format 1.0.
pub fn write(tree: &Tree) -> String {
    let mut out = String::from("[qdirstat 1.0 cache file]\n# Generated by OxTree\n# Do not edit!\n#\n# Type\tpath\t\tsize\tmtime\n\n");
    write_dir(tree, tree.root(), &mut out);
    out
}

fn write_dir(tree: &Tree, dir: u32, out: &mut String) {
    let path = tree.path(dir);
    let own = 4096u64;
    out.push_str(&format!(
        "D {}\t{}\t0x{:x}\n",
        encode(&path),
        own,
        tree.mtime(dir)
    ));
    let mut subdirs = Vec::new();
    let mut files = Vec::new();
    for c in tree.children(dir) {
        match tree.kind(c) {
            Kind::Dir | Kind::UnreadDir | Kind::MountPoint => subdirs.push(c),
            Kind::DotEntry => files.extend(tree.children(c)),
            _ => files.push(c),
        }
    }
    for f in files {
        let t = match tree.kind(f) {
            Kind::Symlink => "L",
            Kind::Special => "Special",
            _ => "F",
        };
        out.push_str(&format!(
            "{t}\t{}\t{}\t0x{:x}\n",
            encode(tree.name(f)),
            tree.size(f),
            tree.mtime(f)
        ));
    }
    for s in subdirs {
        write_dir(tree, s, out);
    }
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => path.get(..i).unwrap_or("/"),
        None => "/",
    }
}

fn parse_size(s: &str) -> Option<u64> {
    let (num, mult) = match s.chars().last()? {
        'K' | 'k' => (s.get(..s.len() - 1)?, 1024u64),
        'M' | 'm' => (s.get(..s.len() - 1)?, 1024 * 1024),
        'G' | 'g' => (s.get(..s.len() - 1)?, 1024 * 1024 * 1024),
        _ => (s, 1),
    };
    num.parse::<u64>().ok()?.checked_mul(mult)
}

fn parse_mtime(s: &str) -> Option<i64> {
    match s.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Percent-decode; bytes are reassembled as UTF-8 with replacement.
pub fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            if let Some(h) = bytes
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(h);
                i += 3;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encode whitespace, `%` and control characters.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c <= ' ' || c == '%' || c == '\x7f' {
            out.push_str(&format!("%{:02X}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: &str = include_str!("../tests/fixtures/qdirstat-cache-v2.txt");

    #[test]
    fn parses_the_documented_example() {
        let (root, dirs) = parse(V2).unwrap();
        assert_eq!(root, "/work/home/sh/src/qdirstat");
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].files.len(), 7);
        assert_eq!(dirs[1].name, "src");
        assert_eq!(dirs[1].parent, Some(0));
        assert_eq!(dirs[0].subdirs, vec![1]);
        let license = dirs[0].files.iter().find(|f| f.name == "LICENSE").unwrap();
        assert_eq!(license.size, 18092u64.div_ceil(512) * 512);
        assert_eq!(license.mtime, 0x4ba94fed);
        assert_eq!(license.mode & 0o777, 0o664);
        let tree = Tree::build(root, dirs).unwrap();
        assert_eq!(tree.total_files(0), 7 + 10);
    }

    #[test]
    fn roundtrip_through_v1() {
        let (root, dirs) = parse(V2).unwrap();
        let tree = Tree::build(root, dirs).unwrap();
        let text = write(&tree);
        assert!(text.starts_with("[qdirstat 1.0 cache file]"));
        let (root2, dirs2) = parse(&text).unwrap();
        let tree2 = Tree::build(root2, dirs2).unwrap();
        assert_eq!(tree.len(), tree2.len());
        assert_eq!(tree.total_files(0), tree2.total_files(0));
        let name = |t: &Tree| {
            t.children(0)
                .map(|c| t.name(c).to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(name(&tree), name(&tree2));
    }

    #[test]
    fn sizes_and_escapes() {
        assert_eq!(parse_size("1K"), Some(1024));
        assert_eq!(parse_size("8G"), Some(8 << 30));
        assert_eq!(parse_size("1025"), Some(1025));
        assert_eq!(parse_size("x"), None);
        assert_eq!(decode("with%20blank%25"), "with blank%");
        assert_eq!(encode("with blank%"), "with%20blank%25");
        assert_eq!(encode("æøå ok"), "æøå%20ok");
        assert!(parse("[qdirstat 3.0 cache file]\n").is_err());
        assert!(parse("hello\n").is_err());
        let text = "[qdirstat 1.0 cache file]\nD /a 4096 0x10\nF x 10 0x11\nD /a/b 4096 0x12\nF /a/b/y%20z 20 13\n";
        let (_, dirs) = parse(text).unwrap();
        assert_eq!(dirs[1].files[0].name, "y z");
        assert_eq!(dirs[1].files[0].mtime, 13);
    }
}
