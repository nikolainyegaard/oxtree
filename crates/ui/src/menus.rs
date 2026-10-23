//! Menu bar and keyboard shortcuts, QDirStat's menu structure trimmed to
//! what exists.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Ui};

use crate::app::{OxTreeApp, View};

const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::O);
const REFRESH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F5);
const QUIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Q);
const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::C);
const TRASH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Delete);
const TREEMAP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F9);
const ZOOM_IN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Plus);
const ZOOM_OUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Minus);
const ZOOM_RESET: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Num0);
const GO_UP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::ALT, Key::ArrowUp);
const CLOSE_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::W);
const VIEW_TREE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Num1);
const VIEW_FILES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Num2);
const VIEW_TYPES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Num3);
const VIEW_TYPES_F8: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::F8);
const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::F);
const PREFERENCES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Comma);
const EXPAND_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::F4);
const CTRL_ALT: Modifiers = Modifiers {
    alt: true,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: false,
};
const EXPORT: KeyboardShortcut = KeyboardShortcut::new(CTRL_ALT, Key::E);
const COPY_TREE: KeyboardShortcut = KeyboardShortcut::new(CTRL_ALT, Key::C);

pub fn handle_shortcuts(app: &mut OxTreeApp, ctx: &egui::Context) {
    if ctx.memory(|m| m.focused().is_some()) {
        return;
    }
    let pressed = |s: &KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(s));
    if pressed(&OPEN) {
        open_dialog(app);
    }
    if pressed(&REFRESH) {
        app.refresh_all();
    }
    if pressed(&QUIT) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    // The three-key shortcuts before the two-key ones they extend.
    if pressed(&EXPORT) {
        open_export(app, app.tree.as_ref().map(|t| t.root()));
    }
    if pressed(&COPY_TREE) {
        app.copy_tree_text(ctx);
    }
    if pressed(&COPY) {
        app.copy_path(ctx);
    }
    if pressed(&TRASH) {
        app.move_to_trash();
    }
    if pressed(&TREEMAP) {
        app.show_treemap = !app.show_treemap;
    }
    if pressed(&ZOOM_IN) {
        app.treemap_zoom_in();
    }
    if pressed(&ZOOM_OUT) {
        app.treemap_zoom_out();
    }
    if pressed(&ZOOM_RESET) {
        app.treemap_zoom_reset();
    }
    if pressed(&GO_UP) {
        app.go_up();
    }
    if pressed(&CLOSE_ALL) {
        app.close_all_levels();
    }
    if pressed(&VIEW_TREE) {
        app.view = View::Tree;
    }
    if pressed(&VIEW_FILES) {
        app.view = View::Files;
    }
    if pressed(&VIEW_TYPES) || pressed(&VIEW_TYPES_F8) {
        app.view = View::Types;
    }
    if pressed(&EXPAND_ALL) {
        app.expand_all();
    }
    if pressed(&FIND) {
        app.focus_search();
    }
    if pressed(&PREFERENCES) {
        app.preferences_open = true;
    }
    let hotkeys: Vec<(usize, String)> = app
        .cleanups
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.hotkey.is_empty())
        .map(|(i, c)| (i, c.hotkey.clone()))
        .collect();
    for (i, hk) in hotkeys {
        if let Some(s) = parse_hotkey(&hk) {
            if pressed(&s) {
                app.request_cleanup(i);
            }
        }
    }
}

fn open_dialog(app: &mut OxTreeApp) {
    if app.open_dir_text.is_none() {
        let start = app
            .scan_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(oxtree_native::config::home_dir);
        app.open_dir_text = Some(start);
    }
}

fn open_export(app: &mut OxTreeApp, root: Option<u32>) {
    let Some(root) = root else { return };
    let home = oxtree_native::config::home_dir();
    let name = app
        .tree
        .as_ref()
        .map(|t| {
            if root == t.root() {
                std::path::Path::new(t.root_path())
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            } else {
                t.name(root).to_string()
            }
        })
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "root".into());
    app.export_dialog = Some(crate::app::ExportDialog {
        root,
        path: format!("{home}/{name}.csv"),
        tsv: false,
        max_depth: String::new(),
        utc: false,
        percent_of_parent: false,
    });
}

