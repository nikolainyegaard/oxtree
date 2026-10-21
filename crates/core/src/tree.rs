//! The scanned directory tree as a struct-of-arrays arena.
//!
//! Every node is an index. A directory's children occupy one contiguous
//! block, already sorted by size, largest first, so the treemap layout and
//! the tree view never sort. Names live in one string; a node's name is a
//! slice of it. Sizes are allocated sizes (st_blocks * 512), which is what
//! the views show.

use std::ops::Range;

pub type NodeId = u32;
pub const NO_NODE: NodeId = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Kind {
    Dir,
    /// The `<Files>` pseudo directory holding a directory's plain files when
    /// that directory also has subdirectories.
    DotEntry,
    File,
    Symlink,
    /// Block or character device, FIFO, socket.
    Special,
    /// A directory that could not be read (permission denied or error).
    UnreadDir,
    /// A directory on another filesystem that was not entered.
    MountPoint,
}

impl Kind {
    pub fn is_dir_like(self) -> bool {
        matches!(
            self,
            Kind::Dir | Kind::DotEntry | Kind::UnreadDir | Kind::MountPoint
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Dir => "Directory",
            Kind::DotEntry => "Pseudo Directory",
            Kind::File => "File",
            Kind::Symlink => "Symbolic Link",
            Kind::Special => "Special File",
            Kind::UnreadDir => "Directory",
            Kind::MountPoint => "Mount Point",
        }
    }
}

/// One non-directory entry as the scanner or the cache parser saw it.
#[derive(Clone, Debug)]
pub struct FileRecord {
    pub name: String,
    /// Allocated size in bytes.
    pub size: u64,
    pub mtime: i64,
    pub mode: u32,
    pub kind: Kind,
}

/// One directory as the scanner or the cache parser saw it. Records are
/// numbered in discovery order, so a parent's id is always smaller than
/// its children's ids.
#[derive(Clone, Debug, Default)]
pub struct DirRecord {
    pub parent: Option<usize>,
    pub name: String,
    pub mtime: i64,
    pub mode: u32,
    /// Allocated size of the directory inode itself.
    pub own_size: u64,
    /// `Kind::Dir`, `Kind::UnreadDir` or `Kind::MountPoint`.
    pub kind: Option<Kind>,
    pub files: Vec<FileRecord>,
    pub subdirs: Vec<usize>,
}

pub struct Tree {
    root_path: String,
    parent: Vec<NodeId>,
    first_child: Vec<NodeId>,
    child_count: Vec<u32>,
    size: Vec<u64>,
    /// Own mtime for files, latest mtime in the subtree for directories.
    mtime: Vec<i64>,
    mode: Vec<u32>,
    kind: Vec<Kind>,
    items: Vec<u32>,
    files: Vec<u32>,
    subdirs: Vec<u32>,
    /// `name_off` has one more entry than there are nodes.
    name_off: Vec<u32>,
    names: String,
}

#[derive(Clone, Copy)]
enum Entry {
    Sub(usize),
    File(usize),
    Dot,
}

