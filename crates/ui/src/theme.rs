//! QDirStat's default colours and sizes as named tokens. Views use these
//! names and never a literal colour.

use eframe::egui::Color32;
use std::time::Duration;

/// `[Treemaps] SelectedItemsColor`
pub const SELECTED_ITEMS: Color32 = Color32::from_rgb(0xff, 0xff, 0x00);
/// `[Treemaps] CurrentItemColor`
pub const CURRENT_ITEM: Color32 = Color32::from_rgb(0xff, 0x00, 0x00);
/// `[Treemaps] FileFillColor`: files without a category.
pub const FILE_FILL: [u8; 3] = [0xde, 0x8d, 0x53];
/// `[Treemaps] MinTileSize`
pub const MIN_TILE_SIZE: f64 = 3.0;
/// Upper bound on treemap rebuilds per second during a resize, so a high
/// refresh rate display does not turn a drag into a benchmark. 120 keeps
/// every frame on a 60 Hz screen; a 60 cap would skip frames there.
pub const TREEMAP_MAX_REBUILD_HZ: u32 = 120;
/// Below this many points on a side the treemap is not rebuilt, QDirStat's
/// way of letting the user shrink it to skip the cost.
pub const TREEMAP_UPDATE_MIN_SIZE: f32 = 20.0;

/// `[PercentBar] Background`
pub const PERCENT_BAR_BACKGROUND: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa0);
/// `[PercentBar] Colors`, by tree depth.
pub const PERCENT_BAR_COLORS: [Color32; 12] = [
    Color32::from_rgb(0x00, 0x00, 0xff),
    Color32::from_rgb(0x80, 0x00, 0x80),
    Color32::from_rgb(0xe7, 0x93, 0x2b),
    Color32::from_rgb(0x04, 0x71, 0x00),
    Color32::from_rgb(0xb0, 0x00, 0x00),
    Color32::from_rgb(0xcc, 0xbb, 0x00),
    Color32::from_rgb(0xa2, 0x62, 0x1e),
    Color32::from_rgb(0x00, 0x94, 0x92),
    Color32::from_rgb(0xd9, 0x5e, 0x00),
    Color32::from_rgb(0x00, 0xc2, 0x41),
    Color32::from_rgb(0xc2, 0x6c, 0xbb),
    Color32::from_rgb(0x00, 0xb3, 0xff),
];
/// `[PercentBar] PercentBarColumnWidth`
pub const PERCENT_BAR_COLUMN_WIDTH: f32 = 180.0;

/// `DirReadErrColor`: name colour of a directory that could not be read.
pub const DIR_READ_ERROR: Color32 = Color32::from_rgb(0xff, 0x00, 0x00);
/// `SubtreeReadErrColor`
pub const SUBTREE_READ_ERROR: Color32 = Color32::from_rgb(0xff, 0xff, 0x00);

/// Tree icons, painted, in the colours of QDirStat's tree-medium set.
pub const ICON_DIR: Color32 = Color32::from_rgb(0xe9, 0x9a, 0x2e);
pub const ICON_DIR_TAB: Color32 = Color32::from_rgb(0xc9, 0x7c, 0x1a);
pub const ICON_FILE: Color32 = Color32::from_rgb(0xd8, 0xd8, 0xd8);
pub const ICON_FILE_EDGE: Color32 = Color32::from_rgb(0x90, 0x90, 0x90);
pub const ICON_SYMLINK: Color32 = Color32::from_rgb(0x40, 0xa0, 0xff);
pub const ICON_MOUNT: Color32 = Color32::from_rgb(0xa0, 0xa8, 0xb8);
pub const ICON_SIZE: f32 = 14.0;
/// The free space row bar and the treemap strip.
pub const FREE_SPACE: Color32 = Color32::from_rgb(0x70, 0x78, 0x80);

/// Behind a search match in the File View.
pub const SEARCH_HIGHLIGHT: Color32 = Color32::from_rgba_premultiplied(0x80, 0x80, 0x00, 0x80);

/// `StatusBarTimeoutMillisec`
pub const STATUS_TIMEOUT: Duration = Duration::from_millis(3000);

pub const ROW_HEIGHT: f32 = 20.0;
pub const INDENT: f32 = 18.0;
/// The name column never gets narrower than this; the tree scrolls
/// sideways instead.
pub const COL_NAME_MIN: f32 = 400.0;
/// The File View path column, likewise.
pub const COL_PATH_MIN: f32 = 520.0;
pub const COL_PERCENT_NUM: f32 = 56.0;
pub const COL_SIZE: f32 = 84.0;
pub const COL_COUNT: f32 = 72.0;
pub const COL_MTIME: f32 = 124.0;
pub const COL_GAP: f32 = 8.0;

pub fn percent_bar_color(depth: u32) -> Color32 {
    PERCENT_BAR_COLORS
        .get(depth as usize % PERCENT_BAR_COLORS.len())
        .copied()
        .unwrap_or(PERCENT_BAR_BACKGROUND)
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}