/// Right-click menu for the tree and the treemap: the Edit items, the
/// cleanups and the treemap actions, all acting on the current selection.
pub fn context_menu(app: &mut OxTreeApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    ui.set_min_width(220.0);
    let has_sel = app.selected.is_some();
    let many = app.marked.len() > 1;
    if ui
        .add_enabled(
            has_sel,
            egui::Button::new(if many {
                "Copy Paths to Clipboard"
            } else {
                "Copy Path to Clipboard"
            })
            .shortcut_text(ctx.format_shortcut(&COPY)),
        )
        .clicked()
    {
        app.copy_path(&ctx);
        ui.close();
    }
    if ui
        .add_enabled(
            app.can_trash(),
            egui::Button::new("Move to Trash").shortcut_text(ctx.format_shortcut(&TRASH)),
        )
        .clicked()
    {
        app.move_to_trash();
        ui.close();
    }
    let dir_selected = app
        .tree
        .as_ref()
        .zip(app.selected)
        .is_some_and(|(t, n)| t.is_dir_like(n) && t.kind(n) != oxtree_core::Kind::DotEntry);
    if ui
        .add_enabled(dir_selected, egui::Button::new("Export Subtree to CSV..."))
        .clicked()
    {
        open_export(app, app.selected);
        ui.close();
    }
    ui.separator();
    let count = app.cleanups.len();
    for i in 0..count {
        let (title, hotkey) = match app.cleanups.get(i) {
            Some(c) => (c.display_title(), c.hotkey.clone()),
            None => continue,
        };
        if ui
            .add_enabled(
                app.cleanup_applies(i),
                egui::Button::new(title).shortcut_text(hotkey),
            )
            .clicked()
        {
            app.request_cleanup(i);
            ui.close();
        }
    }
    ui.separator();
    if ui
        .add(egui::Button::new("Treemap Zoom In").shortcut_text(ctx.format_shortcut(&ZOOM_IN)))
        .clicked()
    {
        app.treemap_zoom_in();
        ui.close();
    }
    if ui
        .add(egui::Button::new("Treemap Zoom Out").shortcut_text(ctx.format_shortcut(&ZOOM_OUT)))
        .clicked()
    {
        app.treemap_zoom_out();
        ui.close();
    }
    if ui
        .add(
            egui::Button::new("Reset Treemap Zoom").shortcut_text(ctx.format_shortcut(&ZOOM_RESET)),
        )
        .clicked()
    {
        app.treemap_zoom_reset();
        ui.close();
    }
    ui.separator();
    if ui
        .add(egui::Button::new("Up One Level").shortcut_text(ctx.format_shortcut(&GO_UP)))
        .clicked()
    {
        app.go_up();
        ui.close();
    }
}

