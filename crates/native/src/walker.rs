//! Parallel directory walk. Workers take directories from a shared queue,
//! read them with `std::fs::read_dir`, stat every entry relative to the
//! directory handle (which is what std does on Linux) and push
//! subdirectories back on the queue. The arena is assembled once at the end
//! via `Tree::build`.
//!
//! The queue is one mutex-protected vector. On a 14 core laptop it gives
//! 3.9x on /usr and 13x on a home directory against one thread. A
//! per-worker deque with stealing is the next step if the mutex ever shows
//! up in a profile.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use oxtree_core::{DirRecord, FileRecord, Kind, Tree};

use crate::meta;
use crate::mounts::MountTable;

#[derive(Clone, Debug)]
pub struct ScanOptions {
    /// Enter directories on other filesystems. QDirStat's default is off.
    pub cross_filesystems: bool,
    /// Which other filesystems to enter when crossing: real local storage
    /// only, never kernel tables or network mounts. Loaded from
    /// /proc/self/mounts when `cross_filesystems` is set.
    pub mounts: MountTable,
    /// Directory names never entered. QDirStat's default rule is `.snapshot`.
    pub exclude_names: Vec<String>,
    /// Worker threads; 0 means one per core, capped at 16.
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            cross_filesystems: false,
            mounts: MountTable::default(),
            exclude_names: vec![".snapshot".into()],
            threads: 0,
        }
    }
}

pub enum ScanEvent {
    Progress {
        dirs: usize,
        items: usize,
        bytes: u64,
    },
    Done {
        tree: Box<Tree>,
        elapsed: Duration,
        unreadable: usize,
    },
    Failed(String),
}

pub struct ScanHandle {
    pub events: Receiver<ScanEvent>,
    cancel: Arc<AtomicBool>,
}

impl ScanHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct Job {
    path: PathBuf,
    id: usize,
    parent: usize,
    /// Device id of this directory; a child on another device is a boundary.
    dev: u64,
}

struct Shared {
    queue: Mutex<Vec<Job>>,
    cv: Condvar,
    pending: AtomicUsize,
    next_id: AtomicUsize,
    items: AtomicUsize,
    bytes: AtomicUsize,
    dirs_done: AtomicUsize,
    cancel: Arc<AtomicBool>,
}

/// Something to call after every event so the UI wakes up at once instead
/// of on its next poll. The UI sets it once its context exists, which may
/// be after the scan started; until then it is a no-op.
type WakeFn = Box<dyn Fn() + Send + Sync>;

#[derive(Clone, Default)]
pub struct Wake(Arc<Mutex<Option<WakeFn>>>);

impl Wake {
    pub fn set(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
    }

    pub fn wake(&self) {
        if let Some(f) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            f();
        }
    }
}

/// Start scanning `root` on background threads. Progress and the finished
/// tree arrive on the returned channel; `wake` is called after each.
pub fn scan(root: &Path, options: ScanOptions, wake: Wake) -> ScanHandle {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = ScanHandle {
        events: rx,
        cancel: cancel.clone(),
    };
    let root = root.to_path_buf();
    std::thread::Builder::new()
        .name("oxtree-scan".into())
        .spawn(move || run_scan(root, options, tx, wake, cancel))
        .ok();
    handle
}

/// Scan synchronously on the calling thread's helpers. Used by the headless
/// benchmark and tests.
pub fn scan_blocking(root: &Path, options: ScanOptions) -> Result<(Tree, Duration), String> {
    let handle = scan(root, options, Wake::default());
    loop {
        match handle.events.recv() {
            Ok(ScanEvent::Done { tree, elapsed, .. }) => return Ok((*tree, elapsed)),
            Ok(ScanEvent::Failed(e)) => return Err(e),
            Ok(ScanEvent::Progress { .. }) => {}
            Err(_) => return Err("scan thread stopped without a result".into()),
        }
    }
}

