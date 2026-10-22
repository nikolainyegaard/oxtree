//! Modal dialogs and the cleanup output window.

use std::path::PathBuf;

use eframe::egui::{self, Context, Id};

use oxtree_core::format;

use crate::app::OxTreeApp;

pub fn show(app: &mut OxTreeApp, ctx: &Context) {
    open_directory(app, ctx);
    cache_file(app, ctx);
    confirm(app, ctx);
    error(app, ctx);
    about(app, ctx);
    output(app, ctx);
}

fn open_directory(app: &mut OxTreeApp, ctx: &Context) {
    let Some(mut text) = app.open_dir_text.clone() else {
        return;
    };
    let mut close = false;
    let mut open = None;
    let mut browse = false;
    app.load_volumes();
    egui::Modal::new(Id::unique("open-dir")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading("Open Directory");
        ui.label("Directory to scan:");
        let resp = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(f32::INFINITY)
                .hint_text("/home/you"),
        );
        if app.tree.is_none() && app.scan.is_none() {
            resp.request_focus();
        }
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(8.0);
        ui.checkbox(
            &mut app.cross_filesystems,
            "Cross into other local filesystems (kernel tables and network mounts stay out)",
        );
        ui.add_space(8.0);
        ui.label("Mounted filesystems:");
        egui::Grid::new("volumes")
            .num_columns(6)
            .spacing([12.0, 2.0])
            .striped(true)
            .show(ui, |ui| {
                for label in ["Mount", "Type", "Size", "Used", "Free", ""] {
                    ui.label(egui::RichText::new(label).weak());
                }
                ui.end_row();
                for v in app.volumes.clone().unwrap_or_default() {
                    let path = v.mount.path.to_string_lossy().into_owned();
                    ui.label(&path);
                    ui.label(&v.mount.fs_type);
                    ui.label(format::size(v.size));
                    ui.label(format::size(v.used));
                    ui.label(format::size(v.avail));
                    if ui.button("Scan").clicked() {
                        open = Some(path);
                    }
                    ui.end_row();
                }
            });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(app.dir_picker.is_none(), egui::Button::new("Browse..."))
                .clicked()
            {
                browse = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Open").clicked() || enter {
                    open = Some(text.clone());
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
        });
    });
    app.open_dir_text = Some(text);
    if browse {
        app.browse_for_directory();
    }
    if let Some(path) = open {
        let path = path.trim();
        if path.is_empty() {
            return;
        }
        let expanded = if let Some(rest) = path.strip_prefix("~") {
            format!("{}{rest}", std::env::var("HOME").unwrap_or_default())
        } else {
            path.to_string()
        };
        app.open_dir_text = None;
        app.volumes = None;
        app.open_directory(PathBuf::from(expanded));
    } else if close && (app.tree.is_some() || app.scan.is_some()) {
        app.open_dir_text = None;
        app.volumes = None;
    }
}

fn cache_file(app: &mut OxTreeApp, ctx: &Context) {
    let Some((write, mut path)) = app.cache_dialog.clone() else {
        return;
    };
    let mut done = None;
    egui::Modal::new(Id::unique("cache-file")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading(if write {
            "Write Cache File"
        } else {
            "Read Cache File"
        });
        ui.label(if write {
            "QDirStat cache file to write (.gz for compressed):"
        } else {
            "QDirStat cache file to read:"
        });
        let resp = ui.add(egui::TextEdit::singleline(&mut path).desired_width(f32::INFINITY));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(if write { "Write" } else { "Read" }).clicked() || enter {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = Some(false);
            }
        });
    });
    match done {
        Some(true) => {
            app.cache_dialog = None;
            let p = PathBuf::from(path.trim());
            if write {
                app.write_cache(p);
            } else {
                app.read_cache(p);
            }
        }
        Some(false) => app.cache_dialog = None,
        None => app.cache_dialog = Some((write, path)),
    }
}

fn confirm(app: &mut OxTreeApp, ctx: &Context) {
    let Some(pending) = app.confirm.as_ref().map(|p| (p.index, p.node)) else {
        return;
    };
    let (index, node) = pending;
    let Some((tree, cleanup)) = app.tree.clone().zip(app.cleanups.get(index).cloned()) else {
        app.confirm = None;
        return;
    };
    let mut answer = None;
    egui::Modal::new(Id::unique("confirm")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading("Please Confirm");
        ui.add_space(4.0);
        ui.label(egui::RichText::new(cleanup.display_title()).strong());
        ui.label(format!(
            "for {} {}",
            if tree.is_dir_like(node) {
                "directory"
            } else {
                "file"
            },
            tree.path(node)
        ));
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(cleanup.expand(
                &tree.path(node),
                tree.name(node),
                &tree.dir_path(node),
                &app.desktop,
            ))
            .monospace()
            .weak(),
        );
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                answer = Some(true);
            }
            if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                answer = Some(false);
            }
        });
    });
    match answer {
        Some(true) => {
            app.confirm = None;
            app.run_cleanup(index, node);
        }
        Some(false) => app.confirm = None,
        None => {}
    }
}

fn error(app: &mut OxTreeApp, ctx: &Context) {
    let Some(text) = app.error.clone() else {
        return;
    };
    let mut close = false;
    egui::Modal::new(Id::unique("error")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.heading("Error");
        ui.label(text);
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("OK").clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape))
            {
                close = true;
            }
        });
    });
    if close {
        app.error = None;
    }
}

fn about(app: &mut OxTreeApp, ctx: &Context) {
    if !app.about_open {
        return;
    }
    let mut close = false;
    egui::Modal::new(Id::unique("about")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.heading(format!("OxTree {}", env!("CARGO_PKG_VERSION")));
        ui.label("Directory statistics: where the disk space went, as a tree and a treemap.");
        ui.add_space(6.0);
        ui.label("Licensed under the GNU General Public License version 2.");
        ui.label("The treemap algorithm, colour defaults, cleanup actions and cache file format come from QDirStat by Stefan Hundhammer.");
        ui.add_space(6.0);
        ui.hyperlink("https://github.com/nikolainyegaard/oxtree");
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        });
    });
    if close {
        app.about_open = false;
    }
}

fn output(app: &mut OxTreeApp, ctx: &Context) {
    let Some(out) = &mut app.output else { return };
    if !out.open {
        return;
    }
    let mut open = out.open;
    let title = out.title.clone();
    egui::Window::new(title)
        .open(&mut open)
        .default_size([720.0, 320.0])
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .max_height(ui.available_height() - 28.0)
                .show(ui, |ui| {
                    for (is_err, line) in &out.lines {
                        let text = egui::RichText::new(line).monospace();
                        ui.label(if *is_err {
                            text.color(egui::Color32::from_rgb(0xff, 0x40, 0x40))
                        } else {
                            text
                        });
                    }
                });
            ui.separator();
            match out.exit {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Running...");
                    });
                }
                Some(Some(0)) => {
                    ui.label("Finished.");
                }
                Some(Some(code)) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xff, 0x40, 0x40),
                        format!("Finished with exit code {code}."),
                    );
                }
                Some(None) => {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xff, 0x40, 0x40),
                        "The command was killed or could not be started.",
                    );
                }
            }
        });
    out.open = open;
}
