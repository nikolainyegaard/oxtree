//! The directory tree as a virtual list: a header row and one painted row
//! per visible node, drawn only for the rows inside the scroll viewport.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use oxtree_core::{format, time, Kind, NO_NODE};

use crate::app::{Column, OxTreeApp};
use crate::columns;
use crate::theme;

/// Column indexes into `app.tree_columns`.
const NAME: usize = 0;
const BAR: usize = 1;
const PCT: usize = 2;
const SIZE: usize = 3;
const ITEMS: usize = 4;
const FILES: usize = 5;
const SUBDIRS: usize = 6;
const MTIME: usize = 7;

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    let row_h = app.row_height;
    let font = egui::TextStyle::Body.resolve(ui.style());
    // egui's plain label colour is a mid grey meant for non-interactive
    // text; rows are the content here, so they get the strong colour.
    let text_color = ui.visuals().strong_text_color();
    let weak = ui.visuals().weak_text_color();

    // The header band is reserved here and painted after the rows, since
    // it follows their sideways scroll and the rows' viewport is only known
    // once the scroll area has run.
    let (header_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::hover());

    // A treemap click earlier in this frame may have expanded ancestors;
    // the jump below has to see the rows as they are now, not as they were
    // when the frame started.
    if app.rows_dirty {
        app.rebuild_rows();
    }
    let total = app.rows.len();
    let selected = app.selected;
    let scroll_to = if app.scroll_to_selected {
        selected.and_then(|n| app.row_index_of(n))
    } else {
        None
    };
    app.scroll_to_selected = false;

    let mut clicked: Option<(u32, bool, bool, egui::Modifiers)> = None; // node, toggle, double
    let mut area = egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt("tree-rows");
    if let Some(i) = scroll_to {
        let spacing = ui.spacing().item_spacing.y;
        let offset = i as f32 * (row_h + spacing) - ui.available_height() / 2.0;
        area = area.vertical_scroll_offset(offset.max(0.0));
    }
    let out = area.show_rows(ui, row_h, total, |ui, range| {
        let Some(tree) = app.tree.clone() else { return };
        let width = columns::row_width(ui, &app.tree_columns);
        for i in range {
            let Some(row) = app.rows.get(i).copied() else {
                continue;
            };
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
            let painter = ui.painter_at(rect);
            columns::stripe(ui, &painter, rect, i);
            let left = rect.left();
            let cols = &app.tree_columns;
            let y = rect.center().y;
            if row.node == NO_NODE {
                free_space_row(app, &painter, rect, &font, weak);
                continue;
            }
            // The id is the node, not the row position, so that after rows
            // shift (expand, collapse, scroll) egui's hover and click state
            // still refers to the same item instead of whatever row now sits
            // at that index.
            let resp = ui.interact(
                rect,
                egui::Id::unique(("oxtree-row", row.node)),
                Sense::click(),
            );
            columns::row_background(
                ui,
                &painter,
                rect,
                app.marked.contains(&row.node),
                selected == Some(row.node),
                resp.hovered(),
            );
            let kind = tree.kind(row.node);
            let indent = left + columns::PAD + row.depth as f32 * theme::INDENT;

            // Expand marker and name
            let expandable = tree.child_count(row.node) > 0;
            let marker_rect =
                Rect::from_center_size(Pos2::new(indent + 6.0, y), Vec2::splat(row_h));
            if expandable {
                let open = app.expansion.is_open(row.node);
                triangle(
                    &painter,
                    Pos2::new(indent + 6.0, y),
                    4.5,
                    if open { Dir::Down } else { Dir::Right },
                    text_color,
                );
            }
            let name_color = match kind {
                Kind::UnreadDir => theme::DIR_READ_ERROR,
                Kind::MountPoint => weak,
                _ if app.subtree_errors.contains(&row.node) => theme::SUBTREE_READ_ERROR,
                _ => text_color,
            };
            let icon_x = indent + 16.0;
            icon(
                &painter,
                Pos2::new(icon_x + theme::ICON_SIZE / 2.0, y),
                kind,
                app.entered_mounts.contains(&row.node),
            );
            let name_x = icon_x + theme::ICON_SIZE + 5.0;
            let name_clip = Rect::from_min_max(
                Pos2::new(name_x, rect.top()),
                Pos2::new(cols.text_right(left, NAME), rect.bottom()),
            );
            let name_painter = painter.with_clip_rect(name_clip);
            let shown = if row.node == tree.root() {
                tree.root_path().to_string()
            } else {
                tree.name(row.node).to_string()
            };
            name_painter.text(
                Pos2::new(name_x, y),
                Align2::LEFT_CENTER,
                shown,
                font.clone(),
                name_color,
            );

            // Percent bar
            let bar_x = cols.x(left, BAR) + columns::PAD;
            let bar_rect = Rect::from_min_max(
                Pos2::new(bar_x, rect.top() + 3.0),
                Pos2::new(cols.text_right(left, BAR), rect.bottom() - 3.0),
            );
            match kind {
                Kind::UnreadDir => {
                    painter.text(
                        Pos2::new(bar_x, y),
                        Align2::LEFT_CENTER,
                        "[Not Read]",
                        font.clone(),
                        theme::DIR_READ_ERROR,
                    );
                }
                Kind::MountPoint => {
                    painter.text(
                        Pos2::new(bar_x, y),
                        Align2::LEFT_CENTER,
                        "[Mount Point]",
                        font.clone(),
                        weak,
                    );
                }
                _ => {
                    if let Some(p) = tree.percent(row.node) {
                        painter.rect_filled(bar_rect, 0.0, theme::PERCENT_BAR_BACKGROUND);
                        let fill = Rect::from_min_size(
                            bar_rect.min,
                            Vec2::new(
                                bar_rect.width() * (p as f32 / 100.0).clamp(0.0, 1.0),
                                bar_rect.height(),
                            ),
                        );
                        painter.rect_filled(
                            fill,
                            0.0,
                            theme::percent_bar_color(row.depth.saturating_sub(1)),
                        );
                        painter.rect_stroke(
                            bar_rect,
                            0.0,
                            Stroke::new(1.0, Color32::from_gray(0x60)),
                            egui::StrokeKind::Inside,
                        );
                        painter.text(
                            Pos2::new(cols.text_right(left, PCT), y),
                            Align2::RIGHT_CENTER,
                            format::percent(p),
                            font.clone(),
                            text_color,
                        );
                    }
                }
            }

            // Numbers
            let number = |col: usize, text: String| {
                painter.text(
                    Pos2::new(cols.text_right(left, col), y),
                    Align2::RIGHT_CENTER,
                    text,
                    font.clone(),
                    text_color,
                );
            };
            number(SIZE, format::size(tree.size(row.node)));
            if tree.is_dir_like(row.node) && kind != Kind::DotEntry {
                number(ITEMS, format::grouped(tree.total_items(row.node) as u64));
                number(FILES, format::grouped(tree.total_files(row.node) as u64));
                number(
                    SUBDIRS,
                    format::grouped(tree.total_subdirs(row.node) as u64),
                );
            } else if kind == Kind::DotEntry {
                number(ITEMS, format::grouped(tree.total_items(row.node) as u64));
            }
            number(MTIME, time::format(tree.mtime(row.node), app.utc_offset));

            if resp.secondary_clicked() {
                app.select_for_menu(row.node);
            }
            resp.context_menu(|ui| crate::menus::context_menu(app, ui));
            let mods = ui.input(|i| i.modifiers);
            if resp.double_clicked() {
                clicked = Some((row.node, false, true, mods));
            } else if resp.clicked() {
                let on_marker = resp
                    .interact_pointer_pos()
                    .map(|p| marker_rect.contains(p))
                    .unwrap_or(false);
                clicked = Some((row.node, on_marker && expandable, false, mods));
            }
        }
    });

    // Header, shifted by the sideways scroll so it stays over its columns.
    let left = out.inner_rect.left() - out.state.clamped_offset().x;
    let sort = |c: Column| (app.sort.column == c).then_some(app.sort.ascending);
    let cells = [
        columns::HeaderCell {
            label: "Name",
            right: false,
            sort: sort(Column::Name),
        },
        columns::HeaderCell {
            label: "Subtree Percentage",
            right: false,
            sort: sort(Column::PercentBar),
        },
        columns::HeaderCell {
            label: "%",
            right: true,
            sort: sort(Column::PercentNum),
        },
        columns::HeaderCell {
            label: "Size",
            right: true,
            sort: sort(Column::Size),
        },
        columns::HeaderCell {
            label: "Items",
            right: true,
            sort: sort(Column::Items),
        },
        columns::HeaderCell {
            label: "Files",
            right: true,
            sort: sort(Column::Files),
        },
        columns::HeaderCell {
            label: "Subdirs",
            right: true,
            sort: sort(Column::Subdirs),
        },
        columns::HeaderCell {
            label: "Last Modified",
            right: true,
            sort: sort(Column::Mtime),
        },
    ];
    if let Some(i) = columns::header(ui, &mut app.tree_columns, header_rect, left, &cells, "tree") {
        const ORDER: [Column; 8] = [
            Column::Name,
            Column::PercentBar,
            Column::PercentNum,
            Column::Size,
            Column::Items,
            Column::Files,
            Column::Subdirs,
            Column::Mtime,
        ];
        if let Some(c) = ORDER.get(i) {
            app.set_sort(*c);
        }
    }

    if let Some((node, toggle, double, mods)) = clicked {
        if toggle {
            app.toggle_expand(node);
        } else if mods.ctrl || mods.shift {
            let order = app.row_order();
            app.select_in(node, mods, &order);
        } else {
            app.select(node, false);
            if double
                && app
                    .tree
                    .as_ref()
                    .map(|t| t.child_count(node) > 0)
                    .unwrap_or(false)
            {
                app.toggle_expand(node);
            }
        }
    }
}