fn run_scan(
    root: PathBuf,
    options: ScanOptions,
    tx: Sender<ScanEvent>,
    wake: Wake,
    cancel: Arc<AtomicBool>,
) {
    let started = Instant::now();
    let send = |ev: ScanEvent| {
        let _ = tx.send(ev);
        wake.wake();
    };
    let root_md = match std::fs::symlink_metadata(&root) {
        Ok(md) if md.is_dir() => md,
        Ok(_) => {
            send(ScanEvent::Failed(format!(
                "{} is not a directory",
                root.display()
            )));
            return;
        }
        Err(e) => {
            send(ScanEvent::Failed(format!(
                "Cannot read {}: {e}",
                root.display()
            )));
            return;
        }
    };
    // On Windows an NTFS volume is read from its MFT instead of walked;
    // anything else, or no administrator rights, falls through to the walk.
    let mut last_report = Instant::now();
    let mut report = |items: usize, bytes: u64| {
        if last_report.elapsed() >= Duration::from_millis(333) {
            last_report = Instant::now();
            send(ScanEvent::Progress {
                dirs: 0,
                items,
                bytes,
            });
        }
    };
    if let Some(result) = crate::mft::scan(&root, &cancel, &mut report) {
        match result {
            Ok(tree) => send(ScanEvent::Done {
                tree: Box::new(tree),
                elapsed: started.elapsed(),
                unreadable: 0,
            }),
            Err(e) => send(ScanEvent::Failed(e)),
        }
        return;
    }
    let root_dev = meta::dev(&root_md);
    let threads = if options.threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(16)
    } else {
        options.threads
    };

    let shared = Shared {
        queue: Mutex::new(vec![Job {
            path: root.clone(),
            id: 0,
            parent: usize::MAX,
            dev: root_dev,
        }]),
        cv: Condvar::new(),
        pending: AtomicUsize::new(1),
        next_id: AtomicUsize::new(1),
        items: AtomicUsize::new(0),
        bytes: AtomicUsize::new(0),
        dirs_done: AtomicUsize::new(0),
        cancel,
    };
    let results: Mutex<Vec<(usize, DirRecord)>> = Mutex::new(Vec::new());

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| worker(&shared, &options, &results));
        }
        // Progress three times a second, like QDirStat's UpdateTimerMillisec,
        // but notice completion within a few milliseconds.
        let mut last_report = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(5));
            let pending = shared.pending.load(Ordering::Acquire);
            let finished = pending == 0 || shared.cancel.load(Ordering::Relaxed);
            if finished || last_report.elapsed() >= Duration::from_millis(333) {
                last_report = Instant::now();
                send(ScanEvent::Progress {
                    dirs: shared.dirs_done.load(Ordering::Relaxed),
                    items: shared.items.load(Ordering::Relaxed),
                    bytes: shared.bytes.load(Ordering::Relaxed) as u64,
                });
            }
            if finished {
                shared.cv.notify_all();
                break;
            }
        }
    });

    if shared.cancel.load(Ordering::Relaxed) {
        send(ScanEvent::Failed("Reading aborted.".into()));
        return;
    }

    // Records arrive in completion order; id order is discovery order and
    // therefore parent before child, which `Tree::build` requires.
    let mut recs = results.into_inner().unwrap_or_else(|e| e.into_inner());
    recs.sort_by_key(|(id, _)| *id);
    let sorted: Vec<DirRecord> = recs.into_iter().map(|(_, r)| r).collect();
    let unreadable = sorted
        .iter()
        .filter(|r| r.kind == Some(Kind::UnreadDir))
        .count();
    let root_name = root.to_string_lossy().into_owned();
    match Tree::build(root_name, sorted) {
        Ok(tree) => {
            send(ScanEvent::Done {
                tree: Box::new(tree),
                elapsed: started.elapsed(),
                unreadable,
            });
        }
        Err(e) => {
            send(ScanEvent::Failed(e));
        }
    }
}

fn worker(shared: &Shared, options: &ScanOptions, results: &Mutex<Vec<(usize, DirRecord)>>) {
    let mut local: Vec<(usize, DirRecord)> = Vec::new();
    loop {
        let job = {
            let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if shared.cancel.load(Ordering::Relaxed) {
                    break None;
                }
                if let Some(j) = q.pop() {
                    break Some(j);
                }
                if shared.pending.load(Ordering::Acquire) == 0 {
                    break None;
                }
                q = shared.cv.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        let Some(job) = job else { break };
        let read = read_dir(shared, options, &job);
        shared.items.fetch_add(
            read.record.files.len() + read.record.subdirs.len(),
            Ordering::Relaxed,
        );
        shared.bytes.fetch_add(
            read.record.files.iter().map(|f| f.size).sum::<u64>() as usize,
            Ordering::Relaxed,
        );
        shared.dirs_done.fetch_add(1, Ordering::Relaxed);
        local.push((job.id, read.record));
        local.extend(read.skipped);
        if !read.jobs.is_empty() {
            shared.pending.fetch_add(read.jobs.len(), Ordering::AcqRel);
            let mut q = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            q.extend(read.jobs);
            shared.cv.notify_all();
        }
        if shared.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
            shared.cv.notify_all();
        }
    }
    results
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .append(&mut local);
}

struct DirRead {
    record: DirRecord,
    jobs: Vec<Job>,
    /// Excluded directories and mount points: finished records that never
    /// become jobs.
    skipped: Vec<(usize, DirRecord)>,
}

