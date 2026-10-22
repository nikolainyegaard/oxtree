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
const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::F);

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
    if pressed(&FIND) {
        app.focus_search();
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
            .or_else(|| std::env::var("HOME").ok())
            .unwrap_or_else(|| "/".into());
        app.open_dir_text = Some(start);
    }
}

/// Right-click menu for the tree and the treemap: the Edit items, the
/// cleanups and the treemap actions, all acting on the current selection.
pub fn context_menu(app: &mut OxTreeApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    ui.set_min_width(220.0);
    let has_sel = app.selected.is_some();
    if ui
        .add_enabled(
            has_sel,
            egui::Button::new("Copy Path to Clipboard").shortcut_text(ctx.format_shortcut(&COPY)),
        )
        .clicked()
    {
        app.copy_path(&ctx);
        ui.close();
    }
    let can_trash = app.selected.map(|n| n != 0).unwrap_or(false) && app.scan.is_none();
    if ui
        .add_enabled(
            can_trash,
            egui::Button::new("Move to Trash").shortcut_text(ctx.format_shortcut(&TRASH)),
        )
        .clicked()
    {
        app.move_to_trash();
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
                    "Opens a second OxTree window as root on the same directory, through polkit",
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
            let can_trash = app.selected.map(|n| n != 0).unwrap_or(false) && app.scan.is_none();
            if ui
                .add_enabled(
                    can_trash,
                    egui::Button::new("Move to Trash").shortcut_text(ctx.format_shortcut(&TRASH)),
                )
                .clicked()
            {
                app.move_to_trash();
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
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let name = app
        .scan_path
        .as_ref()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "root".into());
    format!("{home}/{name}.cache.gz")
}
