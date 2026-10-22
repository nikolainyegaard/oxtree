//! egui views, theme and application state. Nothing here opens a window;
//! the `oxtree` binary does that and hands the state to eframe.

pub mod app;
pub mod columns;
pub mod details_view;
pub mod dialogs;
pub mod expansion;
pub mod file_view;
pub mod menus;
pub mod status_bar;
pub mod theme;
pub mod tree_view;
pub mod treemap_view;
pub mod types_view;

pub use app::OxTreeApp;
