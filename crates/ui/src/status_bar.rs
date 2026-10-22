//! One line at the bottom: what is happening or what to do next.

use eframe::egui::{self, Ui};
use oxtree_core::format;

use crate::app::OxTreeApp;

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(&app.status.text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(tree) = &app.tree {
                let mut text = format!(
                    "{} items, {}",
                    format::grouped(tree.total_items(0) as u64),
                    format::size(tree.size(0))
                );
                if let Some((avail, size)) = app.free_space {
                    text.push_str(&format!(
                        ", {} of {} free",
                        format::size(avail),
                        format::size(size)
                    ));
                }
                ui.label(egui::RichText::new(text).weak());
            }
            if app.scan.is_some() {
                ui.spinner();
            }
        });
    });
}
