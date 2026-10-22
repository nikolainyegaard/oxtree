//! Which tree rows are open, and why. A row opened by hand stays open. A
//! row opened to reveal an item (a treemap click, a File View jump) is
//! temporary: the next reveal closes every temporary row that is not on
//! its own path, so the tree does not fill up with the paths of everything
//! that was ever clicked. Expanding something by hand inside a temporary
//! path makes that path permanent.

use std::collections::HashSet;

use oxtree_core::{NodeId, Tree};

#[derive(Default, Debug, Clone)]
pub struct Expansion {
    open: HashSet<NodeId>,
    /// The subset of `open` that a reveal opened and nobody touched since.
    temporary: HashSet<NodeId>,
}

impl Expansion {
    pub fn is_open(&self, n: NodeId) -> bool {
        self.open.contains(&n)
    }

    pub fn is_temporary(&self, n: NodeId) -> bool {
        self.temporary.contains(&n)
    }

    pub fn open_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.open.iter().copied()
    }

    pub fn temporary_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.temporary.iter().copied()
    }

    /// Open by hand: permanent, and it makes every temporary ancestor
    /// permanent too, since the user is now working inside that path.
    pub fn open_by_hand(&mut self, tree: &Tree, n: NodeId) -> bool {
        let changed = self.open.insert(n);
        self.temporary.remove(&n);
        let mut p = tree.parent(n);
        while let Some(a) = p {
            self.temporary.remove(&a);
            p = tree.parent(a);
        }
        changed
    }

    /// Close by hand, whatever opened it.
    pub fn close(&mut self, n: NodeId) -> bool {
        self.temporary.remove(&n);
        self.open.remove(&n)
    }

    pub fn toggle_by_hand(&mut self, tree: &Tree, n: NodeId) {
        if self.open.contains(&n) {
            self.close(n);
        } else {
            self.open_by_hand(tree, n);
        }
    }

    /// Open the path to `n` so it is visible, temporarily; close the
    /// temporary rows of the previous reveal that are not on this path.
    /// Rows the user opened by hand are untouched.
    pub fn reveal(&mut self, tree: &Tree, n: NodeId) {
        let mut path = HashSet::new();
        let mut p = tree.parent(n);
        while let Some(a) = p {
            path.insert(a);
            p = tree.parent(a);
        }
        let stale: Vec<NodeId> = self
            .temporary
            .iter()
            .copied()
            .filter(|t| !path.contains(t))
            .collect();
        for t in stale {
            self.temporary.remove(&t);
            self.open.remove(&t);
        }
        for a in path {
            if self.open.insert(a) {
                self.temporary.insert(a);
            }
        }
    }

    /// Only the root open, as at the start.
    pub fn close_all(&mut self, root: NodeId) {
        self.open.clear();
        self.temporary.clear();
        self.open.insert(root);
    }

    /// Restore from node sets, after a rescan: the root is always open.
    pub fn restore(
        &mut self,
        root: NodeId,
        permanent: impl IntoIterator<Item = NodeId>,
        temporary: impl IntoIterator<Item = NodeId>,
    ) {
        self.close_all(root);
        self.open.extend(permanent);
        for t in temporary {
            if self.open.insert(t) {
                self.temporary.insert(t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxtree_core::{DirRecord, FileRecord, Kind};

    fn file(name: &str) -> FileRecord {
        FileRecord {
            name: name.into(),
            size: 10,
            mtime: 0,
            mode: 0o100644,
            kind: Kind::File,
        }
    }

    fn dir(parent: Option<usize>, name: &str, subdirs: Vec<usize>, files: Vec<&str>) -> DirRecord {
        DirRecord {
            parent,
            name: name.into(),
            mtime: 0,
            mode: 0o040755,
            own_size: 4096,
            kind: Some(Kind::Dir),
            files: files.into_iter().map(file).collect(),
            subdirs,
        }
    }

    /// /r: a/ (a1/ (f1), a2/ (f2)), b/ (b1/ (f3))
    fn tree() -> Tree {
        let dirs = vec![
            dir(None, "r", vec![1, 4], vec![]),
            dir(Some(0), "a", vec![2, 3], vec![]),
            dir(Some(1), "a1", vec![], vec!["f1"]),
            dir(Some(1), "a2", vec![], vec!["f2"]),
            dir(Some(0), "b", vec![5], vec![]),
            dir(Some(4), "b1", vec![], vec!["f3"]),
        ];
        Tree::build("/r".into(), dirs).unwrap()
    }

    fn id(t: &Tree, path: &str) -> NodeId {
        t.find(path).unwrap()
    }

    #[test]
    fn a_reveal_undoes_the_previous_reveal_but_keeps_shared_ancestors() {
        let t = tree();
        let mut e = Expansion::default();
        e.close_all(0);
        e.reveal(&t, id(&t, "/r/a/a1/f1"));
        assert!(e.is_open(id(&t, "/r/a")) && e.is_open(id(&t, "/r/a/a1")));
        assert!(e.is_temporary(id(&t, "/r/a/a1")));

        e.reveal(&t, id(&t, "/r/a/a2/f2"));
        assert!(e.is_open(id(&t, "/r/a")), "shared ancestor stays open");
        assert!(!e.is_open(id(&t, "/r/a/a1")), "the old leaf path closes");
        assert!(e.is_open(id(&t, "/r/a/a2")));

        e.reveal(&t, id(&t, "/r/b/b1/f3"));
        assert!(!e.is_open(id(&t, "/r/a")));
        assert!(!e.is_open(id(&t, "/r/a/a2")));
        assert!(e.is_open(id(&t, "/r/b")) && e.is_open(id(&t, "/r/b/b1")));
        assert!(e.is_open(0), "the root never closes");
    }

    #[test]
    fn hand_opened_rows_survive_reveals_and_promote_their_path() {
        let t = tree();
        let mut e = Expansion::default();
        e.close_all(0);
        e.open_by_hand(&t, id(&t, "/r/b"));
        e.reveal(&t, id(&t, "/r/a/a1/f1"));
        e.reveal(&t, id(&t, "/r/a/a2/f2"));
        assert!(
            e.is_open(id(&t, "/r/b")),
            "hand-opened row untouched by reveals"
        );

        // Working inside a revealed path makes it permanent.
        e.reveal(&t, id(&t, "/r/a/a1/f1"));
        e.open_by_hand(&t, id(&t, "/r/a/a1"));
        assert!(!e.is_temporary(id(&t, "/r/a")));
        e.reveal(&t, id(&t, "/r/b/b1/f3"));
        assert!(e.is_open(id(&t, "/r/a")) && e.is_open(id(&t, "/r/a/a1")));

        // Closing by hand closes whatever opened it; toggling reopens by hand.
        e.close(id(&t, "/r/b/b1"));
        assert!(!e.is_open(id(&t, "/r/b/b1")));
        e.toggle_by_hand(&t, id(&t, "/r/b/b1"));
        assert!(e.is_open(id(&t, "/r/b/b1")) && !e.is_temporary(id(&t, "/r/b/b1")));

        e.close_all(0);
        assert_eq!(e.open_nodes().count(), 1);
    }
}
