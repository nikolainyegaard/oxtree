//! The File View: every file (and optionally every directory) in one flat
//! list, largest first, capped at the largest N. The tab strip above the
//! central panel switches between this, the tree and the file types.

use eframe::egui::text::CCursor;
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use oxtree_core::dupes::DupMode;
use oxtree_core::flat::FlatSort;
use oxtree_core::{format, time};

use crate::app::{OxTreeApp, View};
use crate::columns;
use crate::theme;

pub const MAX_CHOICES: [(usize, &str); 4] = [
    (1_000, "1 000"),
    (10_000, "10 000"),
    (100_000, "100 000"),
    (0, "All"),
];

/// Column indexes into `app.file_columns`.
const PATH: usize = 0;
const SIZE: usize = 1;
const PCT: usize = 2;
const MTIME: usize = 3;
const DUPS: usize = 4;
const DUP_SIZE: usize = 5;

/// Tree View, File View and File Types tabs, the Reset button, plus the
/// file view's own controls.
pub fn tabs(app: &mut OxTreeApp, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut app.view, View::Tree, "Tree View");
        ui.selectable_value(&mut app.view, View::Files, "File View");
        ui.selectable_value(&mut app.view, View::Types, "File Types");
        if reset_button(ui).clicked() {
            app.reset_views();
        }
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
            ui.label("Duplicates");
            let dup_label = DupMode::ALL
                .iter()
                .find(|(m, _)| *m == fv.dup_mode)
                .map(|(_, l)| *l)
                .unwrap_or("Off");
            egui::ComboBox::from_id_salt("file-view-dupes")
                .selected_text(dup_label)
                .show_ui(ui, |ui| {
                    for (mode, label) in DupMode::ALL {
                        if ui.selectable_value(&mut fv.dup_mode, mode, label).changed() {
                            fv.dirty = true;
                        }
                    }
                })
                .response
                .on_hover_text("Files with the same name and size, or the same name, size and date, grouped; contents are not compared");
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

