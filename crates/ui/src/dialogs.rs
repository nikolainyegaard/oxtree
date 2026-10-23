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
    export(app, ctx);
    preferences(app, ctx);
    output(app, ctx);
}

/// Edit, Preferences: every setting applies as it is changed and the file
/// is written when the dialog closes.
fn preferences(app: &mut OxTreeApp, ctx: &Context) {
    if !app.preferences_open {
        return;
    }
    let mut close = false;
    let mut reset = false;
    let mut theme_changed = false;
    let mut check = false;
    let mut update_now = false;
    egui::Modal::new(Id::unique("preferences")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading("Preferences");
        ui.add_space(6.0);
        egui::Grid::new("preferences-grid")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("At launch").strong());
                ui.vertical(|ui| {
                    use oxtree_core::settings::StartIn;
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.start_in, StartIn::Home, "Scan the home directory");
                        ui.radio_value(&mut app.start_in, StartIn::Ask, "Ask for a directory");
                    });
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut app.start_in, StartIn::Custom, "Scan this directory:");
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut app.start_path)
                                    .hint_text("/srv/data")
                                    .desired_width(260.0),
                            )
                            .changed()
                        {
                            app.start_in = StartIn::Custom;
                        }
                    });
                })
                .response
                .on_hover_text("What opens when OxTree starts without a directory on the command line");
                ui.end_row();

                ui.label(egui::RichText::new("Scanning").strong());
                ui.checkbox(
                    &mut app.cross_filesystems,
                    "Cross into other local filesystems",
                )
                .on_hover_text("Kernel tables and network mounts stay out either way; off shows one filesystem, the view for which disk is full");
                ui.end_row();

                ui.label(egui::RichText::new("Treemap").strong());
                ui.horizontal(|ui| {
                    let mut ms = app.rebuild_delay.as_millis() as u64;
                    ui.label("Rebuild delay");
                    if ui
                        .add(egui::DragValue::new(&mut ms).range(0..=2000).suffix(" ms"))
                        .on_hover_text("How long the treemap size has to hold still before a rebuild during a resize; 0 rebuilds every frame, 200 is QDirStat's value for slow machines")
                        .changed()
                    {
                        app.rebuild_delay = std::time::Duration::from_millis(ms);
                    }
                    ui.checkbox(&mut app.show_tile_names, "Names on tiles");
                });
                ui.end_row();

                ui.label(egui::RichText::new("Tables").strong());
                ui.horizontal(|ui| {
                    ui.label("Row height");
                    ui.add(egui::DragValue::new(&mut app.row_height).range(16.0..=40.0).fixed_decimals(0));
                    ui.add_space(12.0);
                    ui.label("Size decimals");
                    if ui
                        .add(egui::DragValue::new(&mut app.size_decimals).range(0..=3))
                        .changed()
                    {
                        oxtree_core::format::set_size_decimals(app.size_decimals);
                    }
                });
                ui.end_row();

                ui.label(egui::RichText::new("Appearance").strong());
                ui.horizontal(|ui| {
                    use oxtree_core::settings::Theme;
                    for (t, label) in [
                        (Theme::System, "Follow the desktop"),
                        (Theme::Light, "Light"),
                        (Theme::Dark, "Dark"),
                    ] {
                        if ui.radio_value(&mut app.theme, t, label).changed() {
                            theme_changed = true;
                        }
                    }
                });
                ui.end_row();

                ui.label(egui::RichText::new("Updates").strong());
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut app.check_updates, "Check automatically")
                            .on_hover_text("One request to the GitHub releases API through curl, on the schedule next to it; nothing is downloaded. Off, OxTree never touches the network");
                        let label = match app.check_interval_days {
                            0 => "on launch",
                            1 => "daily",
                            7 => "weekly",
                            30 => "monthly",
                            _ => "custom",
                        };
                        egui::ComboBox::from_id_salt("update-interval")
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut app.check_interval_days, 0, "on launch");
                                ui.selectable_value(&mut app.check_interval_days, 1, "daily");
                                ui.selectable_value(&mut app.check_interval_days, 7, "weekly");
                                ui.selectable_value(&mut app.check_interval_days, 30, "monthly");
                            });
                    });
                    ui.horizontal(|ui| {
                        let busy = app.update_job.is_some() || app.install_job.is_some();
                        if ui
                            .add_enabled(!busy, egui::Button::new("Check for Updates"))
                            .clicked()
                        {
                            check = true;
                        }
                        if app.update_job.is_some() {
                            ui.spinner();
                            ui.label("Checking...");
                        }
                        match &app.update_result {
                            Some(oxtree_native::update::UpdateResult::Newer(tag)) => {
                                ui.label(format!("{tag} is available, this is {}", crate::app::VERSION));
                                ui.hyperlink_to("Release page", oxtree_native::update::RELEASES_URL);
                            }
                            Some(oxtree_native::update::UpdateResult::Current) => {
                                ui.label(format!("OxTree {} is the latest release", crate::app::VERSION));
                            }
                            Some(oxtree_native::update::UpdateResult::Failed(e)) => {
                                ui.colored_label(crate::theme::DIR_READ_ERROR, e);
                            }
                            None => {}
                        }
                    });
                    if app.update_available().is_some() {
                        ui.horizontal(|ui| {
                            let can = app.install.as_ref().is_some_and(|i| i.can_update());
                            let busy = app.install_job.is_some();
                            if ui
                                .add_enabled(can && !busy, egui::Button::new("Update Now"))
                                .on_hover_text("Replaces this copy the way it was installed: through the package manager with the desktop's password prompt for a package, in place for an AppImage; then OxTree restarts")
                                .clicked()
                            {
                                update_now = true;
                            }
                            if let Some(i) = &app.install {
                                ui.label(egui::RichText::new(format!("This copy is {}", i.label())).weak());
                            }
                            if let Some(p) = &app.install_progress {
                                ui.spinner();
                                ui.label(p);
                            }
                        });
                        if let Some(e) = &app.install_error {
                            ui.colored_label(crate::theme::DIR_READ_ERROR, e);
                        }
                    }
                });
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!(
                "Saved to {} when this dialog closes. Column widths, the File View choices and the search history are saved with it.",
                oxtree_native::config::settings_path().display()
            ))
            .weak()
            .small(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .button("Reset to Defaults")
                .on_hover_text("Every setting back to its default, including column widths and the search history")
                .clicked()
            {
                reset = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter))
                {
                    close = true;
                }
            });
        });
    });
    if theme_changed {
        app.apply_theme(ctx);
    }
    if check {
        app.start_update_check();
    }
    if update_now {
        app.start_update();
    }
    if reset {
        app.reset_settings(ctx);
    }
    if close {
        app.preferences_open = false;
        app.rows_dirty = true;
        app.save_settings();
    }
}

