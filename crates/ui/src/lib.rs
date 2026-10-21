//! egui views, theme and application state. Nothing here opens a window;
//! the `oxtree` binary does that and hands the state to eframe.

pub mod app;
pub mod details_view;
pub mod dialogs;
pub mod menus;
pub mod status_bar;
pub mod theme;
pub mod tree_view;
pub mod treemap_view;

pub use app::OxTreeApp;
