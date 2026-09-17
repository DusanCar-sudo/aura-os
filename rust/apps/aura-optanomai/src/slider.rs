//! One horizontal track, three coloured arrows on it.
//!
//! Resolution, refresh and scale all ride the same line — blue, green and
//! mauve — so the whole control surface for a screen is one row of pixels
//! instead of three knobs. Drag an arrow to change its setting.

use egui::{
    Align2, Color32, CursorIcon, FontId, Id, Rect, Sense, Shape, Stroke, Ui, pos2, vec2,
};

use crate::theme;

/// Height of the whole widget: label row, arrows, line.
pub const HEIGHT: f32 = 32.0;

/// How far in from the ends the track starts and stops, so the end arrows
/// don't hang off the edge.
const PAD: f32 = 12.0;

/// One arrow on the track.
pub struct Mark<'a> {
    pub id: Id,
    /// What this arrow sets, for the tooltip.
    pub title: &'a str,
    /// The value, drawn above the arrow.
    pub label: String,
    pub color: Color32,
    pub index: usize,
    pub count: usize,
    pub enabled: bool,
}

/// Draw the track and let the user drag the arrows.
///
/// The indices in `marks` are updated in place, so the caller can just read
/// them back afterwards. Returns `true` if anything moved.
pub fn track(ui: &mut Ui, marks: &mut [Mark<'_>]) -> bool {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::hover());
    let line_y = rect.bottom() - 1.0;
    let span = (rect.width() - PAD * 2.0).max(1.0);

    let t_of = |m: &Mark<'_>| -> f32 {
        if m.count <= 1 {
            0.0
        } else {
            m.index.min(m.count - 1) as f32 / (m.count - 1) as f32
        }
    };
    let x_of = |t: f32| rect.left() + PAD + t * span;

    // ── interaction ─────────────────────────────────────────────────────────
    let mut moved = false;
    let mut dragging: Option<usize> = None;

    for (i, m) in marks.iter_mut().enumerate() {
        if !m.enabled || m.count <= 1 {
            continue;
        }
        let hit = Rect::from_center_size(pos2(x_of(t_of(m)), line_y - 11.0), vec2(20.0, 26.0));
        let response = ui
            .interact(hit, m.id, Sense::click_and_drag())
            .on_hover_text(format!("{}: {} — drag to change", m.title, m.label));

        if response.dragged() {
            dragging = Some(i);
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            if let Some(p) = ui.ctx().pointer_interact_pos() {
                // Absolute, like a slider: the arrow jumps under the pointer.
                let t = ((p.x - rect.left() - PAD) / span).clamp(0.0, 1.0);
                let index = (t * (m.count - 1) as f32).round() as usize;
                if index != m.index {
                    m.index = index;
                    moved = true;
                }
            }
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
    }

    // Positions are read after the interaction above, so a dragged arrow
    // tracks the pointer on the very same frame.
    let xs: Vec<f32> = marks.iter().map(|m| x_of(t_of(m))).collect();

    // ── painting ────────────────────────────────────────────────────────────
    let painter = ui.painter();

    // Faint ticks for every position of whichever arrow is being dragged, so
    // you can see what it is snapping to.
    if let Some(i) = dragging {
        let m = &marks[i];
        if (2..=40).contains(&m.count) {
            for k in 0..m.count {
                let t = k as f32 / (m.count - 1) as f32;
                let x = x_of(t);
                painter.line_segment(
                    [pos2(x, line_y - 3.0), pos2(x, line_y + 3.0)],
                    Stroke::new(1.0_f32, theme::dim(m.color, 110)),
                );
            }
        }
    }

    // The line itself.
    painter.line_segment(
        [pos2(rect.left() + PAD, line_y), pos2(rect.right() - PAD, line_y)],
        Stroke::new(1.0_f32, theme::surface2()),
    );

    // Arrows: a filled triangle with its tip on the line.
    for (i, m) in marks.iter().enumerate() {
        let x = xs[i];
        let faded = !m.enabled;
        let col = if faded { theme::overlay0() } else { m.color };
        painter.add(Shape::convex_polygon(
            vec![
                pos2(x, line_y - 1.0),
                pos2(x - 4.5, line_y - 8.5),
                pos2(x + 4.5, line_y - 8.5),
            ],
            col,
            Stroke::NONE,
        ));
    }

    // Values above the arrows, nudged apart so they never overprint.
    let half_widths: Vec<f32> = marks
        .iter()
        .map(|m| {
            let w = ui.fonts(|f| {
                f.layout_no_wrap(m.label.clone(), FontId::monospace(10.5), Color32::BLACK)
                    .size()
                    .x
            });
            w * 0.5 + 3.0
        })
        .collect();

    let mut order: Vec<usize> = (0..marks.len()).collect();
    order.sort_by(|&a, &b| xs[a].partial_cmp(&xs[b]).unwrap_or(std::cmp::Ordering::Equal));

    let mut placed = xs.clone();
    let mut edge = f32::NEG_INFINITY;
    for &i in &order {
        let want = placed[i].max(edge + half_widths[i]);
        placed[i] = want;
        edge = want + half_widths[i];
    }
    // If that pushed things off the right edge, slide the whole row back.
    if let Some(&last) = order.last() {
        let overflow = placed[last] + half_widths[last] - (rect.right() - 1.0);
        if overflow > 0.0 {
            for p in placed.iter_mut() {
                *p -= overflow;
            }
        }
    }

    for (i, m) in marks.iter().enumerate() {
        let col = if m.enabled { m.color } else { theme::overlay0() };
        painter.text(
            pos2(placed[i], line_y - 12.0),
            Align2::CENTER_BOTTOM,
            &m.label,
            FontId::monospace(10.5),
            col,
        );
    }

    moved
}
