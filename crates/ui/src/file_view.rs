//! The File View: every file (and optionally every directory) in one flat
//! list, largest first, capped at the largest N. The tab strip above the
//! central panel switches between this and the tree.

use eframe::egui::text::CCursor;
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Ui, Vec2};
use oxtree_core::flat::FlatSort;
use oxtree_core::{format, time};

use crate::app::{OxTreeApp, View};
use crate::theme;

pub const MAX_CHOICES: [(usize, &str); 4] = [
    (1_000, "1 000"),
    (10_000, "10 000"),
    (100_000, "100 000"),
    (0, "All"),
];

/// Tree View and File View tabs, plus the file view's own controls.
pub fn tabs(app: &mut OxTreeApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.view, View::Tree, "Tree View");
        ui.selectable_value(&mut app.view, View::Files, "File View");
        if app.view == View::Files {
            ui.separator();
            ui.label("Show");
            let fv = &mut app.file_view;
            let current = MAX_CHOICES
                .iter()
                .find(|(n, _)| *n == fv.max)
                .map(|(_, l)| *l)
                .unwrap_or("All");
            egui::ComboBox::from_id_salt("file-view-max")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (n, label) in MAX_CHOICES {
                        if ui.selectable_value(&mut fv.max, n, label).changed() {
                            fv.dirty = true;
                        }
                    }
                });
            if ui.checkbox(&mut fv.include_dirs, "Folders").changed() {
                fv.dirty = true;
            }
            ui.separator();
            let box_id = ui.scope_id().with("file-search");
            let edit = egui::TextEdit::singleline(&mut fv.search)
                .id(box_id)
                .hint_text("Search: name, *.mp3, >100m, <2024/01/01, /regex")
                .desired_width(260.0);
            let resp = ui.add(edit);
            if fv.focus_search {
                fv.focus_search = false;
                resp.request_focus();
            }
            if resp.changed() {
                fv.dirty = true;
            }
            if ui.radio_value(&mut fv.match_path, false, "Name").changed()
                || ui.radio_value(&mut fv.match_path, true, "Path").changed()
            {
                fv.dirty = true;
            }
            if let Some(tree) = &app.tree {
                let shown = app.file_view.rows.len();
                let total = tree.total_files(tree.root())
                    + if app.file_view.include_dirs {
                        tree.total_subdirs(tree.root())
                    } else {
                        0
                    };
                ui.label(
                    egui::RichText::new(format!(
                        "{} of {}",
                        format::grouped(shown as u64),
                        format::grouped(total as u64)
                    ))
                    .weak(),
                );
            }
        }
    });
    if app.view == View::Files {
        if let Some(err) = &app.file_view.search_error {
            ui.label(egui::RichText::new(err.as_str()).color(theme::DIR_READ_ERROR));
        }
    }
}

struct Columns {
    path_w: f32,
    size_x: f32,
    pct_x: f32,
    right: f32,
}

