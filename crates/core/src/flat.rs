//! The flat file list behind the File View: every file, and optionally
//! every directory, of the tree in one order, capped at the largest N.
//! WizTree's File View and its "Top 1000" are this list.

use crate::tree::{NodeId, Tree};
use crate::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlatSort {
    Size,
    Name,
    Mtime,
}

/// Node ids in the requested order. `max` of 0 means all of them. The root
/// and `<Files>` dot entries are never listed; directories only when asked.
pub fn flat_list(
    tree: &Tree,
    include_dirs: bool,
    sort: FlatSort,
    ascending: bool,
    max: usize,
) -> Vec<NodeId> {
    flat_list_filtered(tree, include_dirs, sort, ascending, max, |_| true)
}

/// `flat_list` keeping only the nodes `keep` accepts, before the cap.
pub fn flat_list_filtered(
    tree: &Tree,
    include_dirs: bool,
    sort: FlatSort,
    ascending: bool,
    max: usize,
    keep: impl Fn(NodeId) -> bool,
) -> Vec<NodeId> {
    let mut ids: Vec<NodeId> = (0..tree.len() as NodeId)
        .filter(|&n| n != tree.root())
        .filter(|&n| match tree.kind(n) {
            Kind::DotEntry => false,
            k if k.is_dir_like() => include_dirs,
            _ => true,
        })
        .filter(|&n| keep(n))
        .collect();
    match sort {
        FlatSort::Size => ids.sort_unstable_by_key(|&n| tree.size(n)),
        FlatSort::Mtime => ids.sort_unstable_by_key(|&n| tree.mtime(n)),
        FlatSort::Name => ids.sort_by_cached_key(|&n| tree.name(n).to_lowercase()),
    }
    if !ascending {
        ids.reverse();
    }
    if max > 0 {
        ids.truncate(max);
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::sample;

    #[test]
    fn largest_first_with_cap_and_optional_directories() {
        let t = sample();
        let files = flat_list(&t, false, FlatSort::Size, false, 0);
        let names: Vec<&str> = files.iter().map(|&n| t.name(n)).collect();
        assert_eq!(names, vec!["d", "b.bin", "a.txt", "c.bin"]);

        let top2 = flat_list(&t, false, FlatSort::Size, false, 2);
        assert_eq!(top2.len(), 2);
        assert_eq!(t.name(top2[0]), "d");

        let with_dirs = flat_list(&t, true, FlatSort::Size, false, 0);
        let names: Vec<&str> = with_dirs.iter().map(|&n| t.name(n)).collect();
        // sub is 4096 own plus 350 of files, more than d alone
        assert_eq!(names, vec!["big", "sub", "d", "b.bin", "a.txt", "c.bin"]);
        assert!(!with_dirs.contains(&t.root()));
        assert!(with_dirs.iter().all(|&n| t.kind(n) != Kind::DotEntry));

        let by_name = flat_list(&t, false, FlatSort::Name, true, 0);
        let names: Vec<&str> = by_name.iter().map(|&n| t.name(n)).collect();
        assert_eq!(names, vec!["a.txt", "b.bin", "c.bin", "d"]);
    }
}
