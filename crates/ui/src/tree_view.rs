//! The directory tree as a virtual list: a header row and one painted row
//! per visible node, drawn only for the rows inside the scroll viewport.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use oxtree_core::{format, time, Kind};

use crate::app::{Column, OxTreeApp};
use crate::theme;

struct Columns {
    name_w: f32,
    bar_x: f32,
    pct_x: f32,
    size_x: f32,
    items_x: f32,
    files_x: f32,
    subdirs_x: f32,
    mtime_x: f32,
    right: f32,
}

fn columns(left: f32, width: f32) -> Columns {
    let right = left + width;
    let mtime_x = right - theme::COL_MTIME;
    let subdirs_x = mtime_x - theme::COL_GAP - theme::COL_COUNT;
    let files_x = subdirs_x - theme::COL_GAP - theme::COL_COUNT;
    let items_x = files_x - theme::COL_GAP - theme::COL_COUNT;
    let size_x = items_x - theme::COL_GAP - theme::COL_SIZE;
    let pct_x = size_x - theme::COL_GAP - theme::COL_PERCENT_NUM;
    let bar_x = pct_x - theme::COL_GAP - theme::PERCENT_BAR_COLUMN_WIDTH;
    Columns {
        name_w: (bar_x - theme::COL_GAP - left).max(80.0),
        bar_x,
        pct_x,
        size_x,
        items_x,
        files_x,
        subdirs_x,
        mtime_x,
        right,
    }
}

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    // egui's plain label colour is a mid grey meant for non-interactive
    // text; rows are the content here, so they get the strong colour.
    let text_color = ui.visuals().strong_text_color();
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
    let headers: [(Column, f32, f32, Align2, &str); 8] = [
        (
            Column::Name,
            header_rect.left() + 4.0,
            cols.name_w,
            Align2::LEFT_CENTER,
            "Name",
        ),
        (
            Column::PercentBar,
            cols.bar_x,
            theme::PERCENT_BAR_COLUMN_WIDTH,
            Align2::LEFT_CENTER,
            "Subtree Percentage",
        ),
        (
            Column::PercentNum,
            cols.pct_x,
            theme::COL_PERCENT_NUM,
            Align2::RIGHT_CENTER,
            "%",
        ),
        (
            Column::Size,
            cols.size_x,
            theme::COL_SIZE,
            Align2::RIGHT_CENTER,
            "Size",
        ),
        (
            Column::Items,
            cols.items_x,
            theme::COL_COUNT,
            Align2::RIGHT_CENTER,
            "Items",
        ),
        (
            Column::Files,
            cols.files_x,
            theme::COL_COUNT,
            Align2::RIGHT_CENTER,
            "Files",
        ),
        (
            Column::Subdirs,
            cols.subdirs_x,
            theme::COL_COUNT,
            Align2::RIGHT_CENTER,
            "Subdirs",
        ),
        (
            Column::Mtime,
            cols.mtime_x,
            theme::COL_MTIME,
            Align2::RIGHT_CENTER,
            "Last Modified",
        ),
    ];
    for (col, x, w, align, label) in headers {
        let rect = Rect::from_min_size(
            Pos2::new(x, header_rect.top()),
            Vec2::new(w, theme::ROW_HEIGHT),
        );
        let resp = ui.interact(rect, ui.scope_id().with(("hdr", label)), Sense::click());
        let text = label.to_string();
        let sorted_here = app.sort.column == col;
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
            triangle(
                &painter,
                Pos2::new(cx, y),
                4.0,
                if app.sort.ascending {
                    Dir::Up
                } else {
                    Dir::Down
                },
                weak,
            );
        }
        painter.text(
            anchor,
            align,
            text,
            font.clone(),
            if resp.hovered() { text_color } else { weak },
        );
        if resp.clicked() {
            app.set_sort(col);
        }
    }

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

    let mut clicked: Option<(u32, bool, bool)> = None; // node, toggle, double
    let mut area = egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .id_salt("tree-rows");
    if let Some(i) = scroll_to {
        let spacing = ui.spacing().item_spacing.y;
        let offset = i as f32 * (theme::ROW_HEIGHT + spacing) - ui.available_height() / 2.0;
        area = area.vertical_scroll_offset(offset.max(0.0));
    }
    area.show_rows(ui, theme::ROW_HEIGHT, total, |ui, range| {
        let Some(tree) = app.tree.clone() else { return };
        let width = ui.available_width();
        for i in range {
            let Some(row) = app.rows.get(i).copied() else {
                continue;
            };
            // The id is the node, not the row position, so that after rows
            // shift (expand, collapse, scroll) egui's hover and click state
            // still refers to the same item instead of whatever row now sits
            // at that index.
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, theme::ROW_HEIGHT), Sense::hover());
            let resp = ui.interact(
                rect,
                egui::Id::unique(("oxtree-row", row.node)),
                Sense::click(),
            );
            let cols = columns(rect.left() + 4.0, rect.width() - 8.0);
            let painter = ui.painter_at(rect);
            let is_selected = selected == Some(row.node);
            if is_selected {
                painter.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
            } else if resp.hovered() {
                painter.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let kind = tree.kind(row.node);
            let y = rect.center().y;
            let indent = rect.left() + 4.0 + row.depth as f32 * theme::INDENT;

            // Expand marker and name
            let expandable = tree.child_count(row.node) > 0;
            let marker_rect =
                Rect::from_center_size(Pos2::new(indent + 6.0, y), Vec2::splat(theme::ROW_HEIGHT));
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
                Pos2::new(cols.bar_x - theme::COL_GAP, rect.bottom()),
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
            let bar_rect = Rect::from_min_max(
                Pos2::new(cols.bar_x, rect.top() + 3.0),
                Pos2::new(
                    cols.bar_x + theme::PERCENT_BAR_COLUMN_WIDTH,
                    rect.bottom() - 3.0,
                ),
            );
            match kind {
                Kind::UnreadDir => {
                    painter.text(
                        Pos2::new(cols.bar_x + 4.0, y),
                        Align2::LEFT_CENTER,
                        "[Not Read]",
                        font.clone(),
                        theme::DIR_READ_ERROR,
                    );
                }
                Kind::MountPoint => {
                    painter.text(
                        Pos2::new(cols.bar_x + 4.0, y),
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
                            Pos2::new(cols.pct_x + theme::COL_PERCENT_NUM, y),
                            Align2::RIGHT_CENTER,
                            format::percent(p),
                            font.clone(),
                            text_color,
                        );
                    }
                }
            }

            // Numbers
            painter.text(
                Pos2::new(cols.size_x + theme::COL_SIZE, y),
                Align2::RIGHT_CENTER,
                format::size(tree.size(row.node)),
                font.clone(),
                text_color,
            );
            if tree.is_dir_like(row.node) && kind != Kind::DotEntry {
                painter.text(
                    Pos2::new(cols.items_x + theme::COL_COUNT, y),
                    Align2::RIGHT_CENTER,
                    format::grouped(tree.total_items(row.node) as u64),
                    font.clone(),
                    text_color,
                );
                painter.text(
                    Pos2::new(cols.files_x + theme::COL_COUNT, y),
                    Align2::RIGHT_CENTER,
                    format::grouped(tree.total_files(row.node) as u64),
                    font.clone(),
                    text_color,
                );
                painter.text(
                    Pos2::new(cols.subdirs_x + theme::COL_COUNT, y),
                    Align2::RIGHT_CENTER,
                    format::grouped(tree.total_subdirs(row.node) as u64),
                    font.clone(),
                    text_color,
                );
            } else if kind == Kind::DotEntry {
                painter.text(
                    Pos2::new(cols.items_x + theme::COL_COUNT, y),
                    Align2::RIGHT_CENTER,
                    format::grouped(tree.total_items(row.node) as u64),
                    font.clone(),
                    text_color,
                );
            }
            painter.text(
                Pos2::new(cols.right, y),
                Align2::RIGHT_CENTER,
                time::format(tree.mtime(row.node), app.utc_offset),
                font.clone(),
                text_color,
            );

            if resp.secondary_clicked() {
                app.select(row.node, false);
            }
            resp.context_menu(|ui| crate::menus::context_menu(app, ui));
            if resp.double_clicked() {
                clicked = Some((row.node, false, true));
            } else if resp.clicked() {
                let on_marker = resp
                    .interact_pointer_pos()
                    .map(|p| marker_rect.contains(p))
                    .unwrap_or(false);
                clicked = Some((row.node, on_marker && expandable, false));
            }
        }
    });

    if let Some((node, toggle, double)) = clicked {
        if toggle {
            app.toggle_expand(node);
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
    let (up, down, left, right, home, end, enter) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowLeft),
            i.key_pressed(egui::Key::ArrowRight),
            i.key_pressed(egui::Key::Home),
            i.key_pressed(egui::Key::End),
            i.key_pressed(egui::Key::Enter),
        )
    });
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
    if let Some(t) = target {
        app.select(t, false);
        app.scroll_to_selected = true;
    }
}
