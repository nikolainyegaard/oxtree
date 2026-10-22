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
use oxtree_core::dupes::DupMode;
use oxtree_core::export::ExportOptions;
use oxtree_core::flat::{flat_list_filtered, FlatSort};
use oxtree_core::layout::Tile;
use oxtree_core::mime::Categorizer;
use oxtree_core::query::{Item, Query};
use oxtree_core::settings::{Settings, Theme};
use oxtree_core::types::TypeRow;
use oxtree_core::{format, Kind, NodeId, Tree, NO_NODE};
use oxtree_native::run::RunEvent;
use oxtree_native::update::UpdateResult;
use oxtree_native::{ScanEvent, ScanHandle, ScanOptions, Wake};

use crate::columns::Columns;
use crate::expansion::Expansion;
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
    Types,
}

/// Sort column of the File Types view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSort {
    Ext,
    Category,
    Count,
    Size,
}

/// The Export to CSV dialog's fields.
#[derive(Clone, Debug)]
pub struct ExportDialog {
    pub root: NodeId,
    pub path: String,
    pub tsv: bool,
    pub max_depth: String,
    pub utc: bool,
    pub percent_of_parent: bool,
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default widths: name, percent bar, percent, size, items, files,
/// subdirs, last modified. Each includes its padding.
pub const TREE_COLUMNS: [f32; 8] = [
    theme::COL_NAME_MIN,
    theme::PERCENT_BAR_COLUMN_WIDTH + theme::COL_GAP,
    theme::COL_PERCENT_NUM + theme::COL_GAP,
    theme::COL_SIZE + theme::COL_GAP,
    theme::COL_COUNT + theme::COL_GAP,
    theme::COL_COUNT + theme::COL_GAP,
    theme::COL_COUNT + theme::COL_GAP,
    theme::COL_MTIME + theme::COL_GAP,
];
/// Path, size, percent, last modified, duplicates, duplicate size.
pub const FILE_COLUMNS: [f32; 6] = [
    theme::COL_PATH_MIN,
    theme::COL_SIZE + theme::COL_GAP,
    theme::COL_PERCENT_NUM + theme::COL_GAP,
    theme::COL_MTIME + theme::COL_GAP,
    theme::COL_COUNT + theme::COL_GAP,
    theme::COL_SIZE + theme::COL_GAP,
];
/// Extension, category, files, size, percent.
pub const TYPES_COLUMNS: [f32; 5] = [
    160.0,
    200.0,
    theme::COL_COUNT + theme::COL_GAP + 20.0,
    theme::COL_SIZE + theme::COL_GAP,
    theme::COL_PERCENT_NUM + theme::COL_GAP,
];

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
    /// The search box text, WizTree's syntax; parsed on every rebuild.
    pub search: String,
    pub search_error: Option<String>,
    pub query: Option<Query>,
    /// Match name terms against the whole path instead of the name.
    pub match_path: bool,
    /// Set by Ctrl+F; the search box takes focus on the next frame.
    pub focus_search: bool,
    /// Duplicates only, grouped, with the two duplicate columns.
    pub dup_mode: DupMode,
    /// The list is built on a worker thread so typing never waits for it.
    /// `dirty` set while a job runs means one more run after it finishes,
    /// so a burst of keystrokes costs at most two builds.
    pub job: Option<Receiver<(u64, Vec<NodeId>)>>,
    pub generation: u64,
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
            search: String::new(),
            search_error: None,
            query: None,
            match_path: false,
            focus_search: false,
            dup_mode: DupMode::Off,
            job: None,
            generation: 0,
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
    /// Width in pixels of the free space strip on the right, 0 for none.
    pub free_px: usize,
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
    pub free_px: usize,
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
    pub temporary: Vec<String>,
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
    /// Directories with an unreadable directory somewhere below them,
    /// shown in the subtree error colour as QDirStat does.
    pub subtree_errors: HashSet<NodeId>,

    /// The current item. Always in `marked` when the selection is more
    /// than one item.
    pub selected: Option<NodeId>,
    /// The selection: one item after a click, more after Ctrl or Shift
    /// clicks. Trash, cleanups and copy act on all of them.
    pub marked: HashSet<NodeId>,
    pub expansion: Expansion,
    pub rows: Vec<Row>,
    pub rows_dirty: bool,
    pub scroll_to_selected: bool,
    /// Set by a refresh; applied when the new tree lands.
    pub view_memory: Option<ViewMemory>,
    pub sort: Sort,
    pub view: View,
    pub file_view: FileViewState,
    pub tree_columns: Columns,
    pub file_columns: Columns,
    pub types_columns: Columns,
    /// The File Types table, built the first time the view is shown.
    pub types: Option<Arc<Vec<TypeRow>>>,
    pub types_sort: (TypeSort, bool),
    /// Duplicate counts per node for the mode they were built for.
    pub dupes: Option<(DupMode, Arc<Vec<u32>>)>,
    pub dupes_job: Option<Receiver<(DupMode, Vec<u32>)>>,
    /// Available and total bytes of the scanned filesystem, from df.
    pub free_space: Option<(u64, u64)>,
    /// The scan root is a mount point, so its free space belongs in the
    /// tree and the treemap; for a subdirectory it only goes in the status bar.
    pub root_is_volume: bool,
    pub free_job: Option<Receiver<Option<(u64, u64)>>>,
    pub show_tile_names: bool,
    pub export_dialog: Option<ExportDialog>,
    /// A cleanup over a multi-selection runs once per item, in turn; these
    /// are the items still to go.
    pub cleanup_queue: Vec<(usize, NodeId)>,