impl Tree {
    /// Assemble the arena from directory records. Fails when a record points
    /// at a parent with a higher id than itself or at a missing one.
    pub fn build(root_path: String, dirs: Vec<DirRecord>) -> Result<Tree, String> {
        if dirs.is_empty() {
            return Err("nothing was read".into());
        }
        let n = dirs.len();
        for (id, d) in dirs.iter().enumerate() {
            match d.parent {
                None if id == 0 => {}
                None => return Err(format!("directory record {id} has no parent")),
                Some(p) if p >= id => return Err(format!("directory record {id} has parent {p}")),
                Some(_) => {}
            }
        }

        // Totals bottom-up: children always have higher ids than parents.
        let mut total = vec![0u64; n];
        let mut items = vec![0u32; n];
        let mut files = vec![0u32; n];
        let mut subdirs = vec![0u32; n];
        let mut latest = vec![0i64; n];
        for id in (0..n).rev() {
            let Some(d) = dirs.get(id) else { continue };
            let mut t = d.own_size;
            let mut it = (d.files.len() + d.subdirs.len()) as u32;
            let mut f = d.files.len() as u32;
            let mut s = d.subdirs.len() as u32;
            let mut m = d.mtime;
            for fr in &d.files {
                t = t.saturating_add(fr.size);
                m = m.max(fr.mtime);
            }
            for &sub in &d.subdirs {
                t = t.saturating_add(total.get(sub).copied().unwrap_or(0));
                it = it.saturating_add(items.get(sub).copied().unwrap_or(0));
                f = f.saturating_add(files.get(sub).copied().unwrap_or(0));
                s = s.saturating_add(subdirs.get(sub).copied().unwrap_or(0));
                m = m.max(latest.get(sub).copied().unwrap_or(0));
            }
            set(&mut total, id, t);
            set(&mut items, id, it);
            set(&mut files, id, f);
            set(&mut subdirs, id, s);
            set(&mut latest, id, m);
        }

        let node_count = dirs.iter().map(|d| d.files.len() + 2).sum::<usize>();
        let mut tree = Tree {
            root_path,
            parent: Vec::with_capacity(node_count),
            first_child: Vec::with_capacity(node_count),
            child_count: Vec::with_capacity(node_count),
            size: Vec::with_capacity(node_count),
            mtime: Vec::with_capacity(node_count),
            mode: Vec::with_capacity(node_count),
            kind: Vec::with_capacity(node_count),
            items: Vec::with_capacity(node_count),
            files: Vec::with_capacity(node_count),
            subdirs: Vec::with_capacity(node_count),
            name_off: vec![0],
            names: String::new(),
        };
        let mut dir_node = vec![NO_NODE; n];
        {
            let root = dirs.first().ok_or("nothing was read")?;
            let id = tree.push(
                &root.name,
                total.first().copied().unwrap_or(0),
                latest.first().copied().unwrap_or(0),
                root.mode,
                root.kind.unwrap_or(Kind::Dir),
                NO_NODE,
            );
            tree.set_stats(
                id,
                items.first().copied().unwrap_or(0),
                files.first().copied().unwrap_or(0),
                subdirs.first().copied().unwrap_or(0),
            );
            set(&mut dir_node, 0, id);
        }

        let mut entries: Vec<(u64, Entry)> = Vec::new();
        for id in 0..n {
            let Some(d) = dirs.get(id) else { continue };
            let me = dir_node.get(id).copied().unwrap_or(NO_NODE);
            if me == NO_NODE {
                return Err(format!("directory record {id} was never reached"));
            }
            entries.clear();
            let files_size: u64 = d.files.iter().map(|f| f.size).fold(0, u64::saturating_add);
            for &sub in &d.subdirs {
                entries.push((total.get(sub).copied().unwrap_or(0), Entry::Sub(sub)));
            }
            if d.subdirs.is_empty() {
                for (i, f) in d.files.iter().enumerate() {
                    entries.push((f.size, Entry::File(i)));
                }
            } else if !d.files.is_empty() {
                entries.push((files_size, Entry::Dot));
            }
            sort_desc(&mut entries, |e| match e {
                Entry::Sub(s) => dirs.get(*s).map(|r| r.name.as_str()).unwrap_or(""),
                Entry::File(i) => d.files.get(*i).map(|r| r.name.as_str()).unwrap_or(""),
                Entry::Dot => "",
            });
            let first = tree.len() as NodeId;
            tree.set_children(me, first, entries.len() as u32);
            let mut dot = NO_NODE;
            for &(size, e) in &entries {
                match e {
                    Entry::Sub(s) => {
                        let Some(r) = dirs.get(s) else { continue };
                        let node = tree.push(
                            &r.name,
                            size,
                            latest.get(s).copied().unwrap_or(0),
                            r.mode,
                            r.kind.unwrap_or(Kind::Dir),
                            me,
                        );
                        tree.set_stats(
                            node,
                            items.get(s).copied().unwrap_or(0),
                            files.get(s).copied().unwrap_or(0),
                            subdirs.get(s).copied().unwrap_or(0),
                        );
                        set(&mut dir_node, s, node);
                    }
                    Entry::File(i) => {
                        let Some(f) = d.files.get(i) else { continue };
                        tree.push(&f.name, f.size, f.mtime, f.mode, f.kind, me);
                    }
                    Entry::Dot => {
                        let m = d.files.iter().map(|f| f.mtime).max().unwrap_or(0);
                        dot = tree.push("<Files>", size, m, 0o040000, Kind::DotEntry, me);
                        tree.set_stats(dot, d.files.len() as u32, d.files.len() as u32, 0);
                    }
                }
            }
            if dot != NO_NODE {
                let mut fe: Vec<(u64, Entry)> = d
                    .files
                    .iter()
                    .enumerate()
                    .map(|(i, f)| (f.size, Entry::File(i)))
                    .collect();
                sort_desc(&mut fe, |e| match e {
                    Entry::File(i) => d.files.get(*i).map(|r| r.name.as_str()).unwrap_or(""),
                    _ => "",
                });
                let first = tree.len() as NodeId;
                tree.set_children(dot, first, fe.len() as u32);
                for &(_, e) in &fe {
                    if let Entry::File(i) = e {
                        if let Some(f) = d.files.get(i) {
                            tree.push(&f.name, f.size, f.mtime, f.mode, f.kind, dot);
                        }
                    }
                }
            }
        }
        Ok(tree)
    }