/// The `<Free Space>` row under the root: the volume's available bytes and
/// their share of the volume, in the weak colour since it is not an item.
fn free_space_row(
    app: &OxTreeApp,
    painter: &egui::Painter,
    rect: Rect,
    font: &egui::FontId,
    weak: Color32,
) {
    let Some((avail, size)) = app.free_space else {
        return;
    };
    let cols = &app.tree_columns;
    let left = rect.left();
    let y = rect.center().y;
    let indent = left + columns::PAD + theme::INDENT;
    let icon_x = indent + 16.0;
    let disk = Rect::from_center_size(
        Pos2::new(icon_x + theme::ICON_SIZE / 2.0, y),
        Vec2::new(theme::ICON_SIZE, theme::ICON_SIZE * 0.6),
    );
    painter.rect_stroke(
        disk,
        2.0,
        Stroke::new(1.0, theme::ICON_MOUNT),
        egui::StrokeKind::Inside,
    );
    painter.text(
        Pos2::new(icon_x + theme::ICON_SIZE + 5.0, y),
        Align2::LEFT_CENTER,
        "<Free Space>",
        font.clone(),
        weak,
    );
    let bar_x = cols.x(left, BAR) + columns::PAD;
    let bar_rect = Rect::from_min_max(
        Pos2::new(bar_x, rect.top() + 3.0),
        Pos2::new(cols.text_right(left, BAR), rect.bottom() - 3.0),
    );
    let p = if size > 0 {
        avail as f64 * 100.0 / size as f64
    } else {
        0.0
    };
    painter.rect_filled(bar_rect, 0.0, theme::PERCENT_BAR_BACKGROUND);
    let fill = Rect::from_min_size(
        bar_rect.min,
        Vec2::new(
            bar_rect.width() * (p as f32 / 100.0).clamp(0.0, 1.0),
            bar_rect.height(),
        ),
    );
    painter.rect_filled(fill, 0.0, theme::FREE_SPACE);
    painter.rect_stroke(
        bar_rect,
        0.0,
        Stroke::new(1.0, Color32::from_gray(0x60)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        Pos2::new(cols.text_right(left, PCT), y),
        Align2::RIGHT_CENTER,
        format::percent(p),
        font.clone(),
        weak,
    );
    painter.text(
        Pos2::new(cols.text_right(left, SIZE), y),
        Align2::RIGHT_CENTER,
        format::size(avail),
        font.clone(),
        weak,
    );
    painter.text(
        Pos2::new(cols.text_right(left, MTIME), y),
        Align2::RIGHT_CENTER,
        format!("of {}", format::size(size)),
        font.clone(),
        weak,
    );
}

#[derive(Clone, Copy)]
enum Dir {
    Up,
    Down,
    Right,
}

/// A filled triangle drawn with the painter. The default fonts have no
/// glyph for the small triangle code points, so they rendered as boxes.
fn triangle(painter: &egui::Painter, center: Pos2, r: f32, dir: Dir, color: Color32) {
    let points = match dir {
        Dir::Up => vec![
            Pos2::new(center.x, center.y - r),
            Pos2::new(center.x + r, center.y + r * 0.7),
            Pos2::new(center.x - r, center.y + r * 0.7),
        ],
        Dir::Down => vec![
            Pos2::new(center.x - r, center.y - r * 0.7),
            Pos2::new(center.x + r, center.y - r * 0.7),
            Pos2::new(center.x, center.y + r),
        ],
        Dir::Right => vec![
            Pos2::new(center.x - r * 0.7, center.y - r),
            Pos2::new(center.x + r, center.y),
            Pos2::new(center.x - r * 0.7, center.y + r),
        ],
    };
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

/// A small painted icon per node kind, in the spirit of QDirStat's tree
/// icons: an orange folder, a white page, a page with an arrow for a
/// symlink, a disk for a mount point, a red-edged folder for one that could
/// not be read, a lighter folder for the `<Files>` entry.
pub fn icon(painter: &egui::Painter, center: Pos2, kind: Kind, entered_mount: bool) {
    let s = theme::ICON_SIZE;
    let r = Rect::from_center_size(center, Vec2::new(s, s * 0.8));
    match kind {
        Kind::Dir | Kind::DotEntry | Kind::UnreadDir => {
            let (body, tab) = if kind == Kind::DotEntry {
                (
                    theme::ICON_DIR.gamma_multiply(0.6),
                    theme::ICON_DIR_TAB.gamma_multiply(0.6),
                )
            } else {
                (theme::ICON_DIR, theme::ICON_DIR_TAB)
            };
            let tab_rect = Rect::from_min_size(r.min, Vec2::new(s * 0.45, s * 0.25));
            painter.rect_filled(tab_rect, 1.0, tab);
            let body_rect = Rect::from_min_max(Pos2::new(r.min.x, r.min.y + s * 0.18), r.max);
            painter.rect_filled(body_rect, 1.5, body);
            if kind == Kind::UnreadDir {
                painter.rect_stroke(
                    body_rect,
                    1.5,
                    Stroke::new(1.5, theme::DIR_READ_ERROR),
                    egui::StrokeKind::Inside,
                );
            }
            if entered_mount {
                // A small disk on the folder: another filesystem, read.
                let badge = Rect::from_min_max(
                    Pos2::new(r.max.x - s * 0.55, r.max.y - s * 0.3),
                    Pos2::new(r.max.x + 1.0, r.max.y + 1.0),
                );
                painter.rect_filled(badge, 1.0, theme::ICON_MOUNT);
                painter.rect_stroke(
                    badge,
                    1.0,
                    Stroke::new(1.0, Color32::from_gray(0x30)),
                    egui::StrokeKind::Outside,
                );
            }
        }
        Kind::MountPoint => {
            let disk = Rect::from_center_size(center, Vec2::new(s, s * 0.6));
            painter.rect_filled(disk, 2.0, theme::ICON_MOUNT);
            painter.hline(
                disk.min.x + 2.0..=disk.max.x - 2.0,
                disk.center().y,
                Stroke::new(1.0, Color32::from_gray(0x50)),
            );
        }
        Kind::File | Kind::Symlink | Kind::Special => {
            let page = Rect::from_center_size(center, Vec2::new(s * 0.72, s));
            let fold = s * 0.3;
            let points = vec![
                page.min,
                Pos2::new(page.max.x - fold, page.min.y),
                Pos2::new(page.max.x, page.min.y + fold),
                Pos2::new(page.max.x, page.max.y),
                Pos2::new(page.min.x, page.max.y),
            ];
            painter.add(egui::Shape::convex_polygon(
                points,
                theme::ICON_FILE,
                Stroke::new(1.0, theme::ICON_FILE_EDGE),
            ));
            if kind == Kind::Symlink {
                let a = Pos2::new(page.min.x + 2.0, page.max.y - 3.0);
                let b = Pos2::new(page.max.x - 2.0, page.min.y + fold + 2.0);
                painter.line_segment([a, b], Stroke::new(2.0, theme::ICON_SYMLINK));
                triangle(painter, b, 2.5, Dir::Up, theme::ICON_SYMLINK);
            }
            if kind == Kind::Special {
                painter.circle_filled(page.center(), 2.5, theme::ICON_FILE_EDGE);
            }
        }
    }
}

/// The sort direction marker on a column header, shared with the file view.
pub fn sort_marker(painter: &egui::Painter, center: Pos2, ascending: bool, color: Color32) {
    triangle(
        painter,
        center,
        4.0,
        if ascending { Dir::Up } else { Dir::Down },
        color,
    );
}

/// Arrow keys, Enter and Backspace move the selection when no text field
/// has focus.
pub fn handle_keys(app: &mut OxTreeApp, ctx: &egui::Context) {
    if ctx.memory(|m| m.focused().is_some()) || app.tree.is_none() {
        return;
    }
    let (up, down, left, right, home, end, enter, all) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Enter),
            i.modifiers.ctrl && i.key_pressed(egui::Key::A),
        )
    });
    if all {
        let order = app.row_order();
        app.select_all(&order);
        return;
    }
    if !(up || down || left || right || home || end || enter) {
        return;
    }
    let Some(sel) = app.selected else {
        if let Some(r) = app.rows.first() {
            app.select(r.node, false);
        }
        return;
    };
    let Some(idx) = app.row_index_of(sel) else {
        return;
    };
    let expanded = app.expansion.is_open(sel);
    let has_children = app
        .tree
        .as_ref()
        .map(|t| t.child_count(sel) > 0)
        .unwrap_or(false);
    let parent = app.tree.as_ref().and_then(|t| t.parent(sel));
    let mut target = None;
    if up && idx > 0 {
        target = app.rows.get(idx - 1).map(|r| r.node);
    } else if down {
        target = app.rows.get(idx + 1).map(|r| r.node);
    } else if home {
        target = app.rows.first().map(|r| r.node);
    } else if end {
        target = app.rows.last().map(|r| r.node);
    } else if right && has_children {
        if expanded {
            target = app.rows.get(idx + 1).map(|r| r.node);
        } else {
            app.set_expanded(sel, true);
        }
    } else if left {
        if expanded && has_children {
            app.set_expanded(sel, false);
        } else {
            target = parent;
        }
    } else if enter && has_children {
        app.toggle_expand(sel);
    }
    if let Some(t) = target.filter(|&t| t != NO_NODE) {
        app.select(t, false);
        app.scroll_to_selected = true;
    }
}
