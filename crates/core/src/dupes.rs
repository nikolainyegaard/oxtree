//! Duplicate files the way WizTree finds them: the same name and size, or
//! the same name, size and modification time. Contents are never compared.

use std::collections::HashMap;

use crate::tree::{NodeId, Tree};
use crate::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DupMode {
    #[default]
    Off,
    NameSize,
    NameSizeDate,
}

impl DupMode {
    pub const ALL: [(DupMode, &'static str); 3] = [
        (DupMode::Off, "Off"),
        (DupMode::NameSize, "Name and size"),
        (DupMode::NameSizeDate, "Name, size and date"),
    ];
}

/// For every node, how many files share its key: 1 for a file without a
/// duplicate, 0 for anything that is not a plain file. Empty when the mode
/// is off.
pub fn duplicate_counts(tree: &Tree, mode: DupMode) -> Vec<u32> {
    if mode == DupMode::Off {
        return Vec::new();
    }
    let key = |n: NodeId| {
        let date = if mode == DupMode::NameSizeDate {
            tree.mtime(n)
        } else {
            0
        };
        (tree.name(n), tree.size(n), date)
    };
    let mut groups: HashMap<(&str, u64, i64), u32> = HashMap::new();
    for n in 0..tree.len() as NodeId {
        if tree.kind(n) == Kind::File {
            let c = groups.entry(key(n)).or_insert(0);
            *c = c.saturating_add(1);
        }
    }
    (0..tree.len() as NodeId)
        .map(|n| {
            if tree.kind(n) == Kind::File {
                groups.get(&key(n)).copied().unwrap_or(1)
            } else {
                0
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{DirRecord, FileRecord};

    fn file(name: &str, size: u64, mtime: i64) -> FileRecord {
        FileRecord {
            name: name.into(),
            size,
            mtime,
            mode: 0o100644,
            kind: Kind::File,
        }
    }

    #[test]
    fn counts_files_sharing_name_and_size() {
        let dirs = vec![
            DirRecord {
                parent: None,
                name: "r".into(),
                kind: Some(Kind::Dir),
                files: vec![file("a", 10, 1), file("b", 20, 1)],
                subdirs: vec![1],
                ..Default::default()
            },
            DirRecord {
                parent: Some(0),
                name: "s".into(),
                kind: Some(Kind::Dir),
                files: vec![file("a", 10, 2), file("b", 21, 1)],
                ..Default::default()
            },
        ];
        let t = Tree::build("/r".into(), dirs).unwrap();
        let by_name = |counts: &[u32], name: &str| -> Vec<u32> {
            (0..t.len() as NodeId)
                .filter(|&n| t.name(n) == name)
                .map(|n| counts[n as usize])
                .collect()
        };
        let c = duplicate_counts(&t, DupMode::NameSize);
        assert_eq!(by_name(&c, "a"), vec![2, 2]);
        assert_eq!(by_name(&c, "b"), vec![1, 1]);
        assert_eq!(c[0], 0, "directories count nothing");
        let c = duplicate_counts(&t, DupMode::NameSizeDate);
        assert_eq!(by_name(&c, "a"), vec![1, 1], "different dates");
        assert!(duplicate_counts(&t, DupMode::Off).is_empty());
    }
}
