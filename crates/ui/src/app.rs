//! Application state and the per-frame pump. Views borrow this struct and
//! call its methods; worker threads report through channels drained at the
//! top of every frame.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Context, TextureHandle};
use oxtree_core::cleanup::{Cleanup, OutputPolicy, Refresh};
use oxtree_core::cushion::Shading;
use oxtree_core::flat::{flat_list, FlatSort};
use oxtree_core::layout::Tile;
use oxtree_core::mime::Categorizer;
use oxtree_core::{format, Kind, NodeId, Tree};
use oxtree_native::run::RunEvent;
use oxtree_native::{ScanEvent, ScanHandle, ScanOptions};

use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Column {
    Name,
    PercentBar,
    PercentNum,
    Size,
    Items,
    Files,
    Subdirs,
    Mtime,
}

#[derive(Clone, Copy, Debug)]
pub struct Sort {
    pub column: Column,
    pub ascending: bool,
}

/// Which view the central panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Tree,
    Files,
}

/// The File View's list and its controls.
pub struct FileViewState {
    pub rows: Vec<NodeId>,
    pub dirty: bool,
    pub sort: FlatSort,
    pub ascending: bool,
    /// Largest N to show; 0 is all.
    pub max: usize,
    pub include_dirs: bool,
    pub scroll_to_selected: bool,
}