    fn push(
        &mut self,
        name: &str,
        size: u64,
        mtime: i64,
        mode: u32,
        kind: Kind,
        parent: NodeId,
    ) -> NodeId {
        let id = self.len() as NodeId;
        self.parent.push(parent);
        self.first_child.push(NO_NODE);
        self.child_count.push(0);
        self.size.push(size);
        self.mtime.push(mtime);
        self.mode.push(mode);
        self.kind.push(kind);
        self.items.push(0);
        self.files.push(0);
        self.subdirs.push(0);
        self.names.push_str(name);
        self.name_off.push(self.names.len() as u32);
        id
    }

    fn set_children(&mut self, node: NodeId, first: NodeId, count: u32) {
        set(&mut self.first_child, node as usize, first);
        set(&mut self.child_count, node as usize, count);
    }

    fn set_stats(&mut self, node: NodeId, items: u32, files: u32, subdirs: u32) {
        set(&mut self.items, node as usize, items);
        set(&mut self.files, node as usize, files);
        set(&mut self.subdirs, node as usize, subdirs);
    }

    pub fn len(&self) -> usize {
        self.size.len()
    }

    pub fn is_empty(&self) -> bool {
        self.size.is_empty()
    }

    pub fn root(&self) -> NodeId {
        0
    }

    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    pub fn name(&self, n: NodeId) -> &str {
        if n == 0 {
            return &self.root_path;
        }
        let a = self.name_off.get(n as usize).copied().unwrap_or(0) as usize;
        let b = self.name_off.get(n as usize + 1).copied().unwrap_or(0) as usize;
        self.names.get(a..b).unwrap_or("")
    }

    pub fn parent(&self, n: NodeId) -> Option<NodeId> {
        match self.parent.get(n as usize).copied() {
            Some(NO_NODE) | None => None,
            Some(p) => Some(p),
        }
    }

    /// Children, largest first.
    pub fn children(&self, n: NodeId) -> Range<NodeId> {
        let first = self.first_child.get(n as usize).copied().unwrap_or(NO_NODE);
        if first == NO_NODE {
            return 0..0;
        }
        let count = self.child_count.get(n as usize).copied().unwrap_or(0);
        first..first.saturating_add(count)
    }

    pub fn child_count(&self, n: NodeId) -> u32 {
        self.child_count.get(n as usize).copied().unwrap_or(0)
    }

    pub fn size(&self, n: NodeId) -> u64 {
        self.size.get(n as usize).copied().unwrap_or(0)
    }

    pub fn mtime(&self, n: NodeId) -> i64 {
        self.mtime.get(n as usize).copied().unwrap_or(0)
    }

    pub fn mode(&self, n: NodeId) -> u32 {
        self.mode.get(n as usize).copied().unwrap_or(0)
    }

    pub fn kind(&self, n: NodeId) -> Kind {
        self.kind.get(n as usize).copied().unwrap_or(Kind::File)
    }

    pub fn is_dir_like(&self, n: NodeId) -> bool {
        self.kind(n).is_dir_like()
    }

    pub fn total_items(&self, n: NodeId) -> u32 {
        self.items.get(n as usize).copied().unwrap_or(0)
    }

    pub fn total_files(&self, n: NodeId) -> u32 {
        self.files.get(n as usize).copied().unwrap_or(0)
    }

