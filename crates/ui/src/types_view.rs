//! The File Types view: one row per extension with its category colour,
//! file count, total size and share. Double click lists the type's files
//! in the File View.

use eframe::egui::{self, Align2, Pos2, Rect, Sense, Ui, Vec2};
use oxtree_core::format;
use oxtree_core::types::TypeRow;

use crate::app::{OxTreeApp, TypeSort};
use crate::columns;
use crate::theme;

const EXT: usize = 0;
const CATEGORY: usize = 1;
const COUNT: usize = 2;
const SIZE: usize = 3;
const PCT: usize = 4;

pub fn show(app: &mut OxTreeApp, ui: &mut Ui) {
    let row_h = app.row_height;
    app.ensure_types();
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().strong_text_color();
    let weak = ui.visuals().weak_text_color();
    let (header_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::hover());
    let rows = sorted(app);
    let total_size = app
        .tree
        .as_ref()
        .map(|t| t.size(t.root()))
        .unwrap_or(0)
        .max(1);
    let mut open: Option<String> = None;
    let out = egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt("type-rows")
        .show_rows(ui, row_h, rows.len(), |ui, range| {
            let width = columns::row_width(ui, &app.types_columns);
            for i in range {
                let Some(row) = rows.get(i) else { continue };
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::hover());
                let resp = ui.interact(
                    rect,
                    ui.scope_id().with(("type-row", row.ext.as_str())),
                    Sense::click(),
                );
                let painter = ui.painter_at(rect);
                columns::stripe(ui, &painter, rect, i);
                if resp.hovered() {
                    painter.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
                }
                let cols = &app.types_columns;
                let left = rect.left();
                let y = rect.center().y;
                let swatch = Rect::from_center_size(
                    Pos2::new(left + columns::PAD + 6.0, y),
                    Vec2::splat(10.0),
                );
                painter.rect_filled(
                    swatch,
                    1.0,
                    row.color
                        .map(theme::rgb)
                        .unwrap_or(theme::rgb(theme::FILE_FILL)),
                );
                let ext_clip = Rect::from_min_max(
                    Pos2::new(swatch.right() + 6.0, rect.top()),
                    Pos2::new(cols.text_right(left, EXT), rect.bottom()),
                );
                painter.with_clip_rect(ext_clip).text(
                    Pos2::new(swatch.right() + 6.0, y),
                    Align2::LEFT_CENTER,
                    &row.ext,
                    font.clone(),
                    text_color,
                );
                let cat_x = cols.x(left, CATEGORY) + columns::PAD;
                let cat_clip = Rect::from_min_max(
                    Pos2::new(cat_x, rect.top()),
                    Pos2::new(cols.text_right(left, CATEGORY), rect.bottom()),
                );
                painter.with_clip_rect(cat_clip).text(
                    Pos2::new(cat_x, y),
                    Align2::LEFT_CENTER,
                    row.category.as_deref().unwrap_or(""),
                    font.clone(),
                    weak,
                );
                let number = |col: usize, text: String| {
                    painter.text(
                        Pos2::new(cols.text_right(left, col), y),
                        Align2::RIGHT_CENTER,
                        text,
                        font.clone(),
                        text_color,
                    );
                };
                number(COUNT, format::grouped(row.count as u64));
                number(SIZE, format::size(row.size));
                number(
                    PCT,
                    format::percent(row.size as f64 * 100.0 / total_size as f64),
                );
                if resp.double_clicked() {
                    open = Some(row.query());
                }
            }
        });

    let left = out.inner_rect.left() - out.state.clamped_offset().x;
    let sort = |s: TypeSort| (app.types_sort.0 == s).then_some(app.types_sort.1);
    let cells = [
        columns::HeaderCell {
            label: "Extension",
            right: false,
            sort: sort(TypeSort::Ext),
        },
        columns::HeaderCell {
            label: "Category",
            right: false,
            sort: sort(TypeSort::Category),
        },
        columns::HeaderCell {
            label: "Files",
            right: true,
            sort: sort(TypeSort::Count),
        },
        columns::HeaderCell {
            label: "Size",
            right: true,
            sort: sort(TypeSort::Size),
        },
        columns::HeaderCell {
            label: "%",
            right: true,
            sort: None,
        },
    ];
    match columns::header(
        ui,
        &mut app.types_columns,
        header_rect,
        left,
        &cells,
        "types",
    ) {
        Some(0) => app.set_types_sort(TypeSort::Ext),
        Some(1) => app.set_types_sort(TypeSort::Category),
        Some(2) => app.set_types_sort(TypeSort::Count),
        Some(3) => app.set_types_sort(TypeSort::Size),
        _ => {}
    }
    if let Some(q) = open {
        app.show_type_in_file_view(q);
    }
}

/// The table in the chosen order. The stored table is largest first; the
/// other orders are sorted here, a few hundred rows at most.
fn sorted(app: &OxTreeApp) -> Vec<TypeRow> {
    let Some(types) = &app.types else {
        return Vec::new();
    };
    let (sort, ascending) = app.types_sort;
    let mut rows: Vec<TypeRow> = types.iter().cloned().collect();
    match sort {
        TypeSort::Ext => rows.sort_by(|a, b| a.ext.cmp(&b.ext)),
        TypeSort::Category => {
            rows.sort_by(|a, b| a.category.cmp(&b.category).then(b.size.cmp(&a.size)))
        }
        TypeSort::Count => rows.sort_by_key(|r| r.count),
        TypeSort::Size => rows.sort_by_key(|r| r.size),
    }
    if !ascending {
        rows.reverse();
    }
    rows
}