/// `Ctrl+Shift+Delete` style text into a shortcut.
pub fn parse_hotkey(text: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+') {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CTRL,
            "shift" => mods |= Modifiers::SHIFT,
            "alt" => mods |= Modifiers::ALT,
            "meta" | "super" => mods |= Modifiers::COMMAND,
            k => key = Key::from_name(&capitalize(k)),
        }
    }
    key.map(|k| KeyboardShortcut::new(mods, k))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui
                .add(
                    egui::Button::new("Open Directory...")
                        .shortcut_text(ctx.format_shortcut(&OPEN)),
                )
                .clicked()
            {
                open_dialog(app);
                ui.close();
            }
            if ui
                .add_enabled(
                    app.scan_path.is_some(),
                    egui::Button::new("Refresh All").shortcut_text(ctx.format_shortcut(&REFRESH)),
                )
                .clicked()
            {
                app.refresh_all();
                ui.close();
            }
            if ui
                .add_enabled(
                    app.scan_path.is_some() && !app.is_root && app.root_launch.is_none(),
                    egui::Button::new("Rescan as Root..."),
                )
                .on_hover_text(
                    "Replaces this window with OxTree running as root on the same directory, through polkit",
                )
                .clicked()
            {
                app.rescan_as_root();
                ui.close();
            }
            if ui
                .add_enabled(app.scan.is_some(), egui::Button::new("Stop Reading"))
                .clicked()
            {
                app.stop_reading();
                ui.close();
            }
            ui.separator();
            if ui.button("Read Cache File...").clicked() {
                app.cache_dialog = Some((false, default_cache_path(app)));
                ui.close();
            }
            if ui
                .add_enabled(app.tree.is_some(), egui::Button::new("Write Cache File..."))
                .clicked()
            {
                app.cache_dialog = Some((true, default_cache_path(app)));
                ui.close();
            }
            ui.separator();
            if ui
                .add_enabled(
                    app.tree.is_some(),
                    egui::Button::new("Export to CSV...").shortcut_text(ctx.format_shortcut(&EXPORT)),
                )
                .clicked()
            {
                open_export(app, app.tree.as_ref().map(|t| t.root()));
                ui.close();
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Quit").shortcut_text(ctx.format_shortcut(&QUIT)))
                .clicked()
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Edit", |ui| {
            let has_sel = app.selected.is_some();
            if ui
                .add_enabled(
                    has_sel,
                    egui::Button::new("Copy Path to Clipboard")
                        .shortcut_text(ctx.format_shortcut(&COPY)),
                )
                .clicked()
            {
                app.copy_path(&ctx);
                ui.close();
            }
            if ui
                .add_enabled(
                    app.tree.is_some(),
                    egui::Button::new("Copy Tree to Clipboard")
                        .shortcut_text(ctx.format_shortcut(&COPY_TREE)),
                )
                .on_hover_text("The rows as shown, names and sizes, indented")
                .clicked()
            {
                app.copy_tree_text(&ctx);
                ui.close();
            }
            if ui
                .add_enabled(
                    app.can_trash(),
                    egui::Button::new("Move to Trash").shortcut_text(ctx.format_shortcut(&TRASH)),
                )
                .clicked()
            {
                app.move_to_trash();
                ui.close();
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new("Preferences...")
                        .shortcut_text(ctx.format_shortcut(&PREFERENCES)),
                )
                .clicked()
            {
                app.preferences_open = true;
                ui.close();
            }
        });
        ui.menu_button("View", |ui| {
            if ui
                .add(egui::Button::new("Tree View").shortcut_text(ctx.format_shortcut(&VIEW_TREE)))
                .clicked()
            {
                app.view = View::Tree;
                ui.close();
            }
            if ui
                .add(egui::Button::new("File View").shortcut_text(ctx.format_shortcut(&VIEW_FILES)))
                .clicked()
            {
                app.view = View::Files;
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new("File Types").shortcut_text(ctx.format_shortcut(&VIEW_TYPES)),
                )
                .clicked()
            {
                app.view = View::Types;
                ui.close();
            }
            if ui
                .add(egui::Button::new("Find Files...").shortcut_text(ctx.format_shortcut(&FIND)))
                .clicked()
            {
                app.focus_search();
                ui.close();
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new(if app.show_treemap {
                        "Hide Treemap"
                    } else {
                        "Show Treemap"
                    })
                    .shortcut_text(ctx.format_shortcut(&TREEMAP)),
                )
                .clicked()
            {
                app.show_treemap = !app.show_treemap;
                ui.close();
            }
            if ui
                .button(if app.show_details {
                    "Hide Details Panel"
                } else {
                    "Show Details Panel"
                })
                .clicked()
            {
                app.show_details = !app.show_details;
                ui.close();
            }
            if ui
                .checkbox(&mut app.show_tile_names, "Names on Treemap Tiles")
                .changed()
            {
                ui.close();
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new("Close All Tree Levels")
                        .shortcut_text(ctx.format_shortcut(&CLOSE_ALL)),
                )
                .clicked()
            {
                app.close_all_levels();
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new("Expand All Tree Levels")
                        .shortcut_text(ctx.format_shortcut(&EXPAND_ALL)),
                )
                .clicked()
            {
                app.expand_all();
                ui.close();
            }
            ui.menu_button("Expand Tree to Level", |ui| {
                for level in 1..=9u32 {
                    if ui.button(format!("Level {level}")).clicked() {
                        app.expand_to_level(level);
                        ui.close();
                    }
                }
            });
            ui.separator();
            if ui
                .add(
                    egui::Button::new("Treemap Zoom In")
                        .shortcut_text(ctx.format_shortcut(&ZOOM_IN)),
                )
                .clicked()
            {
                app.treemap_zoom_in();
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new("Treemap Zoom Out")
                        .shortcut_text(ctx.format_shortcut(&ZOOM_OUT)),
                )
                .clicked()
            {
                app.treemap_zoom_out();
                ui.close();
            }
            if ui
                .add(
                    egui::Button::new("Reset Treemap Zoom")
                        .shortcut_text(ctx.format_shortcut(&ZOOM_RESET)),
                )
                .clicked()
            {
                app.treemap_zoom_reset();
                ui.close();
            }
            if ui.button("Rebuild Treemap").clicked() {
                app.treemap_rebuild();
                ui.close();
            }
        });
        ui.menu_button("Go", |ui| {
            if ui
                .add(egui::Button::new("Up One Level").shortcut_text(ctx.format_shortcut(&GO_UP)))
                .clicked()
            {
                app.go_up();
                ui.close();
            }
            if ui.button("To Toplevel").clicked() {
                app.go_to_toplevel();
                ui.close();
            }
        });
        ui.menu_button("Clean Up", |ui| {
            let count = app.cleanups.len();
            for i in 0..count {
                let (title, hotkey) = match app.cleanups.get(i) {
                    Some(c) => (c.display_title(), c.hotkey.clone()),
                    None => continue,
                };
                let enabled = app.cleanup_applies(i);
                if ui
                    .add_enabled(enabled, egui::Button::new(title).shortcut_text(hotkey))
                    .clicked()
                {
                    app.request_cleanup(i);
                    ui.close();
                }
            }
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "Edit {}",
                    oxtree_native::config::cleanups_path().display()
                ))
                .weak()
                .small(),
            );
        });
        ui.menu_button("Help", |ui| {
            if ui.button("About OxTree").clicked() {
                app.about_open = true;
                ui.close();
            }
        });
    });
}

fn default_cache_path(app: &OxTreeApp) -> String {
    let home = oxtree_native::config::home_dir();
    let name = app
        .scan_path
        .as_ref()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "root".into());
    format!("{home}/{name}.cache.gz")
}