fn export(app: &mut OxTreeApp, ctx: &Context) {
    let Some(mut d) = app.export_dialog.clone() else {
        return;
    };
    let Some(tree) = app.tree.clone() else {
        app.export_dialog = None;
        return;
    };
    let mut done = None;
    egui::Modal::new(Id::unique("export")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading("Export to CSV");
        ui.label(format!("Exporting {}", tree.path(d.root)));
        ui.add_space(4.0);
        ui.label("File to write:");
        let resp = ui.add(egui::TextEdit::singleline(&mut d.path).desired_width(f32::INFINITY));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut d.tsv, "Tab separated");
            ui.checkbox(&mut d.utc, "UTC timestamps");
            ui.checkbox(&mut d.percent_of_parent, "% of parent column");
        });
        ui.horizontal(|ui| {
            ui.label("Levels below the root to include, empty for all:");
            ui.add(egui::TextEdit::singleline(&mut d.max_depth).desired_width(40.0));
        });
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Export").clicked() || enter {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                done = Some(false);
            }
        });
    });
    match done {
        Some(true) => {
            app.export_dialog = None;
            if !d.path.trim().is_empty() {
                app.export_csv(&d);
            }
        }
        Some(false) => app.export_dialog = None,
        None => app.export_dialog = Some(d),
    }
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
            format!("{}{rest}", oxtree_native::config::home_dir())
        } else {
            path.to_string()
        };
        app.open_dir_text = None;
        app.volumes = None;
        app.open_directory(PathBuf::from(expanded));
    } else if close {
        // Cancel always closes the dialog; with nothing scanned yet that
        // leaves an empty window, and Ctrl+O brings the dialog back.
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
        let targets = app.cleanup_targets(index);
        if targets.len() > 1 {
            ui.label(format!("for {} items, one after the other:", targets.len()));
            for &t in targets.iter().take(8) {
                ui.label(egui::RichText::new(tree.path(t)).weak());
            }
            if targets.len() > 8 {
                ui.label(egui::RichText::new(format!("and {} more", targets.len() - 8)).weak());
            }
        } else {
            ui.label(format!(
                "for {} {}",
                if tree.is_dir_like(node) {
                    "directory"
                } else {
                    "file"
                },
                tree.path(node)
            ));
        }
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
            app.run_cleanup_on_selection(index);
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