fn read_dir(shared: &Shared, options: &ScanOptions, job: &Job) -> DirRead {
    let is_root = job.parent == usize::MAX;
    let name = if is_root {
        job.path.to_string_lossy().into_owned()
    } else {
        job.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut record = DirRecord {
        parent: if is_root { None } else { Some(job.parent) },
        name,
        kind: Some(Kind::Dir),
        ..Default::default()
    };
    if let Ok(md) = std::fs::symlink_metadata(&job.path) {
        record.mtime = meta::mtime(&md);
        record.mode = meta::mode(&md);
        record.own_size = meta::allocated(&md);
    }
    let mut read = DirRead {
        record,
        jobs: Vec::new(),
        skipped: Vec::new(),
    };
    let rd = match std::fs::read_dir(&job.path) {
        Ok(rd) => rd,
        Err(_) => {
            read.record.kind = Some(Kind::UnreadDir);
            return read;
        }
    };
    for entry in rd.flatten() {
        let Ok(md) = entry.metadata() else { continue };
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if md.is_dir() {
            let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
            read.record.subdirs.push(id);
            let excluded = options.exclude_names.contains(&entry_name);
            let boundary = meta::dev(&md) != job.dev;
            let other_fs = boundary
                && !(options.cross_filesystems && options.mounts.should_enter(&entry.path()));
            if excluded || other_fs {
                let kind = if excluded {
                    Kind::UnreadDir
                } else {
                    Kind::MountPoint
                };
                read.skipped.push((
                    id,
                    DirRecord {
                        parent: Some(job.id),
                        name: entry_name,
                        mtime: meta::mtime(&md),
                        mode: meta::mode(&md),
                        own_size: meta::allocated(&md),
                        kind: Some(kind),
                        ..Default::default()
                    },
                ));
            } else {
                read.jobs.push(Job {
                    path: entry.path(),
                    id,
                    parent: job.id,
                    dev: meta::dev(&md),
                });
            }
        } else {
            let kind = if md.file_type().is_symlink() {
                Kind::Symlink
            } else if md.is_file() {
                Kind::File
            } else {
                Kind::Special
            };
            // A file with several hard links is on disk once. Each link
            // gets an equal share, so the totals add up to one copy
            // whichever link the walk met first; QDirStat does the same.
            let links = if kind == Kind::File {
                meta::nlink(&md).max(1)
            } else {
                1
            };
            read.record.files.push(FileRecord {
                name: entry_name,
                size: meta::allocated(&md) / links,
                mtime: meta::mtime(&md),
                mode: meta::mode(&md),
                kind,
            });
        }
    }
    read
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_a_small_tree_with_many_threads() {
        let dir = std::env::temp_dir().join(format!("oxtree-walker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::create_dir_all(dir.join(".snapshot/x")).unwrap();
        std::fs::write(dir.join("top.txt"), vec![0u8; 5000]).unwrap();
        std::fs::write(dir.join("a/mid.bin"), vec![0u8; 70_000]).unwrap();
        std::fs::write(dir.join("a/b/deep"), vec![0u8; 1000]).unwrap();
        std::fs::write(dir.join(".snapshot/x/hidden"), vec![0u8; 1000]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("top.txt", dir.join("link")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file("top.txt", dir.join("link")).unwrap();
        std::fs::hard_link(dir.join("a/mid.bin"), dir.join("a/b/mid-link.bin")).unwrap();

        let (tree, _) = scan_blocking(
            &dir,
            ScanOptions {
                threads: 8,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(tree.root_path(), dir.to_string_lossy());
        let names: Vec<String> = tree.children(0).map(|c| tree.name(c).to_string()).collect();
        assert!(
            names.contains(&"a".to_string())
                && names.contains(&".snapshot".to_string())
                && names.contains(&"<Files>".to_string()),
            "{names:?}"
        );
        let snap = tree
            .children(0)
            .find(|&c| tree.name(c) == ".snapshot")
            .unwrap();
        assert_eq!(tree.kind(snap), Kind::UnreadDir);
        assert_eq!(tree.child_count(snap), 0);
        let a = tree.children(0).find(|&c| tree.name(c) == "a").unwrap();
        assert!(
            tree.size(a) >= 71_000 && tree.size(a) < 2 * 71_000,
            "{}",
            tree.size(a)
        );
        assert_eq!(tree.total_files(a), 3);
        let b = tree.children(a).find(|&c| tree.name(c) == "b").unwrap();
        let mid = tree.children(a).find(|&c| tree.name(c) == "mid.bin");
        let dot = tree.children(a).find(|&c| tree.kind(c) == Kind::DotEntry);
        let mid = mid
            .or_else(|| dot.and_then(|d| tree.children(d).find(|&c| tree.name(c) == "mid.bin")))
            .unwrap();
        let link = tree
            .children(b)
            .find(|&c| tree.name(c) == "mid-link.bin")
            .unwrap();
        assert_eq!(tree.size(mid), tree.size(link), "each hard link gets half");
        let dot = tree
            .children(0)
            .find(|&c| tree.kind(c) == Kind::DotEntry)
            .unwrap();
        let link = tree
            .children(dot)
            .find(|&c| tree.name(c) == "link")
            .unwrap();
        assert_eq!(tree.kind(link), Kind::Symlink);
        assert_eq!(tree.path(link), dir.join("link").to_string_lossy());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_root_fails_cleanly() {
        let Err(err) = scan_blocking(Path::new("/definitely/not/here"), ScanOptions::default())
        else {
            panic!("scan of a missing root succeeded")
        };
        assert!(err.contains("Cannot read"));
    }
}