impl Default for FileViewState {
    fn default() -> Self {
        FileViewState {
            rows: Vec::new(),
            dirty: true,
            sort: FlatSort::Size,
            ascending: false,
            max: 1_000,
            include_dirs: false,
            scroll_to_selected: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub node: NodeId,
    pub depth: u32,
}

pub struct RenderResult {
    pub generation: u64,
    pub width: usize,
    pub height: usize,
    pub tiles: Arc<Vec<Tile>>,
    pub pixels: Vec<u8>,
}

#[derive(Default)]
pub struct TreemapState {
    pub texture: Option<TextureHandle>,
    pub texture_size: (usize, usize),
    pub tiles: Arc<Vec<Tile>>,
    pub job: Option<Receiver<RenderResult>>,
    /// Bumped by zoom, reset and Rebuild Treemap; a texture is current only
    /// when `rendered_generation` matches.
    pub generation: u64,
    pub rendered_generation: u64,
    pub hover: Option<NodeId>,
    /// The size the view wants and when it last changed, for the rebuild delay.
    pub wanted_size: (usize, usize),
    pub size_changed_at: Option<Instant>,
    pub job_started_at: Option<Instant>,
    pub rebuilds: u64,
}

pub struct OutputWindow {
    pub title: String,
    pub lines: Vec<(bool, String)>,
    pub events: Option<Receiver<RunEvent>>,
    pub exit: Option<Option<i32>>,
    pub open: bool,
    pub refresh: Refresh,
    pub policy: OutputPolicy,
}

/// View state remembered across a rescan, by path because node ids change.
pub struct ViewMemory {
    pub selected: Option<String>,
    pub expanded: Vec<String>,
    pub treemap_root: String,
}

pub struct PendingCleanup {
    pub index: usize,
    pub node: NodeId,
}

pub struct StatusLine {
    pub text: String,
    pub until: Option<Instant>,
}

pub struct OxTreeApp {
    pub tree: Option<Arc<Tree>>,
    pub scan: Option<ScanHandle>,
    pub scan_path: Option<PathBuf>,
    pub scan_started: Instant,
    pub progress: (usize, usize, u64),
    pub last_elapsed: Option<Duration>,
    pub unreadable: usize,

    pub selected: Option<NodeId>,
    pub expanded: HashSet<NodeId>,
    pub rows: Vec<Row>,
    pub rows_dirty: bool,
    pub scroll_to_selected: bool,
    /// Set by a refresh; applied when the new tree lands.
    pub view_memory: Option<ViewMemory>,
    pub sort: Sort,
    pub view: View,
    pub file_view: FileViewState,

    pub treemap_root: NodeId,
    pub treemap: TreemapState,
    pub show_treemap: bool,
    pub show_details: bool,

    pub cleanups: Vec<Cleanup>,
    pub confirm: Option<PendingCleanup>,
    pub output: Option<OutputWindow>,
    pub trash_job: Option<Receiver<Result<String, String>>>,

    pub open_dir_text: Option<String>,
    pub dir_picker: Option<Receiver<Option<String>>>,
    pub cache_dialog: Option<(bool, String)>,
    pub about_open: bool,
    pub error: Option<String>,
    pub status: StatusLine,

    pub utc_offset: i32,
    pub categorizer: Arc<Categorizer>,
    pub desktop: String,
    pub shading: Shading,
    pub threads: usize,
    /// Enter other local filesystems when scanning. Off by default, as in
    /// QDirStat; `OXTREE_CROSS_FILESYSTEMS=1` until it is a setting.
    pub cross_filesystems: bool,

    pub screenshot: Option<(PathBuf, u32)>,
    pub quit_after_screenshot: bool,

    /// How long the treemap size has to hold still before a rebuild. Zero,
    /// the default, rebuilds every frame during a resize: measured at about
    /// 120% of one core for the duration of a drag, which is fine. QDirStat
    /// waits 200 ms; `OXTREE_REBUILD_DELAY_MS` sets that here.
    pub rebuild_delay: Duration,
    /// `OXTREE_STATS=1` prints rebuild counts and process CPU time once a second.
    pub stats: Option<Stats>,
}

pub struct Stats {
    pub window_start: Instant,
    pub cpu_ms_at_start: u64,
    pub rebuilds_in_window: u64,
}

/// User plus system CPU time of this process in milliseconds, from
/// /proc/self/stat. Assumes the usual 100 clock ticks per second.
pub fn process_cpu_ms() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    let mut fields = after_comm.split_whitespace();
    let utime: u64 = fields.nth(11)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some((utime + stime) * 10)
}

impl OxTreeApp {
    pub fn new(
        start: Option<PathBuf>,
        cache: Option<PathBuf>,
        screenshot: Option<PathBuf>,
    ) -> OxTreeApp {
        let (cleanups, note) = oxtree_native::config::load_cleanups();
        let mut app = OxTreeApp {
            tree: None,
            scan: None,
            scan_path: None,
            scan_started: Instant::now(),
            progress: (0, 0, 0),
            last_elapsed: None,
            unreadable: 0,
            selected: None,
            expanded: HashSet::new(),
            rows: Vec::new(),
            rows_dirty: true,
            scroll_to_selected: false,
            view_memory: None,
            view: View::Tree,
            file_view: FileViewState::default(),
            sort: Sort {
                column: Column::Size,
                ascending: false,
            },
            treemap_root: 0,
            treemap: TreemapState::default(),
            show_treemap: true,
            show_details: true,
            cleanups,
            confirm: None,
            output: None,
            trash_job: None,
            open_dir_text: None,
            dir_picker: None,
            cache_dialog: None,
            about_open: false,
            error: None,
            status: StatusLine {
                text: String::new(),
                until: None,
            },
            utc_offset: oxtree_native::localtime::utc_offset_secs(),
            categorizer: Arc::new(Categorizer::with_defaults()),
            desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            shading: Shading::default(),
            threads: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(16),
            cross_filesystems: std::env::var_os("OXTREE_CROSS_FILESYSTEMS")
                .is_some_and(|v| v == "1" || v == "true"),
            screenshot: screenshot.map(|p| (p, 4)),
            quit_after_screenshot: false,
            rebuild_delay: Duration::from_millis(
                std::env::var("OXTREE_REBUILD_DELAY_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            ),
            stats: std::env::var_os("OXTREE_STATS").map(|_| Stats {
                window_start: Instant::now(),
                cpu_ms_at_start: process_cpu_ms().unwrap_or(0),
                rebuilds_in_window: 0,
            }),
        };
        if let Some(n) = note {
            app.set_status(n, false);
        }
        if let Some(c) = cache {
            app.read_cache(c);
        } else if let Some(p) = start {
            app.open_directory(p);
        } else {
            app.open_dir_text = Some(std::env::var("HOME").unwrap_or_else(|_| "/".into()));
        }
        app
    }

    // ------------------------------------------------------------ status

    pub fn set_status(&mut self, text: impl Into<String>, permanent: bool) {
        self.status = StatusLine {
            text: text.into(),
            until: if permanent {
                None
            } else {
                Some(Instant::now() + theme::STATUS_TIMEOUT)
            },
        };
    }

    pub fn show_error(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.set_status(text.clone(), false);
        self.error = Some(text);
    }

    // ------------------------------------------------------------ scanning

    pub fn open_directory(&mut self, path: PathBuf) {
        self.tree = None;
        self.selected = None;
        self.expanded.clear();
        self.rows.clear();
        self.rows_dirty = true;
        self.treemap = TreemapState::default();
        self.treemap_root = 0;
        self.view_memory = None;
        self.start_scan(path);
    }

    /// Rescan the current directory without touching what is on screen.
    /// The old tree stays until the new one lands; the selection, the
    /// expanded directories and the treemap zoom carry over by path.
    pub fn refresh_all(&mut self) {
        let Some(p) = self.scan_path.clone() else {
            return;
        };
        if let Some(tree) = &self.tree {
            self.view_memory = Some(ViewMemory {
                selected: self.selected.map(|n| tree.path(n)),
                expanded: self.expanded.iter().map(|&n| tree.path(n)).collect(),
                treemap_root: tree.path(self.treemap_root),
            });
        }
        self.start_scan(p);
    }

    fn start_scan(&mut self, path: PathBuf) {
        if let Some(s) = self.scan.take() {
            s.cancel();
        }
        self.progress = (0, 0, 0);
        self.last_elapsed = None;
        self.scan_started = Instant::now();
        self.set_status(format!("Reading {}...", path.display()), true);
        self.scan = Some(oxtree_native::scan(
            &path,
            ScanOptions {
                threads: self.threads,
                cross_filesystems: self.cross_filesystems,
                mounts: if self.cross_filesystems {
                    oxtree_native::mounts::MountTable::load()
                } else {
                    Default::default()
                },
                ..Default::default()
            },
        ));
        self.scan_path = Some(path);
    }

    pub fn stop_reading(&mut self) {
        if let Some(s) = &self.scan {
            s.cancel();
        }
    }

    pub fn read_cache(&mut self, path: PathBuf) {
        match oxtree_native::cache_io::read(&path) {
            Ok(tree) => {
                self.scan_path = Some(PathBuf::from(tree.root_path()));
                self.install_tree(tree, None, 0);
                self.set_status(format!("Read cache file {}", path.display()), false);
            }
            Err(e) => self.show_error(format!("Can't read cache file \"{}\": {e}", path.display())),
        }
    }

    pub fn write_cache(&mut self, path: PathBuf) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        match oxtree_native::cache_io::write(&tree, &path) {
            Ok(()) => self.set_status(format!("Wrote cache file {}", path.display()), false),
            Err(e) => self.show_error(format!(
                "ERROR writing cache file \"{}\": {e}",
                path.display()
            )),
        }
    }

    fn install_tree(&mut self, tree: Tree, elapsed: Option<Duration>, unreadable: usize) {
        self.scan = None;
        let root = tree.root();
        self.expanded.clear();
        self.expanded.insert(root);
        match self.view_memory.take() {
            Some(m) => {
                for p in &m.expanded {
                    if let Some(n) = tree.find(p) {
                        self.expanded.insert(n);
                    }
                }
                self.selected = Some(m.selected.map(|p| tree.find_nearest(&p)).unwrap_or(root));
                self.treemap_root = tree.find(&m.treemap_root).unwrap_or(root);
                // Keep the old texture on screen until the new render lands;
                // the tiles belong to the old ids, so drop those.
                self.treemap.tiles = Arc::new(Vec::new());
                self.treemap.generation += 1;
            }
            None => {
                self.selected = Some(root);
                self.treemap_root = root;
                self.treemap = TreemapState::default();
            }
        }
        self.tree = Some(Arc::new(tree));
        self.rows_dirty = true;
        self.file_view.dirty = true;
        self.last_elapsed = elapsed;
        self.unreadable = unreadable;
        match elapsed {
            Some(e) => {
                let mut msg = format!("Finished. Elapsed time: {}", format::elapsed(e));
                if unreadable > 0 {
                    let noun = if unreadable == 1 {
                        "directory"
                    } else {
                        "directories"
                    };
                    msg.push_str(&format!("  [{unreadable} {noun} could not be read]"));
                }
                self.set_status(msg, true);
            }
            None => self.set_status("Finished.", true),
        }
    }

    // ------------------------------------------------------------ per frame

    /// Drain every channel. Called once at the top of each frame.
    pub fn pump(&mut self, ctx: &Context) {
        if let Some(scan) = &self.scan {
            let mut done = None;
            while let Ok(ev) = scan.events.try_recv() {
                match ev {
                    ScanEvent::Progress { dirs, items, bytes } => {
                        self.progress = (dirs, items, bytes);
                    }
                    ScanEvent::Done {
                        tree,
                        elapsed,
                        unreadable,
                    } => done = Some(Ok((tree, elapsed, unreadable))),
                    ScanEvent::Failed(e) => done = Some(Err(e)),
                }
            }
            match done {
                Some(Ok((tree, elapsed, unreadable))) => {
                    self.install_tree(*tree, Some(elapsed), unreadable)
                }
                Some(Err(e)) => {
                    self.scan = None;
                    self.view_memory = None;
                    self.show_error(e);
                }
                None => {
                    let (dirs, items, bytes) = self.progress;
                    self.status.text = format!(
                        "Reading... {} items in {} directories, {}",
                        format::grouped(items as u64),
                        format::grouped(dirs as u64),
                        format::size(bytes)
                    );
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
        }

        if let Some(job) = &self.treemap.job {
            if let Ok(result) = job.try_recv() {
                self.treemap.job = None;
                if result.generation == self.treemap.generation {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [result.width, result.height],
                        &result.pixels,
                    );
                    match &mut self.treemap.texture {
                        Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
                        None => {
                            self.treemap.texture = Some(ctx.load_texture(
                                "treemap",
                                image,
                                egui::TextureOptions::NEAREST,
                            ))
                        }
                    }
                    self.treemap.texture_size = (result.width, result.height);
                    self.treemap.tiles = result.tiles;
                    self.treemap.rendered_generation = result.generation;
                    self.treemap.rebuilds += 1;
                    if let Some(st) = &mut self.stats {
                        st.rebuilds_in_window += 1;
                    }
                }
            }
        }
        if let Some(st) = &mut self.stats {
            let elapsed = st.window_start.elapsed();
            if elapsed >= Duration::from_secs(1) {
                let cpu = process_cpu_ms().unwrap_or(0);
                let used = cpu.saturating_sub(st.cpu_ms_at_start);
                if st.rebuilds_in_window > 0 || used > 0 {
                    eprintln!(
                        "stats: {} treemap rebuilds in {:.1} s, process cpu {} ms ({:.0}% of one core)",
                        st.rebuilds_in_window,
                        elapsed.as_secs_f64(),
                        used,
                        used as f64 / elapsed.as_secs_f64() / 10.0
                    );
                }
                st.window_start = Instant::now();
                st.cpu_ms_at_start = cpu;
                st.rebuilds_in_window = 0;
            }
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        if let Some(rx) = &self.dir_picker {
            match rx.try_recv() {
                Ok(Some(path)) => {
                    self.dir_picker = None;
                    self.open_dir_text = None;
                    self.open_directory(PathBuf::from(path));
                }
                Ok(None) => self.dir_picker = None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.dir_picker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(200))
                }
            }
        }

        if let Some(rx) = &self.trash_job {
            if let Ok(result) = rx.try_recv() {
                self.trash_job = None;
                match result {
                    Ok(path) => {
                        self.set_status(format!("Moved to trash: {path}"), false);
                        self.refresh_all();
                    }
                    Err(e) => self.show_error(format!("Move to trash failed: {e}")),
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        let mut refresh: Option<Refresh> = None;
        if let Some(out) = &mut self.output {
            if let Some(rx) = &out.events {
                let mut finished = false;
                while let Ok(ev) = rx.try_recv() {
                    match ev {
                        RunEvent::Stdout(l) => out.lines.push((false, l)),
                        RunEvent::Stderr(l) => out.lines.push((true, l)),
                        RunEvent::Finished(code) => {
                            out.exit = Some(code);
                            finished = true;
                        }
                        RunEvent::Failed(e) => {
                            out.lines.push((true, e));
                            out.exit = Some(None);
                            finished = true;
                        }
                    }
                }
                if finished {
                    out.events = None;
                    let had_errors =
                        out.lines.iter().any(|(err, _)| *err) || out.exit != Some(Some(0));
                    // ShowNever means never, as in QDirStat; errors still
                    // reach the status bar below.
                    if out.policy != OutputPolicy::Never
                        && (had_errors || out.policy == OutputPolicy::Always)
                    {
                        out.open = true;
                    }
                    refresh = Some(out.refresh);
                } else {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
            }
        }
        if let Some(r) = refresh {
            let ok = self
                .output
                .as_ref()
                .map(|o| o.exit == Some(Some(0)))
                .unwrap_or(false);
            if ok {
                self.set_status("Cleanup action finished successfully.", false);
            } else {
                let errors = self
                    .output
                    .as_ref()
                    .map(|o| o.lines.iter().filter(|(e, _)| *e).count())
                    .unwrap_or(0);
                self.set_status(
                    format!("Cleanup action finished with {errors} errors."),
                    false,
                );
            }
            if r != Refresh::No {
                // ponytail: any refresh policy rescans the whole tree; a
                // subtree rescan spliced into the arena is the upgrade when
                // scans are slow enough to notice.
                self.refresh_all();
            }
        }

        if let Some(until) = self.status.until {
            if Instant::now() >= until {
                self.status.until = None;
                self.status.text = self.default_status();
            } else {
                ctx.request_repaint_after(until.saturating_duration_since(Instant::now()));
            }
        }

        if self.rows_dirty {
            self.rebuild_rows();
        }
    }

    fn default_status(&self) -> String {
        match (&self.tree, self.selected) {
            (Some(tree), Some(n)) => format!("{}  {}", tree.path(n), format::size(tree.size(n))),
            _ => String::new(),
        }
    }

    // ------------------------------------------------------------ tree rows

    pub fn rebuild_rows(&mut self) {
        self.rows_dirty = false;
        self.rows.clear();
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let mut stack: Vec<(NodeId, u32)> = vec![(tree.root(), 0)];
        let sort = self.sort;
        while let Some((node, depth)) = stack.pop() {
            self.rows.push(Row { node, depth });
            if !self.expanded.contains(&node) {
                continue;
            }
            let mut kids: Vec<NodeId> = tree.children(node).collect();
            sort_children(&tree, &mut kids, sort);
            for k in kids.into_iter().rev() {
                stack.push((k, depth + 1));
            }
        }
    }

    pub fn select(&mut self, node: NodeId, from_treemap: bool) {
        self.selected = Some(node);
        if from_treemap {
            if let Some(tree) = &self.tree {
                let mut p = tree.parent(node);
                while let Some(a) = p {
                    self.expanded.insert(a);
                    p = tree.parent(a);
                }
                self.rows_dirty = true;
                self.scroll_to_selected = true;
            }
        }
        self.status.text = self.default_status();
        self.status.until = None;
    }

    pub fn toggle_expand(&mut self, node: NodeId) {
        if !self.expanded.remove(&node) {
            self.expanded.insert(node);
        }
        self.rows_dirty = true;
    }

    pub fn set_expanded(&mut self, node: NodeId, expanded: bool) {
        let changed = if expanded {
            self.expanded.insert(node)
        } else {
            self.expanded.remove(&node)
        };
        if changed {
            self.rows_dirty = true;
        }
    }

    pub fn close_all_levels(&mut self) {
        self.expanded.clear();
        if let Some(tree) = &self.tree {
            self.expanded.insert(tree.root());
            self.selected = Some(tree.root());
        }
        self.rows_dirty = true;
    }

    pub fn set_sort(&mut self, column: Column) {
        if self.sort.column == column {
            self.sort.ascending = !self.sort.ascending;
        } else {
            self.sort = Sort {
                column,
                ascending: matches!(column, Column::Name | Column::Mtime),
            };
        }
        self.rows_dirty = true;
    }

    // ------------------------------------------------------------ file view

    pub fn rebuild_file_view(&mut self) {
        self.file_view.dirty = false;
        self.file_view.rows.clear();
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let fv = &mut self.file_view;
        fv.rows = flat_list(&tree, fv.include_dirs, fv.sort, fv.ascending, fv.max);
    }

    pub fn set_file_sort(&mut self, sort: FlatSort) {
        let fv = &mut self.file_view;
        if fv.sort == sort {
            fv.ascending = !fv.ascending;
        } else {
            fv.sort = sort;
            fv.ascending = matches!(sort, FlatSort::Name);
        }
        fv.dirty = true;
    }

    pub fn row_index_of(&self, node: NodeId) -> Option<usize> {
        self.rows.iter().position(|r| r.node == node)
    }

    // ------------------------------------------------------------ treemap

    pub fn treemap_zoom_in(&mut self) {
        let Some(tree) = &self.tree else { return };
        let Some(sel) = self.selected else { return };
        let target = if tree.is_dir_like(sel) && tree.kind(sel) != Kind::DotEntry {
            sel
        } else {
            tree.parent(sel).unwrap_or(sel)
        };
        if target != self.treemap_root && tree.is_ancestor(self.treemap_root, target) {
            self.treemap_root = target;
            self.treemap.generation += 1;
        }
    }

    pub fn treemap_zoom_out(&mut self) {
        let Some(tree) = &self.tree else { return };
        if let Some(p) = tree.parent(self.treemap_root) {
            self.treemap_root = if tree.kind(p) == Kind::DotEntry {
                tree.parent(p).unwrap_or(p)
            } else {
                p
            };
            self.treemap.generation += 1;
        }
    }

    pub fn treemap_zoom_reset(&mut self) {
        if self.treemap_root != 0 {
            self.treemap_root = 0;
            self.treemap.generation += 1;
        }
    }

    pub fn treemap_rebuild(&mut self) {
        self.treemap.generation += 1;
    }

    /// Start a layout and render job for the given pixel size if none is
    /// running and the texture is stale.
    pub fn request_treemap(&mut self, ctx: &Context, width: usize, height: usize) {
        if self.treemap.job.is_some() || width == 0 || height == 0 {
            return;
        }
        let fresh = self.treemap.texture.is_some()
            && self.treemap.texture_size == (width, height)
            && self.treemap.rendered_generation == self.treemap.generation;
        if fresh {
            return;
        }
        if (width, height) != self.treemap.wanted_size {
            self.treemap.wanted_size = (width, height);
            self.treemap.size_changed_at = Some(Instant::now());
        }
        // With a delay set, a size change waits until the size has settled.
        // Zoom and rebuild requests go at once.
        let root_changed = self.treemap.rendered_generation != self.treemap.generation;
        if self.treemap.texture.is_some() && !root_changed {
            if let Some(changed) = self.treemap.size_changed_at {
                let since = changed.elapsed();
                if since < self.rebuild_delay {
                    ctx.request_repaint_after(self.rebuild_delay - since);
                    return;
                }
            }
        }
        if let Some(started) = self.treemap.job_started_at {
            let min_interval = Duration::from_secs(1) / theme::TREEMAP_MAX_REBUILD_HZ;
            let since = started.elapsed();
            if since < min_interval {
                ctx.request_repaint_after(min_interval - since);
                return;
            }
        }
        let Some(tree) = self.tree.clone() else {
            return;
        };
        self.treemap.job_started_at = Some(Instant::now());
        let root = self.treemap_root;
        let generation = self.treemap.generation;
        let categorizer = self.categorizer.clone();
        let shading = self.shading;
        let threads = self.threads;
        let ctx = ctx.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.treemap.job = Some(rx);
        std::thread::Builder::new()
            .name("oxtree-treemap".into())
            .spawn(move || {
                let started = Instant::now();
                let rect = oxtree_core::layout::Rect::new(0.0, 0.0, width as f64, height as f64);
                let tiles = oxtree_core::layout::layout(&tree, root, rect, theme::MIN_TILE_SIZE);
                let color_of = |n: NodeId| {
                    categorizer
                        .category(tree.name(n), tree.kind(n), tree.mode(n))
                        .map(|c| c.color)
                        .unwrap_or(theme::FILE_FILL)
                };
                let pixels = oxtree_core::cushion::render(
                    &tree, &tiles, width, height, &color_of, shading, threads,
                );
                let _ = started;
                let _ = tx.send(RenderResult {
                    generation,
                    width,
                    height,
                    tiles: Arc::new(tiles),
                    pixels,
                });
                ctx.request_repaint();
            })
            .ok();
    }

    // ------------------------------------------------------------ actions

    pub fn copy_path(&mut self, ctx: &Context) {
        let Some((tree, n)) = self.tree.clone().zip(self.selected) else {
            return;
        };
        let path = tree.path(n);
        ctx.copy_text(path.clone());
        self.set_status(format!("Copied to clipboard: {path}"), false);
    }

    pub fn move_to_trash(&mut self) {
        let Some((tree, n)) = self.tree.clone().zip(self.selected) else {
            return;
        };
        if n == tree.root() || tree.kind(n) == Kind::DotEntry || self.trash_job.is_some() {
            return;
        }
        let path = tree.path(n);
        let (tx, rx) = std::sync::mpsc::channel();
        self.trash_job = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(oxtree_native::trash::move_to_trash(&path).map(|()| path));
        });
    }

    pub fn cleanup_applies(&self, index: usize) -> bool {
        let (Some(tree), Some(n), Some(c)) = (&self.tree, self.selected, self.cleanups.get(index))
        else {
            return false;
        };
        c.active && c.works_for(tree.kind(n))
    }

    pub fn request_cleanup(&mut self, index: usize) {
        if !self.cleanup_applies(index) {
            return;
        }
        let Some(n) = self.selected else { return };
        let ask = self
            .cleanups
            .get(index)
            .map(|c| c.ask_for_confirmation)
            .unwrap_or(true);
        if ask {
            self.confirm = Some(PendingCleanup { index, node: n });
        } else {
            self.run_cleanup(index, n);
        }
    }

    pub fn run_cleanup(&mut self, index: usize, node: NodeId) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let Some(c) = self.cleanups.get(index).cloned() else {
            return;
        };
        let path = tree.path(node);
        let name = if node == tree.root() {
            path.clone()
        } else {
            tree.name(node).to_string()
        };
        let dir = tree.dir_path(node);
        let command = c.expand(&path, &name, &dir, &self.desktop);
        let Some(shell) = oxtree_native::run::choose_shell(&c.shell) else {
            self.show_error("No usable shell found for the cleanup action");
            return;
        };
        let rx = oxtree_native::run::run(&shell, &command, &dir);
        self.output = Some(OutputWindow {
            title: format!("{}: {}", c.display_title(), path),
            lines: vec![
                (false, format!("{shell} -c {command}")),
                (false, format!("in {dir}")),
            ],
            events: Some(rx),
            exit: None,
            open: c.output == OutputPolicy::Always,
            refresh: c.refresh,
            policy: c.output,
        });
        self.set_status(format!("Running {}...", c.display_title()), true);
    }

    pub fn go_up(&mut self) {
        let Some((tree, n)) = self.tree.clone().zip(self.selected) else {
            return;
        };
        if let Some(p) = tree.parent(n) {
            let target = if tree.kind(p) == Kind::DotEntry {
                tree.parent(p).unwrap_or(p)
            } else {
                p
            };
            self.select(target, true);
        }
    }

    pub fn go_to_toplevel(&mut self) {
        if self.tree.is_some() {
            self.select(0, true);
        }
    }

    pub fn browse_for_directory(&mut self) {
        if self.dir_picker.is_some() {
            return;
        }
        let start = self.open_dir_text.clone().unwrap_or_else(|| "/".into());
        self.dir_picker = Some(oxtree_native::dialog::pick_directory(&start, &self.desktop));
    }
}

pub fn sort_children(tree: &Tree, kids: &mut [NodeId], sort: Sort) {
    // Children are stored largest first; that is the default and costs nothing.
    if sort.column == Column::Size && !sort.ascending {
        return;
    }
    match sort.column {
        Column::Name => kids.sort_by(|&a, &b| {
            tree.name(a)
                .to_lowercase()
                .cmp(&tree.name(b).to_lowercase())
        }),
        Column::Size | Column::PercentBar | Column::PercentNum => {
            kids.sort_by_key(|&k| tree.size(k))
        }
        Column::Items => kids.sort_by_key(|&k| tree.total_items(k)),
        Column::Files => kids.sort_by_key(|&k| tree.total_files(k)),
        Column::Subdirs => kids.sort_by_key(|&k| tree.total_subdirs(k)),
        Column::Mtime => kids.sort_by_key(|&k| tree.mtime(k)),
    }
    if !sort.ascending {
        kids.reverse();
    }
}