/// A square button with a painted eraser: reset, as distinct from the
/// circular arrow of a refresh.
fn reset_button(ui: &mut Ui) -> egui::Response {
    let resp = ui
        .add(egui::Button::new("").min_size(Vec2::splat(22.0)))
        .on_hover_text("Reset the views: close the tree, clear the search and filters, default sorting, column widths and treemap zoom");
    let r = resp.rect;
    let c = r.center();
    let color = ui.visuals().strong_text_color();
    let painter = ui.painter();
    // The eraser body, tilted: a rounded block with a darker rubbing end.
    let body = [
        Pos2::new(c.x - 6.0, c.y + 1.0),
        Pos2::new(c.x - 1.0, c.y - 4.0),
        Pos2::new(c.x + 5.0, c.y + 2.0),
        Pos2::new(c.x, c.y + 7.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        body.to_vec(),
        theme::ICON_FILE,
        Stroke::new(1.0, color),
    ));
    let tip = [
        Pos2::new(c.x - 1.0, c.y - 4.0),
        Pos2::new(c.x + 2.0, c.y - 7.0),
        Pos2::new(c.x + 8.0, c.y - 1.0),
        Pos2::new(c.x + 5.0, c.y + 2.0),
    ];
    painter.add(egui::Shape::convex_polygon(
        tip.to_vec(),
        theme::ICON_DIR,
        Stroke::new(1.0, color),
    ));
    // The streak it leaves.
    painter.line_segment(
        [
            Pos2::new(c.x - 8.0, c.y + 8.0),
            Pos2::new(c.x - 3.0, c.y + 8.0),
        ],
        Stroke::new(1.5, color),
    );
    resp
}

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    app.request_file_view(ui.ctx());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().strong_text_color();
    let weak = ui.visuals().weak_text_color();
    let dup_mode = app.file_view.dup_mode != DupMode::Off;
    let dupes = app
        .dupes
        .as_ref()
        .filter(|(m, _)| *m == app.file_view.dup_mode)
        .map(|(_, c)| c.clone());

    let (header_rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), theme::ROW_HEIGHT),
        Sense::hover(),
    );

    let total = app.file_view.rows.len();
    let selected = app.selected;
    let scroll_to = if app.file_view.scroll_to_selected {
        selected.and_then(|n| app.file_view.rows.iter().position(|&r| r == n))
    } else {
        None
    };
    app.file_view.scroll_to_selected = false;

    let mut clicked: Option<(u32, bool, egui::Modifiers)> = None; // node, double
    let mut area = egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt("file-rows");
    if let Some(i) = scroll_to {
        let spacing = ui.spacing().item_spacing.y;
        let offset = i as f32 * (theme::ROW_HEIGHT + spacing) - ui.available_height() / 2.0;
        area = area.vertical_scroll_offset(offset.max(0.0));
    }
    let shown_columns = if dup_mode { 6 } else { 4 };
    let out = area.show_rows(ui, theme::ROW_HEIGHT, total, |ui, range| {
        let Some(tree) = app.tree.clone() else { return };
        let root_size = tree.size(tree.root()).max(1);
        let width = ui
            .clip_rect()
            .width()
            .max(app.file_columns.widths.iter().take(shown_columns).sum());
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
            let painter = ui.painter_at(rect);
            columns::stripe(ui, &painter, rect, i);
            columns::row_background(
                ui,
                &painter,
                rect,
                app.marked.contains(&node),
                selected == Some(node),
                resp.hovered(),
            );
            let cols = &app.file_columns;
            let left = rect.left();
            let y = rect.center().y;
            let path_clip = Rect::from_min_max(
                Pos2::new(left + columns::PAD, rect.top()),
                Pos2::new(cols.text_right(left, PATH), rect.bottom()),
            );
            let path = tree.path(node);
            let color = if tree.is_dir_like(node) {
                weak
            } else {
                text_color
            };
            let clipped = painter.with_clip_rect(path_clip);
            let galley = clipped.layout_no_wrap(path.clone(), font.clone(), color);
            let pos = Pos2::new(left + columns::PAD, y - galley.size().y / 2.0);
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
            let number = |col: usize, text: String| {
                painter.text(
                    Pos2::new(cols.text_right(left, col), y),
                    Align2::RIGHT_CENTER,
                    text,
                    font.clone(),
                    text_color,
                );
            };
            number(SIZE, format::size(tree.size(node)));
            number(
                PCT,
                format::percent(tree.size(node) as f64 * 100.0 / root_size as f64),
            );
            number(MTIME, time::format(tree.mtime(node), app.utc_offset));
            if let Some(d) = &dupes {
                let count = d.get(node as usize).copied().unwrap_or(0);
                if count > 1 {
                    number(DUPS, format::grouped(count as u64));
                    number(
                        DUP_SIZE,
                        format::size((count as u64).saturating_mul(tree.size(node))),
                    );
                }
            }

            if resp.secondary_clicked() {
                app.select_for_menu(node);
            }
            resp.context_menu(|ui| crate::menus::context_menu(app, ui));
            let mods = ui.input(|i| i.modifiers);
            if resp.double_clicked() {
                clicked = Some((node, true, mods));
            } else if resp.clicked() {
                clicked = Some((node, false, mods));
            }
        }
    });

    let left = out.inner_rect.left() - out.state.clamped_offset().x;
    let sort = |s: FlatSort| (app.file_view.sort == s).then_some(app.file_view.ascending);
    let mut cells = vec![
        columns::HeaderCell {
            label: "Path",
            right: false,
            sort: sort(FlatSort::Name),
        },
        columns::HeaderCell {
            label: "Size",
            right: true,
            sort: sort(FlatSort::Size),
        },
        columns::HeaderCell {
            label: "%",
            right: true,
            sort: None,
        },
        columns::HeaderCell {
            label: "Last Modified",
            right: true,
            sort: sort(FlatSort::Mtime),
        },
    ];
    if dup_mode {
        cells.push(columns::HeaderCell {
            label: "Dups",
            right: true,
            sort: None,
        });
        cells.push(columns::HeaderCell {
            label: "Dup Size",
            right: true,
            sort: None,
        });
    }
    match columns::header(
        ui,
        &mut app.file_columns,
        header_rect,
        left,
        &cells,
        "files",
    ) {
        Some(0) => app.set_file_sort(FlatSort::Name),
        Some(1) => app.set_file_sort(FlatSort::Size),
        Some(3) => app.set_file_sort(FlatSort::Mtime),
        _ => {}
    }

    if let Some((node, double, mods)) = clicked {
        if double {
            // Jump to the item in the tree, ancestors expanded.
            app.select(node, true);
            app.view = View::Tree;
        } else if mods.ctrl || mods.shift {
            let order = app.file_view.rows.clone();
            app.select_in(node, mods, &order);
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
/// to the item in the tree; Ctrl+A selects every listed row.
pub fn handle_keys(app: &mut OxTreeApp, ctx: &egui::Context) {
    if ctx.memory(|m| m.focused().is_some()) || app.tree.is_none() {
        return;
    }
    let (up, down, home, end, enter, all) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Enter),
            i.modifiers.ctrl && i.key_pressed(egui::Key::A),
        )
    });
    if all {
        let order = app.file_view.rows.clone();
        app.select_all(&order);
        return;
    }
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