    pub fn total_subdirs(&self, n: NodeId) -> u32 {
        self.subdirs.get(n as usize).copied().unwrap_or(0)
    }

    pub fn depth(&self, mut n: NodeId) -> u32 {
        let mut d = 0;
        while let Some(p) = self.parent(n) {
            d += 1;
            n = p;
        }
        d
    }

    /// Percentage of the parent's size, or `None` for the root.
    pub fn percent(&self, n: NodeId) -> Option<f64> {
        let p = self.parent(n)?;
        let total = self.size(p);
        if total == 0 {
            return Some(0.0);
        }
        Some(self.size(n) as f64 * 100.0 / total as f64)
    }

    /// Full filesystem path. Dot entries do not appear in paths; they share
    /// their parent directory's path.
    pub fn path(&self, n: NodeId) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut cur = n;
        loop {
            if cur == 0 {
                break;
            }
            if self.kind(cur) != Kind::DotEntry {
                parts.push(self.name(cur));
            }
            match self.parent(cur) {
                Some(p) => cur = p,
                None => break,
            }
        }
        let mut out = self.root_path.clone();
        for part in parts.iter().rev() {
            if !out.ends_with('/') {
                out.push('/');
            }
            out.push_str(part);
        }
        out
    }

    /// The node for a full path as `path()` produces it, or `None`. Used to
    /// carry view state across a rescan, where every node id changes.
    pub fn find(&self, path: &str) -> Option<NodeId> {
        let rest = path.strip_prefix(self.root_path.as_str())?;
        let mut cur: NodeId = 0;
        for part in rest.split('/').filter(|p| !p.is_empty()) {
            cur = self.child_named(cur, part)?;
        }
        Some(cur)
    }

    /// The nearest node on the path that still exists: the node itself,
    /// else its parent, and so on up to the root.
    pub fn find_nearest(&self, path: &str) -> NodeId {
        let mut p = path;
        loop {
            if let Some(n) = self.find(p) {
                return n;
            }
            match p.rfind('/') {
                Some(i) if i > 0 => p = p.get(..i).unwrap_or(""),
                _ => return 0,
            }
        }
    }

    /// A direct child by name, looking inside the `<Files>` dot entry too
    /// since paths skip it.
    fn child_named(&self, dir: NodeId, name: &str) -> Option<NodeId> {
        let mut dot = None;
        for c in self.children(dir) {
            if self.kind(c) == Kind::DotEntry {
                dot = Some(c);
            } else if self.name(c) == name {
                return Some(c);
            }
        }
        let dot = dot?;
        self.children(dot).find(|&c| self.name(c) == name)
    }

    /// The directory a cleanup runs in: the node itself for directories, the
    /// parent directory otherwise.
    pub fn dir_path(&self, n: NodeId) -> String {
        if self.is_dir_like(n) {
            return self.path(n);
        }
        match self.parent(n) {
            Some(p) => self.path(p),
            None => self.path(n),
        }
    }

    /// Whether `ancestor` is on the parent chain of `n` (or equal to it).
    pub fn is_ancestor(&self, ancestor: NodeId, mut n: NodeId) -> bool {
        loop {
            if n == ancestor {
                return true;
            }
            match self.parent(n) {
                Some(p) => n = p,
                None => return false,
            }
        }
    }

    /// Iterate the whole subtree depth-first, parent before children.
    pub fn walk(&self, root: NodeId, mut f: impl FnMut(NodeId)) {
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            f(n);
            for c in self.children(n).rev() {
                stack.push(c);
            }
        }
    }
}

fn set<T>(v: &mut [T], i: usize, value: T) {
    if let Some(slot) = v.get_mut(i) {
        *slot = value;
    }
}

