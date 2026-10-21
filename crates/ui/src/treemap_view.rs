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
    // The texture is drawn at its own pixel size, anchored top left, never
    // stretched. While a resize settles, the map keeps its shape and the
    // uncovered strip shows the background, as QDirStat's view does.
    let (tw, th) = app.treemap.texture_size;
    let ppp = ui.ctx().pixels_per_point();
    let tex_points = Vec2::new(tw as f32 / ppp, th as f32 / ppp);
    let drawn = Rect::from_min_size(rect.min, tex_points).intersect(rect);
    let uv_max = Pos2::new(
        (drawn.width() / tex_points.x.max(1.0)).min(1.0),
        (drawn.height() / tex_points.y.max(1.0)).min(1.0),
    );
    painter.image(
        tex.id(),
        drawn,
        Rect::from_min_max(Pos2::ZERO, uv_max),
        egui::Color32::WHITE,
    );

    let to_tex = |p: Pos2| -> (f64, f64) {
        (
            ((p.x - rect.left()) * ppp) as f64,
            ((p.y - rect.top()) * ppp) as f64,
        )
    };
    let to_screen = |r: oxtree_core::layout::Rect| -> Rect {
        Rect::from_min_size(
            Pos2::new(
                rect.left() + r.x as f32 / ppp,
                rect.top() + r.y as f32 / ppp,
            ),
            Vec2::new(r.w as f32 / ppp, r.h as f32 / ppp),
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