    pub treemap_root: NodeId,
    pub treemap: TreemapState,
    pub show_treemap: bool,
    pub show_details: bool,

    pub cleanups: Vec<Cleanup>,
    pub confirm: Option<PendingCleanup>,
    pub output: Option<OutputWindow>,
    pub trash_job: Option<Receiver<Result<String, String>>>,
    /// A second instance started as root through pkexec; reports when it ends.
    pub root_launch: Option<oxtree_native::root::Launch>,
    pub is_root: bool,

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
    /// Enter other local filesystems when scanning; `[DirectoryTree]
    /// CrossFilesystems`, on by default.
    pub cross_filesystems: bool,
    /// Settings that are not state of a view: see `settings.rs` for the keys.
    pub row_height: f32,
    pub theme: Theme,
    pub size_decimals: u8,
    pub search_history: Vec<String>,
    pub check_updates: bool,
    pub check_interval_days: u32,
    pub last_check: i64,
    pub update_job: Option<Receiver<UpdateResult>>,
    pub update_result: Option<UpdateResult>,
    pub preferences_open: bool,
    /// The window's size each frame, saved with the settings on exit.
    pub window_size: Option<(f32, f32)>,
    /// Called by the scan threads after every event, once the egui context
    /// is attached, so a finished scan is drawn on the next frame rather
    /// than on the next poll.
    pub wake: Wake,
    /// The mount table at the last scan start, for the tree icons and the
    /// details panel.
    pub mounts: oxtree_native::mounts::MountTable,
    /// Directories in the tree that are mount points of filesystems the
    /// scan entered, so the boundary stays visible.
    pub entered_mounts: HashSet<NodeId>,
    /// Loaded when the Open Directory dialog opens, dropped when it closes.
    pub volumes: Option<Vec<oxtree_native::mounts::Volume>>,

    #[cfg(feature = "diagnostics")]
    pub screenshot: Option<(PathBuf, u32)>,
    #[cfg(feature = "diagnostics")]
    pub quit_after_screenshot: bool,

    /// How long the treemap size has to hold still before a rebuild. Zero,
    /// the default, rebuilds every frame during a resize: measured at about
    /// 120% of one core for the duration of a drag, which is fine. QDirStat
    /// waits 200 ms; `[Treemaps] RebuildDelayMillisec` sets that here.
    pub rebuild_delay: Duration,
    /// `OXTREE_STATS=1` prints rebuild counts and process CPU time once a second.
    #[cfg(feature = "diagnostics")]
    pub stats: Option<Stats>,
}

#[cfg(feature = "diagnostics")]
pub struct Stats {
    pub window_start: Instant,
    pub cpu_ms_at_start: u64,
    pub rebuilds_in_window: u64,
}

/// User plus system CPU time of this process in milliseconds, from
/// /proc/self/stat. Assumes the usual 100 clock ticks per second.
#[cfg(feature = "diagnostics")]
pub fn process_cpu_ms() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    let mut fields = after_comm.split_whitespace();
    let utime: u64 = fields.nth(11)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some((utime + stime) * 10)
}

/// Work that starts at `main` entry, before the window: the small reads
/// (cleanups file, `date +%z`) on a thread, and the mount table. The scan
/// itself waits for the window; started earlier it slows the window setup
/// on every core and the window appears later, see performance.md.
pub struct EarlyWork {
    start: Option<PathBuf>,
    mounts: oxtree_native::mounts::MountTable,
    settings: Settings,
    threads: usize,
    config: std::thread::JoinHandle<(Vec<Cleanup>, Option<String>, i32)>,
}

/// Scan workers: one per core, capped at 16.
pub fn scan_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(16)
}

pub fn begin_early(start: Option<PathBuf>, settings: Settings) -> EarlyWork {
    let mounts = oxtree_native::mounts::MountTable::load();
    let config = std::thread::spawn(|| {
        let (cleanups, note) = oxtree_native::config::load_cleanups();
        (cleanups, note, oxtree_native::localtime::utc_offset_secs())
    });
    EarlyWork {
        start,
        mounts,
        settings,
        threads: scan_threads(),
        config,
    }
}