fn sort_desc<'a>(entries: &mut [(u64, Entry)], name: impl Fn(&Entry) -> &'a str) {
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| name(&a.1).cmp(name(&b.1))));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> FileRecord {
        FileRecord {
            name: name.into(),
            size,
            mtime: 10,
            mode: 0o100644,
            kind: Kind::File,
        }
    }

    pub(crate) fn sample() -> Tree {
        // /root: a.txt (100), sub/ (b.bin 300, c.bin 50), big/ (d 1000)
        let dirs = vec![
            DirRecord {
                parent: None,
                name: "root".into(),
                mtime: 5,
                mode: 0o040755,
                own_size: 4096,
                kind: Some(Kind::Dir),
                files: vec![file("a.txt", 100)],
                subdirs: vec![1, 2],
            },
            DirRecord {
                parent: Some(0),
                name: "sub".into(),
                mtime: 7,
                mode: 0o040755,
                own_size: 4096,
                kind: Some(Kind::Dir),
                files: vec![file("c.bin", 50), file("b.bin", 300)],
                subdirs: vec![],
            },
            DirRecord {
                parent: Some(0),
                name: "big".into(),
                mtime: 9,
                mode: 0o040755,
                own_size: 4096,
                kind: Some(Kind::Dir),
                files: vec![file("d", 1000)],
                subdirs: vec![],
            },
        ];
        Tree::build("/root".into(), dirs).unwrap()
    }

    #[test]
    fn find_by_path_round_trips_and_falls_back_to_the_nearest_ancestor() {
        let t = sample();
        for n in 0..t.len() as NodeId {
            if t.kind(n) != Kind::DotEntry {
                assert_eq!(t.find(&t.path(n)), Some(n), "{}", t.path(n));
            }
        }
        assert_eq!(t.find("/root"), Some(0));
        assert_eq!(t.find("/root/a.txt").map(|n| t.name(n)), Some("a.txt"));
        assert_eq!(t.find("/root/sub/zzz"), None);
        assert_eq!(t.find("/elsewhere"), None);
        let sub = t.find("/root/sub").unwrap();
        assert_eq!(t.find_nearest("/root/sub/zzz/deeper"), sub);
        assert_eq!(t.find_nearest("/root/gone"), 0);
        assert_eq!(t.find_nearest("/elsewhere"), 0);
    }

    #[test]
    fn totals_and_sorted_children() {
        let t = sample();
        assert_eq!(t.size(0), 4096 * 3 + 100 + 350 + 1000);
        let kids: Vec<&str> = t.children(0).map(|c| t.name(c)).collect();
        assert_eq!(kids, vec!["big", "sub", "<Files>"]);
        let sub = t.children(0).find(|&c| t.name(c) == "sub").unwrap();
        let sub_kids: Vec<&str> = t.children(sub).map(|c| t.name(c)).collect();
        assert_eq!(sub_kids, vec!["b.bin", "c.bin"]);
        assert_eq!(t.total_items(0), 6);
        assert_eq!(t.total_files(0), 4);
        assert_eq!(t.total_subdirs(0), 2);
        assert_eq!(t.mtime(0), 10);
    }

    #[test]
    fn paths_skip_dot_entries() {
        let t = sample();
        let dot = t
            .children(0)
            .find(|&c| t.kind(c) == Kind::DotEntry)
            .unwrap();
        let a = t.children(dot).next().unwrap();
        assert_eq!(t.path(a), "/root/a.txt");
        assert_eq!(t.path(dot), "/root");
        assert_eq!(t.dir_path(a), "/root");
        let big = t.children(0).next().unwrap();
        let d = t.children(big).next().unwrap();
        assert_eq!(t.path(d), "/root/big/d");
        assert_eq!(t.depth(d), 2);
        assert!(t.is_ancestor(0, d));
        assert!(!t.is_ancestor(d, 0));
        assert!((t.percent(big).unwrap() - (5096.0 * 100.0 / t.size(0) as f64)).abs() < 1e-9);
    }

    #[test]
    fn root_slash_has_no_double_slash() {
        let dirs = vec![DirRecord {
            parent: None,
            name: "/".into(),
            own_size: 4096,
            kind: Some(Kind::Dir),
            files: vec![file("x", 1)],
            ..Default::default()
        }];
        let t = Tree::build("/".into(), dirs).unwrap();
        assert_eq!(t.path(1), "/x");
    }

    #[test]
    fn bad_parent_is_an_error() {
        let dirs = vec![
            DirRecord {
                parent: None,
                name: "r".into(),
                kind: Some(Kind::Dir),
                ..Default::default()
            },
            DirRecord {
                parent: Some(1),
                name: "loop".into(),
                kind: Some(Kind::Dir),
                ..Default::default()
            },
        ];
        assert!(Tree::build("/r".into(), dirs).is_err());
    }
}