fn columns(left: f32, width: f32) -> Columns {
    let right = left + width;
    let mtime_x = right - theme::COL_MTIME;
    let pct_x = mtime_x - theme::COL_GAP - theme::COL_PERCENT_NUM;
    let size_x = pct_x - theme::COL_GAP - theme::COL_SIZE;
    Columns {
        path_w: (size_x - theme::COL_GAP - left).max(80.0),
        size_x,
        pct_x,
        right,
    }
}

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    app.request_file_view(ui.ctx());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();

    // Header
    let (header_rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), theme::ROW_HEIGHT),
        Sense::hover(),
    );
    let cols = columns(header_rect.left() + 4.0, header_rect.width() - 8.0);
    let painter = ui.painter_at(header_rect);
    painter.rect_filled(header_rect, 0.0, ui.visuals().faint_bg_color);
    let y = header_rect.center().y;
    let headers: [(Option<FlatSort>, f32, f32, Align2, &str); 4] = [
        (
            Some(FlatSort::Name),
            header_rect.left() + 4.0,
            cols.path_w,
            Align2::LEFT_CENTER,
            "Path",
        ),
        (
            Some(FlatSort::Size),
            cols.size_x,
            theme::COL_SIZE,
            Align2::RIGHT_CENTER,
            "Size",
        ),
        (
            None,
            cols.pct_x,
            theme::COL_PERCENT_NUM,
            Align2::RIGHT_CENTER,
            "%",
        ),
        (
            Some(FlatSort::Mtime),
            cols.right - theme::COL_MTIME,
            theme::COL_MTIME,
            Align2::RIGHT_CENTER,
            "Last Modified",
        ),
    ];
    for (sort, x, w, align, label) in headers {
        let rect = Rect::from_min_size(
            Pos2::new(x, header_rect.top()),
            Vec2::new(w, theme::ROW_HEIGHT),
        );
        let resp = ui.interact(rect, ui.scope_id().with(("fhdr", label)), Sense::click());
        let sorted_here = sort.is_some() && sort == Some(app.file_view.sort);
        let anchor = if align == Align2::LEFT_CENTER {
            Pos2::new(x, y)
        } else {
            Pos2::new(x + w - if sorted_here { 12.0 } else { 0.0 }, y)
        };
        if sorted_here {
            let cx = if align == Align2::LEFT_CENTER {
                x + w - 8.0
            } else {
                x + w - 5.0
            };
            crate::tree_view::sort_marker(
                &painter,
                Pos2::new(cx, y),
                app.file_view.ascending,
                weak,
            );
        }
        painter.text(
            anchor,
            align,
            label,
            font.clone(),
            if resp.hovered() { text_color } else { weak },
        );
        if resp.clicked() {
            if let Some(s) = sort {
                app.set_file_sort(s);
            }
        }
    }

    let total = app.file_view.rows.len();
    let selected = app.selected;
    let scroll_to = if app.file_view.scroll_to_selected {
        selected.and_then(|n| app.file_view.rows.iter().position(|&r| r == n))
    } else {
        None
    };
    app.file_view.scroll_to_selected = false;

    let mut clicked: Option<(u32, bool)> = None; // node, double
    let mut area = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("file-rows");
    if let Some(i) = scroll_to {
        let spacing = ui.spacing().item_spacing.y;
        let offset = i as f32 * (theme::ROW_HEIGHT + spacing) - ui.available_height() / 2.0;
        area = area.vertical_scroll_offset(offset.max(0.0));
    }
    area.show_rows(ui, theme::ROW_HEIGHT, total, |ui, range| {
        let Some(tree) = app.tree.clone() else { return };
        let root_size = tree.size(tree.root()).max(1);
        let width = ui.available_width();
        for i in range {
            let Some(node) = app.file_view.rows.get(i).copied() else {
                continue;
            };
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, theme::ROW_HEIGHT), Sense::hover());
            let resp = ui.interact(
                rect,
                egui::Id::unique(("oxtree-file-row", node)),
                Sense::click(),
            );
            let cols = columns(rect.left() + 4.0, rect.width() - 8.0);
            let painter = ui.painter_at(rect);
            let is_selected = selected == Some(node);
            if is_selected {
                painter.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
            } else if resp.hovered() {
                painter.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let y = rect.center().y;
            let path_clip = Rect::from_min_max(
                Pos2::new(rect.left() + 4.0, rect.top()),
                Pos2::new(rect.left() + 4.0 + cols.path_w, rect.bottom()),
            );
            let path = tree.path(node);
            let color = if tree.is_dir_like(node) {
                weak
            } else {
                text_color
            };
            let clipped = painter.with_clip_rect(path_clip);
            let galley = clipped.layout_no_wrap(path.clone(), font.clone(), color);
            let pos = Pos2::new(rect.left() + 4.0, y - galley.size().y / 2.0);
            if let Some((a, b)) = search_highlight(app, &path, tree.name(node)) {
                let x0 = galley.pos_from_cursor(CCursor::new(a)).min.x;
                let x1 = galley.pos_from_cursor(CCursor::new(b)).min.x;
                clipped.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(pos.x + x0, rect.top() + 2.0),
                        Pos2::new(pos.x + x1, rect.bottom() - 2.0),
                    ),
                    2.0,
                    theme::SEARCH_HIGHLIGHT,
                );
            }
            clipped.galley(pos, galley, color);
            painter.text(
                Pos2::new(cols.size_x + theme::COL_SIZE, y),
                Align2::RIGHT_CENTER,
                format::size(tree.size(node)),
                font.clone(),
                text_color,
            );
            painter.text(
                Pos2::new(cols.pct_x + theme::COL_PERCENT_NUM, y),
                Align2::RIGHT_CENTER,
                format::percent(tree.size(node) as f64 * 100.0 / root_size as f64),
                font.clone(),
                text_color,
            );
            painter.text(
                Pos2::new(cols.right, y),
                Align2::RIGHT_CENTER,
                time::format(tree.mtime(node), app.utc_offset),
                font.clone(),
                text_color,
            );

            if resp.secondary_clicked() {
                app.select(node, false);
            }
            resp.context_menu(|ui| crate::menus::context_menu(app, ui));
            if resp.double_clicked() {
                clicked = Some((node, true));
            } else if resp.clicked() {
                clicked = Some((node, false));
            }
        }
    });

    if let Some((node, double)) = clicked {
        if double {
            // Jump to the item in the tree, ancestors expanded.
            app.select(node, true);
            app.view = View::Tree;
        } else {
            app.select(node, false);
        }
    }
}

/// Character range to highlight in the displayed path: the match in the
/// whole path in path mode, else the match in the name, shifted to where
/// the name sits at the end of the path.
fn search_highlight(app: &OxTreeApp, path: &str, name: &str) -> Option<(usize, usize)> {
    let q = app.file_view.query.as_ref()?;
    if app.file_view.match_path {
        return q.highlight(path);
    }
    let (a, b) = q.highlight(name)?;
    let offset = path.chars().count().checked_sub(name.chars().count())?;
    Some((offset + a, offset + b))
}

/// Up, Down, Home and End move the selection through the list; Enter jumps
/// to the item in the tree.
pub fn handle_keys(app: &mut OxTreeApp, ctx: &egui::Context) {
    if ctx.memory(|m| m.focused().is_some()) || app.tree.is_none() {
        return;
    }
    let (up, down, home, end, enter) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Enter),
        )
    });
    if !(up || down || home || end || enter) {
        return;
    }
    let rows = &app.file_view.rows;
    let idx = app.selected.and_then(|s| rows.iter().position(|&r| r == s));
    let target = match (idx, up, down, home, end) {
        (_, _, _, true, _) => rows.first().copied(),
        (_, _, _, _, true) => rows.last().copied(),
        (None, _, _, _, _) => rows.first().copied(),
        (Some(i), true, _, _, _) if i > 0 => rows.get(i - 1).copied(),
        (Some(i), _, true, _, _) => rows.get(i + 1).copied(),
        _ => None,
    };
    if let Some(t) = target {
        app.select(t, false);
        app.file_view.scroll_to_selected = true;
    } else if enter {
        if let Some(s) = app.selected {
            app.select(s, true);
            app.view = View::Tree;
        }
    }
}