impl OxTreeApp {
    pub fn new(early: EarlyWork, cache: Option<PathBuf>) -> OxTreeApp {
        let EarlyWork {
            start,
            mounts,
            settings,
            threads,
            config,
        } = early;
        let (cleanups, note, utc_offset) = config
            .join()
            .unwrap_or_else(|_| (oxtree_core::cleanup::standard(), None, 0));
        let mut app = OxTreeApp {
            tree: None,
            scan: None,
            scan_path: None,
            scan_started: Instant::now(),
            progress: (0, 0, 0),
            last_elapsed: None,
            unreadable: 0,
            subtree_errors: HashSet::new(),
            selected: None,
            marked: HashSet::new(),
            expansion: Expansion::default(),
            rows: Vec::new(),
            rows_dirty: true,
            scroll_to_selected: false,
            view_memory: None,
            view: View::Tree,
            file_view: FileViewState::default(),
            tree_columns: Columns::new(&TREE_COLUMNS),
            file_columns: Columns::new(&FILE_COLUMNS),
            types_columns: Columns::new(&TYPES_COLUMNS),
            types: None,
            types_sort: (TypeSort::Size, false),
            dupes: None,
            dupes_job: None,
            free_space: None,
            root_is_volume: false,
            free_job: None,
            show_tile_names: true,
            export_dialog: None,
            cleanup_queue: Vec::new(),
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
            root_launch: None,
            is_root: oxtree_native::root::is_root(),
            open_dir_text: None,
            dir_picker: None,
            cache_dialog: None,
            about_open: false,
            error: None,
            status: StatusLine {
                text: String::new(),
                until: None,
            },
            utc_offset,
            categorizer: Arc::new(Categorizer::with_defaults()),
            desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            shading: Shading::default(),
            threads,
            cross_filesystems: true,
            row_height: theme::ROW_HEIGHT,
            theme: Theme::System,
            size_decimals: 1,
            search_history: Vec::new(),
            check_updates: false,
            check_interval_days: 7,
            last_check: 0,
            update_job: None,
            update_result: None,
            preferences_open: false,
            window_size: None,
            wake: Wake::default(),
            mounts,
            entered_mounts: HashSet::new(),
            volumes: None,
            #[cfg(feature = "diagnostics")]
            screenshot: None,
            #[cfg(feature = "diagnostics")]
            quit_after_screenshot: false,
            rebuild_delay: Duration::ZERO,
            #[cfg(feature = "diagnostics")]
            stats: std::env::var_os("OXTREE_STATS").map(|_| Stats {
                window_start: Instant::now(),
                cpu_ms_at_start: process_cpu_ms().unwrap_or(0),
                rebuilds_in_window: 0,
            }),
        };
        app.apply_settings(&settings);
        if app.check_updates
            && unix_now().saturating_sub(app.last_check) >= app.check_interval_days as i64 * 86_400
        {
            app.start_update_check();
        }
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

    // ------------------------------------------------------------ settings

    /// Load the settings into the state they belong to. Column widths are
    /// taken only when the count matches, so an old file after a column
    /// was added keeps the defaults.
    pub fn apply_settings(&mut self, s: &Settings) {
        self.cross_filesystems = s.cross_filesystems;
        self.rebuild_delay = Duration::from_millis(s.rebuild_delay_ms);
        self.show_tile_names = s.tile_names;
        self.show_treemap = s.show_treemap;
        self.show_details = s.show_details;
        self.theme = s.theme;
        self.size_decimals = s.size_decimals;
        format::set_size_decimals(s.size_decimals);
        self.row_height = s.row_height;
        for (cols, saved) in [
            (&mut self.tree_columns, &s.tree_columns),
            (&mut self.file_columns, &s.file_columns),
            (&mut self.types_columns, &s.types_columns),
        ] {
            if saved.len() == cols.widths.len() {
                cols.widths = saved.clone();
            }
        }
        self.file_view.max = s.file_max;
        self.file_view.include_dirs = s.file_folders;
        self.file_view.dup_mode = s.dup_mode;
        self.file_view.match_path = s.match_path;
        self.file_view.dirty = true;
        self.search_history = s.search_history.clone();
        self.check_updates = s.check_updates;
        self.check_interval_days = s.check_interval_days;
        self.last_check = s.last_check;
    }

    pub fn to_settings(&self) -> Settings {
        Settings {
            cross_filesystems: self.cross_filesystems,
            rebuild_delay_ms: self.rebuild_delay.as_millis() as u64,
            tile_names: self.show_tile_names,
            show_treemap: self.show_treemap,
            show_details: self.show_details,
            window: self.window_size,
            theme: self.theme,
            size_decimals: self.size_decimals,
            row_height: self.row_height,
            tree_columns: self.tree_columns.widths.clone(),
            file_columns: self.file_columns.widths.clone(),
            types_columns: self.types_columns.widths.clone(),
            file_max: self.file_view.max,
            file_folders: self.file_view.include_dirs,
            dup_mode: self.file_view.dup_mode,
            match_path: self.file_view.match_path,
            search_history: self.search_history.clone(),
            check_updates: self.check_updates,
            check_interval_days: self.check_interval_days,
            last_check: self.last_check,
        }
    }

    /// Write the settings file; a failure goes to the status bar.
    pub fn save_settings(&mut self) {
        if let Err(e) = oxtree_native::config::save_settings(&self.to_settings()) {
            self.set_status(e, false);
        }
    }

    /// Reset configuration: defaults in the state, in the file and on screen.
    pub fn reset_settings(&mut self, ctx: &Context) {
        let s = Settings::default();
        self.apply_settings(&s);
        self.tree_columns.reset();
        self.file_columns.reset();
        self.types_columns.reset();
        self.apply_theme(ctx);
        self.rows_dirty = true;
        match oxtree_native::config::save_settings(&s) {
            Ok(()) => self.set_status("Configuration reset to the defaults", false),
            Err(e) => self.set_status(e, false),
        }
    }

    pub fn apply_theme(&self, ctx: &Context) {
        ctx.set_theme(match self.theme {
            Theme::System => egui::ThemePreference::System,
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
        });
    }

    /// Remember a search once the user is done typing it.
    pub fn remember_search(&mut self) {
        let mut s = Settings {
            search_history: std::mem::take(&mut self.search_history),
            ..Default::default()
        };
        s.remember_search(&self.file_view.search);
        self.search_history = s.search_history;
    }

    /// Check for Updates, by hand or on schedule. Nothing runs unless this
    /// is called.
    pub fn start_update_check(&mut self) {
        if self.update_job.is_some() {
            return;
        }
        self.last_check = unix_now();
        self.update_result = None;
        self.update_job = Some(oxtree_native::update::check(VERSION));
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
        self.marked.clear();
        self.expansion.close_all(0);
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
                expanded: self
                    .expansion
                    .open_nodes()
                    .filter(|&n| !self.expansion.is_temporary(n))
                    .map(|n| tree.path(n))
                    .collect(),
                temporary: self
                    .expansion
                    .temporary_nodes()
                    .map(|n| tree.path(n))
                    .collect(),
                treemap_root: tree.path(self.treemap_root),
            });
        }
        self.start_scan(p);
    }

    fn start_scan(&mut self, path: PathBuf) {
        if let Some(s) = self.scan.take() {
            s.cancel();
        }
        self.mounts = oxtree_native::mounts::MountTable::load();
        self.progress = (0, 0, 0);
        self.last_elapsed = None;
        self.scan_started = Instant::now();
        self.set_status(format!("Reading {}...", path.display()), true);
        self.scan = Some(oxtree_native::scan(
            &path,
            ScanOptions {
                threads: self.threads,
                cross_filesystems: self.cross_filesystems,
                mounts: self.mounts.clone(),
                ..Default::default()
            },
            self.wake.clone(),
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        self.free_job = Some(rx);
        self.free_space = None;
        self.root_is_volume = self.mounts.at(&path).is_some();
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(oxtree_native::mounts::free_space(&p));
        });
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
        self.expansion.close_all(root);
        match self.view_memory.take() {
            Some(m) => {
                let permanent: Vec<NodeId> =
                    m.expanded.iter().filter_map(|p| tree.find(p)).collect();
                let temporary: Vec<NodeId> =
                    m.temporary.iter().filter_map(|p| tree.find(p)).collect();
                self.expansion.restore(root, permanent, temporary);
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
        self.entered_mounts.clear();
        for m in &self.mounts.mounts {
            let p = m.path.to_string_lossy();
            if let Some(n) = tree.find(&p) {
                if n != root && tree.kind(n) == Kind::Dir {
                    self.entered_mounts.insert(n);
                }
            }
        }
        self.subtree_errors.clear();
        for n in 0..tree.len() as NodeId {
            if tree.kind(n) == Kind::UnreadDir {
                let mut p = tree.parent(n);
                while let Some(a) = p {
                    if !self.subtree_errors.insert(a) {
                        break;
                    }
                    p = tree.parent(a);
                }
            }
        }
        self.tree = Some(Arc::new(tree));
        self.marked = self.selected.into_iter().collect();
        self.types = None;
        self.dupes = None;
        self.dupes_job = None;
        self.cleanup_queue.clear();
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
                    if !self.is_root {
                        msg.push_str("  File, Rescan as Root reads them.");
                    }
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

        self.pump_file_view(ctx);
        if let Some(rx) = &self.dupes_job {
            if let Ok((mode, counts)) = rx.try_recv() {
                self.dupes_job = None;
                self.dupes = Some((mode, Arc::new(counts)));
                self.file_view.dirty = true;
            }
        }
        if let Some(rx) = &self.update_job {
            match rx.try_recv() {
                Ok(result) => {
                    self.update_job = None;
                    if let UpdateResult::Newer(tag) = &result {
                        self.set_status(
                            format!("OxTree {tag} is available; see Edit, Preferences"),
                            false,
                        );
                    }
                    self.update_result = Some(result);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.update_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(250));
                }
            }
        }
        if let Some(rx) = &self.free_job {
            match rx.try_recv() {
                Ok(free) => {
                    self.free_job = None;
                    self.free_space = free;
                    self.rows_dirty = true;
                    self.treemap.generation += 1;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.free_job = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }

        if let Some(launch) = &self.root_launch {
            if launch.is_ready() {
                // The root window is up: this one hands over and closes.
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                match launch.exit.try_recv() {
                    Ok(Ok(_)) => {
                        self.root_launch = None;
                        self.set_status("The root instance closed.", false);
                    }
                    Ok(Err(e)) => {
                        self.root_launch = None;
                        self.show_error(e);
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => self.root_launch = None,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        ctx.request_repaint_after(Duration::from_millis(100));
                    }
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
                    self.treemap.free_px = result.free_px;
                    self.treemap.rendered_generation = result.generation;
                    self.treemap.rebuilds += 1;
                    #[cfg(feature = "diagnostics")]
                    if let Some(st) = &mut self.stats {
                        st.rebuilds_in_window += 1;
                    }
                }
            }
        }
        #[cfg(feature = "diagnostics")]
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
            if let Some((index, node)) = self.cleanup_queue.pop() {
                // The next item of a multi-selection; the rescan waits for
                // the last one.
                self.run_cleanup(index, node);
            } else if r != Refresh::No {
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
        let Some(tree) = &self.tree else {
            return String::new();
        };
        if self.marked.len() > 1 {
            let total: u64 = self
                .marked
                .iter()
                .map(|&n| tree.size(n))
                .fold(0, u64::saturating_add);
            return format!(
                "{} items selected  {}",
                format::grouped(self.marked.len() as u64),
                format::size(total)
            );
        }
        match self.selected {
            Some(n) => format!("{}  {}", tree.path(n), format::size(tree.size(n))),
            None => String::new(),
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
            if node == NO_NODE || !self.expansion.is_open(node) {
                continue;
            }
            if node == tree.root() && self.root_is_volume && self.free_space.is_some() {
                // Free space of the filesystem as the last row under the
                // root, as WizTree lists it; `NO_NODE` marks it.
                stack.push((NO_NODE, 1));
            }
            let mut kids: Vec<NodeId> = tree.children(node).collect();
            sort_children(&tree, &mut kids, sort);
            for k in kids.into_iter().rev() {
                stack.push((k, depth + 1));
            }
        }
    }

    /// Node ids of the rows in order, for range selection.
    pub fn row_order(&self) -> Vec<NodeId> {
        self.rows
            .iter()
            .map(|r| r.node)
            .filter(|&n| n != NO_NODE)
            .collect()
    }

    /// Select; with `reveal` the path to the node is opened temporarily
    /// and the tree scrolls to it (a treemap click, a File View jump).
    pub fn select(&mut self, node: NodeId, reveal: bool) {
        self.selected = Some(node);
        self.marked.clear();
        self.marked.insert(node);
        if reveal {
            if let Some(tree) = &self.tree {
                self.expansion.reveal(tree, node);
                self.rows_dirty = true;
                self.scroll_to_selected = true;
            }
        }
        self.status.text = self.default_status();
        self.status.until = None;
    }

    /// A click with modifiers: Ctrl toggles the item in the selection,
    /// Shift selects from the current item to it in `order` (the rows as
    /// shown), neither selects it alone.
    pub fn select_in(&mut self, node: NodeId, mods: egui::Modifiers, order: &[NodeId]) {
        if mods.shift {
            let anchor = self.selected.unwrap_or(node);
            let a = order.iter().position(|&n| n == anchor);
            let b = order.iter().position(|&n| n == node);
            if let (Some(a), Some(b)) = (a, b) {
                let (lo, hi) = (a.min(b), a.max(b));
                if !mods.ctrl {
                    self.marked.clear();
                }
                self.marked
                    .extend(order.get(lo..=hi).unwrap_or_default().iter().copied());
                self.marked.insert(anchor);
                self.status.text = self.default_status();
                self.status.until = None;
                return;
            }
        }
        if mods.ctrl {
            if !self.marked.remove(&node) {
                self.marked.insert(node);
            }
            self.selected = Some(node);
            self.status.text = self.default_status();
            self.status.until = None;
            return;
        }
        self.select(node, false);
    }

    /// A right click keeps a selection the item is part of.
    pub fn select_for_menu(&mut self, node: NodeId) {
        if self.marked.contains(&node) {
            self.selected = Some(node);
        } else {
            self.select(node, false);
        }
    }

    /// Ctrl+A: every row of the view.
    pub fn select_all(&mut self, order: &[NodeId]) {
        if order.is_empty() {
            return;
        }
        self.marked = order.iter().copied().collect();
        if self.selected.is_none_or(|s| !self.marked.contains(&s)) {
            self.selected = order.first().copied();
        }
        self.status.text = self.default_status();
        self.status.until = None;
    }

    /// The Reset button: the views as at the start, with the tree, the
    /// current directory and the treemap position kept.
    pub fn reset_views(&mut self) {
        self.close_all_levels();
        self.sort = Sort {
            column: Column::Size,
            ascending: false,
        };
        self.file_view = FileViewState::default();
        self.types_sort = (TypeSort::Size, false);
        self.tree_columns.reset();
        self.file_columns.reset();
        self.types_columns.reset();
        self.treemap_zoom_reset();
    }

    /// Open every directory with children.
    pub fn expand_all(&mut self) {
        self.expand_to_level(u32::MAX);
    }

    /// Open directories above `level` levels below the root, close the rest.
    pub fn expand_to_level(&mut self, level: u32) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        self.expansion.close_all(tree.root());
        let mut stack: Vec<(NodeId, u32)> = vec![(tree.root(), 0)];
        while let Some((node, depth)) = stack.pop() {
            if depth > 0 {
                if depth > level || tree.child_count(node) == 0 {
                    continue;
                }
                self.expansion.open_by_hand(&tree, node);
            }
            for c in tree.children(node) {
                stack.push((c, depth + 1));
            }
        }
        self.rows_dirty = true;
    }

    pub fn toggle_expand(&mut self, node: NodeId) {
        if let Some(tree) = &self.tree {
            self.expansion.toggle_by_hand(tree, node);
            self.rows_dirty = true;
        }
    }

    pub fn set_expanded(&mut self, node: NodeId, expanded: bool) {
        let Some(tree) = &self.tree else { return };
        let changed = if expanded {
            self.expansion.open_by_hand(tree, node)
        } else {
            self.expansion.close(node)
        };
        if changed {
            self.rows_dirty = true;
        }
    }

    pub fn close_all_levels(&mut self) {
        if let Some(tree) = &self.tree {
            self.expansion.close_all(tree.root());
            self.select(tree.root(), false);
        }
        self.rows_dirty = true;
    }

    // ------------------------------------------------------------ file types

    /// Build the File Types table if the tree changed since the last one.
    pub fn ensure_types(&mut self) {
        if self.types.is_some() {
            return;
        }
        let Some(tree) = &self.tree else { return };
        self.types = Some(Arc::new(oxtree_core::types::file_types(
            tree,
            &self.categorizer,
        )));
    }

    pub fn set_types_sort(&mut self, sort: TypeSort) {
        if self.types_sort.0 == sort {
            self.types_sort.1 = !self.types_sort.1;
        } else {
            self.types_sort = (sort, matches!(sort, TypeSort::Ext | TypeSort::Category));
        }
    }

    /// Double click on a type: the File View listing those files.
    pub fn show_type_in_file_view(&mut self, query: String) {
        self.file_view.search = query;
        self.file_view.match_path = false;
        self.file_view.dirty = true;
        self.view = View::Files;
    }

    /// Duplicate counts for the File View's mode, started on a thread when
    /// missing; `None` until they exist.
    fn dupes_for(&mut self, ctx: &Context, mode: DupMode) -> Option<Arc<Vec<u32>>> {
        if mode == DupMode::Off {
            return Some(Arc::new(Vec::new()));
        }
        if let Some((m, counts)) = &self.dupes {
            if *m == mode {
                return Some(counts.clone());
            }
        }
        if self.dupes_job.is_none() {
            let tree = self.tree.clone()?;
            let (tx, rx) = std::sync::mpsc::channel();
            self.dupes_job = Some(rx);
            let repaint = ctx.clone();
            std::thread::Builder::new()
                .name("oxtree-dupes".into())
                .spawn(move || {
                    let _ = tx.send((mode, oxtree_core::dupes::duplicate_counts(&tree, mode)));
                    repaint.request_repaint();
                })
                .ok();
        }
        None
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

    /// Start building the list on a worker thread if it is out of date and
    /// no build is running. The old rows stay on screen until the new ones
    /// land in `pump`.
    pub fn request_file_view(&mut self, ctx: &Context) {
        if !self.file_view.dirty || self.file_view.job.is_some() {
            return;
        }
        let Some(tree) = self.tree.clone() else {
            self.file_view.dirty = false;
            self.file_view.rows.clear();
            return;
        };
        let Some(dupes) = self.dupes_for(ctx, self.file_view.dup_mode) else {
            return;
        };
        self.file_view.dirty = false;
        let fv = &mut self.file_view;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        match Query::parse(&fv.search, now, self.utc_offset) {
            Ok(q) => {
                fv.query = if q.is_empty() { None } else { Some(q) };
                fv.search_error = None;
            }
            Err(e) => {
                fv.query = None;
                fv.search_error = Some(e);
            }
        }
        let match_path = fv.match_path;
        let needs_path = fv.query.as_ref().is_some_and(|q| q.needs_path(match_path));
        let query = fv.query.clone();
        let (include_dirs, sort, ascending, max) = (fv.include_dirs, fv.sort, fv.ascending, fv.max);
        fv.generation += 1;
        let generation = fv.generation;
        let (tx, rx) = std::sync::mpsc::channel();
        fv.job = Some(rx);
        let repaint = ctx.clone();
        std::thread::Builder::new()
            .name("oxtree-search".into())
            .spawn(move || {
                let dup_only = !dupes.is_empty();
                let rows = flat_list_filtered(
                    &tree,
                    include_dirs,
                    sort,
                    ascending,
                    if dup_only { 0 } else { max },
                    |n| {
                        if dup_only && dupes.get(n as usize).copied().unwrap_or(0) < 2 {
                            return false;
                        }
                        let Some(q) = &query else { return true };
                        // ponytail: the path is built per node when the query
                        // needs it; a shared path buffer is the upgrade if a
                        // million-file path search ever feels slow.
                        let path = if needs_path {
                            tree.path(n)
                        } else {
                            String::new()
                        };
                        q.matches(
                            &Item {
                                name: tree.name(n),
                                path: &path,
                                size: tree.size(n),
                                mtime: tree.mtime(n),
                                is_dir: tree.is_dir_like(n),
                            },
                            match_path,
                        )
                    },
                );
                let rows = if dup_only {
                    // Groups together, the group with the most wasted space
                    // first, as WizTree orders them.
                    let mut rows = rows;
                    rows.sort_by_cached_key(|&n| {
                        let count = dupes.get(n as usize).copied().unwrap_or(0) as u64;
                        (
                            std::cmp::Reverse(count.saturating_mul(tree.size(n))),
                            tree.name(n).to_lowercase(),
                            std::cmp::Reverse(tree.size(n)),
                        )
                    });
                    if max > 0 {
                        rows.truncate(max);
                    }
                    rows
                } else {
                    rows
                };
                let _ = tx.send((generation, rows));
                repaint.request_repaint();
            })
            .ok();
    }

    /// Called from `pump`: take a finished list, then start the next build
    /// if the inputs changed while it ran.
    fn pump_file_view(&mut self, ctx: &Context) {
        let mut finished = false;
        if let Some(rx) = &self.file_view.job {
            if let Ok((generation, rows)) = rx.try_recv() {
                if generation == self.file_view.generation {
                    self.file_view.rows = rows;
                }
                finished = true;
            }
        }
        if finished {
            self.file_view.job = None;
            if self.file_view.dirty {
                self.request_file_view(ctx);
            }
        }
    }

    /// `--screenshot`: write the first full frame to this file, then quit.
    #[cfg(feature = "diagnostics")]
    pub fn set_screenshot(&mut self, path: PathBuf) {
        self.screenshot = Some((path, 4));
    }

    /// Give the background threads a way to request a frame.
    pub fn attach_context(&mut self, ctx: &Context) {
        self.apply_theme(ctx);
        let ctx = ctx.clone();
        self.wake.set(move || ctx.request_repaint());
    }

    /// Ctrl+F: the File View with the search box focused.
    pub fn focus_search(&mut self) {
        self.view = View::Files;
        self.file_view.focus_search = true;
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
        // Free space of the volume as a strip on the right of the whole
        // map, sized like a tile: its share of used plus free.
        let free_px = match self.free_space {
            Some((avail, _)) if self.root_is_volume && root == tree.root() && avail > 0 => {
                let used = tree.size(root) as f64;
                let share = avail as f64 / (used + avail as f64);
                let px = (width as f64 * share).round() as usize;
                if px >= theme::MIN_TILE_SIZE as usize && px < width {
                    px
                } else {
                    0
                }
            }
            _ => 0,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.treemap.job = Some(rx);
        std::thread::Builder::new()
            .name("oxtree-treemap".into())
            .spawn(move || {
                let started = Instant::now();
                let rect = oxtree_core::layout::Rect::new(
                    0.0,
                    0.0,
                    (width - free_px) as f64,
                    height as f64,
                );
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
                    free_px,
                });
                ctx.request_repaint();
            })
            .ok();
    }

    // ------------------------------------------------------------ actions

    /// The selected items in tree order, the current item first when it
    /// is one of them.
    pub fn selection(&self) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = self.marked.iter().copied().collect();
        v.sort_unstable();
        if let Some(s) = self.selected {
            if let Some(i) = v.iter().position(|&n| n == s) {
                v.remove(i);
                v.insert(0, s);
            } else if v.is_empty() {
                v.push(s);
            }
        }
        v
    }

    pub fn copy_path(&mut self, ctx: &Context) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let paths: Vec<String> = self.selection().iter().map(|&n| tree.path(n)).collect();
        if paths.is_empty() {
            return;
        }
        let text = paths.join("\n");
        ctx.copy_text(text);
        let msg = match paths.as_slice() {
            [one] => format!("Copied to clipboard: {one}"),
            many => format!("Copied {} paths to clipboard", many.len()),
        };
        self.set_status(msg, false);
    }

    /// Edit, Copy Tree to Clipboard: the rows as shown, indented.
    pub fn copy_tree_text(&mut self, ctx: &Context) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let text = oxtree_core::export::tree_text(
            &tree,
            self.rows
                .iter()
                .filter(|r| r.node != NO_NODE)
                .map(|r| (r.node, r.depth)),
        );
        ctx.copy_text(text);
        self.set_status(
            format!("Copied {} rows to clipboard", self.rows.len()),
            false,
        );
    }

    pub fn export_csv(&mut self, dialog: &ExportDialog) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let opts = ExportOptions {
            separator: if dialog.tsv { '\t' } else { ',' },
            max_depth: dialog.max_depth.trim().parse().ok(),
            utc_offset: if dialog.utc { 0 } else { self.utc_offset },
            percent_of_parent: dialog.percent_of_parent,
        };
        let text = oxtree_core::export::csv(&tree, dialog.root, &opts, |_| true);
        let path = dialog.path.trim();
        match std::fs::write(path, text) {
            Ok(()) => self.set_status(format!("Exported {path}"), false),
            Err(e) => self.show_error(format!("Cannot write {path}: {e}")),
        }
    }

    /// Items the trash and the cleanups may touch: the selection without
    /// the root and the `<Files>` entries.
    fn actionable(&self) -> Vec<NodeId> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        self.selection()
            .into_iter()
            .filter(|&n| n != tree.root() && tree.kind(n) != Kind::DotEntry)
            .collect()
    }

    pub fn can_trash(&self) -> bool {
        self.scan.is_none() && self.trash_job.is_none() && !self.actionable().is_empty()
    }

    pub fn move_to_trash(&mut self) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        let items = self.actionable();
        if items.is_empty() || self.trash_job.is_some() {
            return;
        }
        let paths: Vec<String> = items.iter().map(|&n| tree.path(n)).collect();
        let (tx, rx) = std::sync::mpsc::channel();
        self.trash_job = Some(rx);
        std::thread::spawn(move || {
            for (done, p) in paths.iter().enumerate() {
                if let Err(e) = oxtree_native::trash::move_to_trash(p) {
                    let _ = tx.send(Err(if done > 0 {
                        format!("{e} ({done} moved before it)")
                    } else {
                        e
                    }));
                    return;
                }
            }
            let _ = tx.send(Ok(match paths.as_slice() {
                [one] => one.clone(),
                many => format!("{} items", many.len()),
            }));
        });
    }

    pub fn cleanup_applies(&self, index: usize) -> bool {
        let (Some(tree), Some(c)) = (&self.tree, self.cleanups.get(index)) else {
            return false;
        };
        c.active && self.selection().iter().any(|&n| c.works_for(tree.kind(n)))
    }

    /// The selected items this cleanup works for.
    pub fn cleanup_targets(&self, index: usize) -> Vec<NodeId> {
        let (Some(tree), Some(c)) = (&self.tree, self.cleanups.get(index)) else {
            return Vec::new();
        };
        self.selection()
            .into_iter()
            .filter(|&n| c.works_for(tree.kind(n)))
            .collect()
    }

    pub fn request_cleanup(&mut self, index: usize) {
        if !self.cleanup_applies(index) {
            return;
        }
        let Some(&n) = self.cleanup_targets(index).first() else {
            return;
        };
        let ask = self
            .cleanups
            .get(index)
            .map(|c| c.ask_for_confirmation)
            .unwrap_or(true);
        if ask {
            self.confirm = Some(PendingCleanup { index, node: n });
        } else {
            self.run_cleanup_on_selection(index);
        }
    }

    /// Run the cleanup for every selected item it works for, one after
    /// the other; the output window shows the one running.
    pub fn run_cleanup_on_selection(&mut self, index: usize) {
        let mut targets = self.cleanup_targets(index);
        targets.reverse();
        let Some(first) = targets.pop() else { return };
        self.cleanup_queue = targets.into_iter().map(|n| (index, n)).collect();
        self.run_cleanup(index, first);
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

    /// File, Rescan as Root: OxTree as root on the current path replaces
    /// this window once it is up. A dismissed or refused authorization
    /// leaves this window as it is.
    pub fn rescan_as_root(&mut self) {
        let Some(path) = self.scan_path.clone() else {
            return;
        };
        match oxtree_native::root::relaunch_as_root(&path, &oxtree_native::config::config_dir()) {
            Ok(launch) => {
                self.root_launch = Some(launch);
                self.set_status("Waiting for authorization...", true);
            }
            Err(e) => self.show_error(e),
        }
    }

    pub fn load_volumes(&mut self) {
        if self.volumes.is_none() {
            self.volumes = Some(oxtree_native::mounts::volumes());
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

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
