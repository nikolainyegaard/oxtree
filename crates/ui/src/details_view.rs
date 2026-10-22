//! The details panel for the selected item, with QDirStat's labels.

use eframe::egui::{self, Ui};
use oxtree_core::{format, time, Kind};

use crate::app::OxTreeApp;

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    ui.heading("Details");
    ui.add_space(4.0);
    let Some((tree, n)) = app.tree.clone().zip(app.selected) else {
        ui.label(egui::RichText::new("No current item").weak());
        return;
    };
    let kind = tree.kind(n);
    let name = if n == tree.root() {
        tree.root_path().to_string()
    } else {
        tree.name(n).to_string()
    };
    ui.label(egui::RichText::new(name).strong());
    ui.label(egui::RichText::new(kind.label()).weak());
    ui.add_space(6.0);

    let mtime = time::format(tree.mtime(n), app.utc_offset);
    let category = app
        .categorizer
        .category(tree.name(n), kind, tree.mode(n))
        .map(|c| c.name.clone());

    egui::Grid::new("details-grid")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            let row = |ui: &mut Ui, label: &str, value: String| {
                ui.label(egui::RichText::new(label).weak());
                ui.label(value);
                ui.end_row();
            };
            if tree.is_dir_like(n) {
                row(ui, "Total Size:", format::size(tree.size(n)));
                if kind != Kind::DotEntry {
                    row(ui, "Items:", format::grouped(tree.total_items(n) as u64));
                    row(ui, "Files:", format::grouped(tree.total_files(n) as u64));
                    row(
                        ui,
                        "Subdirs:",
                        format::grouped(tree.total_subdirs(n) as u64),
                    );
                } else {
                    row(ui, "Files:", format::grouped(tree.total_files(n) as u64));
                }
                row(ui, "Last Modified:", mtime);
                if kind == Kind::UnreadDir {
                    row(ui, "Status:", "[Permission Denied]".into());
                }
                if kind == Kind::MountPoint {
                    row(ui, "Status:", "Other filesystem, not read".into());
                }
                if kind == Kind::MountPoint || app.entered_mounts.contains(&n) {
                    if let Some(m) = app.mounts.at(std::path::Path::new(&tree.path(n))) {
                        row(ui, "Filesystem:", format!("{} on {}", m.fs_type, m.device));
                    }
                }
            } else {
                row(ui, "Size:", format::size(tree.size(n)));
                row(ui, "Allocated:", format::bytes(tree.size(n)));
                row(ui, "Last Modified:", mtime);
                if let Some(c) = category {
                    row(ui, "MIME Category:", c);
                }
            }
            if kind != Kind::DotEntry {
                row(
                    ui,
                    "Permissions:",
                    format!(
                        "{}  {}",
                        format::permissions(tree.mode(n)),
                        format::octal_permissions(tree.mode(n))
                    ),
                );
            }
            if let Some(p) = tree.percent(n) {
                row(ui, "Of Parent:", format::percent(p));
            }
        });
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tree.path(n)).small().weak());
}
