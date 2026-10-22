//! Column widths for the three tables, draggable at the header dividers,
//! and the header band they share: labels, sort marker, dividers and the
//! drag handles. Each column's width includes its padding; text sits
//! `PAD` points in from either edge.

use eframe::egui::{self, Align2, Color32, CursorIcon, Pos2, Rect, Sense, Stroke, Ui, Vec2};

pub const PAD: f32 = 4.0;
const MIN_WIDTH: f32 = 40.0;

pub struct Columns {
    pub widths: Vec<f32>,
    defaults: &'static [f32],
}

impl Columns {
    pub fn new(defaults: &'static [f32]) -> Columns {
        Columns {
            widths: defaults.to_vec(),
            defaults,
        }
    }

    pub fn reset(&mut self) {
        self.widths = self.defaults.to_vec();
    }

    pub fn width(&self, i: usize) -> f32 {
        self.widths.get(i).copied().unwrap_or(0.0)
    }

    /// Left edge of column `i`, given the left edge of the row.
    pub fn x(&self, left: f32, i: usize) -> f32 {
        left + self.widths.iter().take(i).sum::<f32>()
    }

    /// Right edge of column `i` minus the padding: where right-aligned text ends.
    pub fn text_right(&self, left: f32, i: usize) -> f32 {
        self.x(left, i + 1) - PAD
    }

    pub fn total(&self) -> f32 {
        self.widths.iter().sum()
    }
}

/// One header cell: its label, whether its text is right-aligned, and
/// `Some(ascending)` when the table is sorted by it.
pub struct HeaderCell {
    pub label: &'static str,
    pub right: bool,
    pub sort: Option<bool>,
}

/// Paint the header band over `band` for columns starting at `left` (which
/// is off to the left by the sideways scroll), with drag handles on the
/// dividers. Returns the index of a clicked label.
pub fn header(
    ui: &mut Ui,
    cols: &mut Columns,
    band: Rect,
    left: f32,
    cells: &[HeaderCell],
    salt: &str,
) -> Option<usize> {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let strong = ui.visuals().strong_text_color();
    let weak = ui.visuals().weak_text_color();
    let painter = ui.painter_at(band);
    painter.rect_filled(band, 0.0, ui.visuals().faint_bg_color);
    let y = band.center().y;
    let mut clicked = None;
    for (i, cell) in cells.iter().enumerate() {
        let x = cols.x(left, i);
        let w = cols.width(i);
        let rect = Rect::from_min_size(Pos2::new(x, band.top()), Vec2::new(w, band.height()));
        let resp = ui.interact(rect, ui.scope_id().with((salt, "hdr", i)), Sense::click());
        let marker_room = if cell.sort.is_some() { 12.0 } else { 0.0 };
        let (anchor, align) = if cell.right {
            (
                Pos2::new(x + w - PAD - marker_room, y),
                Align2::RIGHT_CENTER,
            )
        } else {
            (Pos2::new(x + PAD, y), Align2::LEFT_CENTER)
        };
        if let Some(ascending) = cell.sort {
            crate::tree_view::sort_marker(
                &painter,
                Pos2::new(x + w - PAD - 5.0, y),
                ascending,
                weak,
            );
        }
        painter.text(
            anchor,
            align,
            cell.label,
            font.clone(),
            if resp.hovered() { strong } else { weak },
        );
        if resp.clicked() {
            clicked = Some(i);
        }
        // Divider and its drag handle.
        let dx = x + w;
        painter.vline(
            dx - 0.5,
            band.top() + 4.0..=band.bottom() - 4.0,
            Stroke::new(1.0, Color32::from_gray(0x60)),
        );
        let handle = Rect::from_center_size(Pos2::new(dx, y), Vec2::new(8.0, band.height()));
        let drag = ui.interact(handle, ui.scope_id().with((salt, "div", i)), Sense::drag());
        if drag.hovered() || drag.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if drag.dragged() {
            if let Some(cw) = cols.widths.get_mut(i) {
                *cw = (*cw + drag.drag_delta().x).max(MIN_WIDTH);
            }
        }
    }
    clicked
}

/// Alternate row background, drawn before anything else in the row.
pub fn stripe(ui: &Ui, painter: &egui::Painter, rect: Rect, index: usize) {
    if index % 2 == 1 {
        painter.rect_filled(rect, 0.0, ui.visuals().faint_bg_color);
    }
}

/// The row's background for the selection state: the selection colour for
/// a marked row, an outline on the current one, the hover tint otherwise.
pub fn row_background(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    marked: bool,
    current: bool,
    hovered: bool,
) {
    if marked {
        painter.rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
    } else if hovered {
        painter.rect_filled(rect, 2.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    if current && !marked {
        painter.rect_stroke(
            rect.shrink(0.5),
            2.0,
            Stroke::new(1.0, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
}

/// Row width inside a two-axis scroll area: the viewport or the columns,
/// whichever is wider.
pub fn row_width(ui: &Ui, cols: &Columns) -> f32 {
    ui.clip_rect().width().max(cols.total())
}
