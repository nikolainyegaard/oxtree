//! The treemap: one texture rendered off the UI thread, drawn stretched to
//! the panel, with hover tooltips and selection outlines painted on top.

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use oxtree_core::format;
use oxtree_core::layout::{hit_test, tile_of};
use oxtree_core::Kind;

use crate::app::OxTreeApp;
use crate::theme;

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(avail, Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);

    if app.tree.is_none() {
        return;
    }
    if rect.width() >= theme::TREEMAP_UPDATE_MIN_SIZE
        && rect.height() >= theme::TREEMAP_UPDATE_MIN_SIZE
    {
        let ppp = ui.ctx().pixels_per_point();
        let w = (rect.width() * ppp).round() as usize;
        let h = (rect.height() * ppp).round() as usize;
        app.request_treemap(ui.ctx(), w, h);
    }

    let Some(tex) = &app.treemap.texture else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Building treemap...",
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().weak_text_color(),
        );
        return;
    };
    painter.image(
        tex.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    let (tw, th) = app.treemap.texture_size;
    let to_tex = |p: Pos2| -> (f64, f64) {
        let fx = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let fy = ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        (fx as f64 * tw as f64, fy as f64 * th as f64)
    };
    let to_screen = |r: oxtree_core::layout::Rect| -> Rect {
        let sx = rect.width() / tw.max(1) as f32;
        let sy = rect.height() / th.max(1) as f32;
        Rect::from_min_size(
            Pos2::new(rect.left() + r.x as f32 * sx, rect.top() + r.y as f32 * sy),
            Vec2::new(r.w as f32 * sx, r.h as f32 * sy),
        )
    };

    let tiles = app.treemap.tiles.clone();
    let Some(tree) = app.tree.clone() else { return };

    // Hover
    app.treemap.hover = None;
    if let Some(pos) = resp.hover_pos() {
        let (x, y) = to_tex(pos);
        if let Some(t) = hit_test(&tiles, x, y) {
            app.treemap.hover = Some(t.node);
            let node = t.node;
            resp.clone().on_hover_ui_at_pointer(|ui| {
                ui.label(tree.path(node));
                ui.label(format::size(tree.size(node)));
            });
        }
    }

    // Clicks
    if resp.double_clicked() {
        if let Some(n) = app.treemap.hover {
            app.select(n, true);
            app.treemap_zoom_in();
        }
    } else if resp.clicked() {
        if let Some(n) = app.treemap.hover {
            app.select(n, true);
        }
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        let ctrl = ui.input(|i| i.modifiers.ctrl);
        if ctrl && scroll > 0.0 {
            app.treemap_zoom_in();
        } else if ctrl && scroll < 0.0 {
            app.treemap_zoom_out();
        }
    }

    // Selection outline: the selected node's tile, or the nearest laid-out
    // ancestor when the node itself was too small to draw.
    if let Some(sel) = app.selected {
        let mut n = Some(sel);
        while let Some(cur) = n {
            if let Some(t) = tile_of(&tiles, cur) {
                let r = to_screen(t.rect);
                let color = if cur == sel {
                    theme::SELECTED_ITEMS
                } else {
                    theme::CURRENT_ITEM
                };
                painter.rect_stroke(
                    r.shrink(0.5),
                    0.0,
                    Stroke::new(if cur == sel { 2.0 } else { 1.0 }, color),
                    egui::StrokeKind::Inside,
                );
                break;
            }
            n = tree.parent(cur);
        }
        // Parent directory frame, as QDirStat draws it, unless it is the whole map.
        if let Some(p) = tree.parent(sel) {
            let p = if tree.kind(p) == Kind::DotEntry {
                tree.parent(p).unwrap_or(p)
            } else {
                p
            };
            if p != app.treemap_root {
                if let Some(t) = tile_of(&tiles, p) {
                    painter.rect_stroke(
                        to_screen(t.rect),
                        0.0,
                        Stroke::new(1.0, theme::CURRENT_ITEM),
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
    }
}
