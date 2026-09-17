//! The desk: your screens, laid out the way they really sit.
//!
//! Each connected monitor is a flat rectangle with a hairline border and a
//! terminal-style label block in the corner. Drag one and it snaps to its
//! neighbours' edges, with guide lines showing what it locked onto.

use egui::{
    Align2, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
    vec2,
};

use crate::displays::Display;
use crate::theme;

/// Maps the desktop's coordinate space onto the canvas.
///
/// `scale` is canvas points per logical pixel, `origin` is where logical
/// `(0, 0)` lands on screen.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub scale: f32,
    pub origin: Pos2,
}

impl View {
    pub fn screen_pos(&self, x: i32, y: i32) -> Pos2 {
        self.origin + vec2(x as f32, y as f32) * self.scale
    }
}

/// One in-flight drag. We remember where the tile started and the total mouse
/// travel, so snapping never accumulates drift.
struct Drag {
    index: usize,
    start: (i32, i32),
    total: Vec2,
    view: View,
}

#[derive(Default)]
pub struct CanvasState {
    drag: Option<Drag>,
}

/// Draw the desk. Returns `true` if the user moved something.
pub fn show(
    ui: &mut Ui,
    state: &mut CanvasState,
    displays: &mut [Display],
    selected: &mut usize,
) -> bool {
    let mut changed = false;

    let full = ui.available_rect_before_wrap();
    let (rect, _) = ui.allocate_exact_size(full.size(), Sense::hover());
    let painter = ui.painter_at(rect);

    // The field the screens sit in, leaving a line at the top for the status.
    let field = Rect::from_min_max(rect.min + vec2(0.0, 22.0), rect.max);

    // The well the screens sit in, one shade below the panel.
    painter.rect_filled(rect, theme::radius(), theme::crust());
    painter.rect_stroke(
        rect,
        theme::radius(),
        Stroke::new(1.0_f32, theme::surface1()),
        StrokeKind::Inside,
    );

    // Safety net: if the mouse button came up somewhere we never got told
    // about, don't leave the view frozen mid-drag forever.
    if state.drag.is_some() && !ui.input(|i| i.pointer.any_down()) {
        state.drag = None;
    }

    // Freeze the view while dragging so the tile tracks the pointer exactly;
    // otherwise the bounding box would grow as you drag and everything would
    // slide around underneath you.
    let bbox = layout_bounds(displays);
    let view = match state.drag.as_ref() {
        Some(d) => d.view,
        None => fit(bbox, field),
    };

    let sizes: Vec<(i32, i32)> = displays.iter().map(|d| d.logical_size()).collect();
    let places: Vec<(i32, i32, i32, i32)> = displays
        .iter()
        .zip(&sizes)
        .map(|(d, &(w, h))| (d.draft.x, d.draft.y, w, h))
        .collect();

    // ── interaction ─────────────────────────────────────────────────────────
    for i in 0..displays.len() {
        let (w, h) = sizes[i];
        let hit = clamp_rect(screen_rect(&view, places[i]).intersect(rect));
        let response = ui.interact(hit, ui.id().with(("tile", i)), Sense::click_and_drag());

        if response.drag_started() {
            state.drag = Some(Drag {
                index: i,
                start: (displays[i].draft.x, displays[i].draft.y),
                total: Vec2::ZERO,
                view,
            });
            *selected = i;
        }
        if response.clicked() {
            *selected = i;
        }

        if response.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            if let Some(drag) = state.drag.as_mut() {
                if drag.index == i {
                    drag.total += response.drag_delta();
                    let s = drag.view.scale.max(1e-4);
                    let raw = (
                        drag.start.0 + (drag.total.x / s).round() as i32,
                        drag.start.1 + (drag.total.y / s).round() as i32,
                    );
                    // ~12 screen points of pull before an edge grabs you.
                    let tol = (12.0 / s).round().max(1.0) as i32;
                    let snapped = snap(raw, (w, h), &places, i, tol);

                    let d = &mut displays[i];
                    if (d.draft.x, d.draft.y) != snapped {
                        changed = true;
                    }
                    d.draft.x = snapped.0;
                    d.draft.y = snapped.1;
                }
            }
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }

        if response.drag_stopped() && matches!(state.drag, Some(ref d) if d.index == i) {
            state.drag = None;
        }
    }

    // ── painting (positions are now final for this frame) ───────────────────
    let tiles: Vec<Rect> = displays
        .iter()
        .map(|d| {
            let (w, h) = d.logical_size();
            screen_rect(&view, (d.draft.x, d.draft.y, w, h))
        })
        .collect();

    // Tiles that sit on top of each other can't happen on a real desk, so flag
    // them rather than silently letting the user make a mess.
    let mut overlapping = vec![false; displays.len()];
    for a in 0..displays.len() {
        for b in (a + 1)..displays.len() {
            if !displays[a].draft.enabled || !displays[b].draft.enabled {
                continue;
            }
            let inter = tiles[a].intersect(tiles[b]);
            if inter.width() > 1.0 && inter.height() > 1.0 {
                overlapping[a] = true;
                overlapping[b] = true;
            }
        }
    }

    // Guide lines showing which edges the dragged tile has locked onto.
    if let Some(drag) = state.drag.as_ref() {
        let i = drag.index;
        let (w, h) = sizes[i];
        let (verticals, horizontals) = snap_guides(
            (displays[i].draft.x, displays[i].draft.y),
            (w, h),
            &places,
            i,
        );
        let guide = theme::dim(theme::accent(), 120);
        for gx in verticals {
            let sx = view.screen_pos(gx, 0).x;
            painter.line_segment(
                [pos2(sx, field.top()), pos2(sx, field.bottom())],
                Stroke::new(1.0_f32, guide),
            );
        }
        for gy in horizontals {
            let sy = view.screen_pos(0, gy).y;
            painter.line_segment(
                [pos2(field.left(), sy), pos2(field.right(), sy)],
                Stroke::new(1.0_f32, guide),
            );
        }
    }

    for (i, d) in displays.iter().enumerate() {
        paint_tile(&painter, tiles[i], d, i == *selected, overlapping[i]);
    }

    // Status line along the top, like a terminal title bar.
    let content = content_bounds(displays);
    painter.text(
        rect.left_top() + vec2(8.0, 5.0),
        Align2::LEFT_TOP,
        format!(
            "desk {}x{}",
            content.width().round() as i32,
            content.height().round() as i32
        ),
        FontId::monospace(11.0),
        theme::overlay0(),
    );
    if displays.iter().any(|d| d.dirty()) {
        painter.text(
            rect.right_top() + vec2(-8.0, 5.0),
            Align2::RIGHT_TOP,
            "modified",
            FontId::monospace(11.0),
            theme::warn(),
        );
    }

    changed
}

// ────────────────────────────────────────────────────────────────────────────
// Geometry
// ────────────────────────────────────────────────────────────────────────────

fn screen_rect(view: &View, p: (i32, i32, i32, i32)) -> Rect {
    Rect::from_min_size(
        view.screen_pos(p.0, p.1),
        vec2(p.2 as f32, p.3 as f32) * view.scale,
    )
}

/// egui gets unhappy with degenerate rects, and a tile dragged off the canvas
/// would otherwise produce one.
fn clamp_rect(r: Rect) -> Rect {
    Rect::from_min_size(r.min, vec2(r.width().max(1.0), r.height().max(1.0)))
}

fn pad_of(r: Rect) -> Vec2 {
    vec2(r.width() * 0.10 + 120.0, r.height() * 0.10 + 120.0)
}

/// The tight bounding box of every tile, in logical pixels.
fn content_bounds(displays: &[Display]) -> Rect {
    let mut acc: Option<Rect> = None;
    for d in displays {
        let (w, h) = d.logical_size();
        let t = Rect::from_min_size(
            Pos2::new(d.draft.x as f32, d.draft.y as f32),
            vec2(w as f32, h as f32),
        );
        acc = Some(match acc {
            Some(a) => a.union(t),
            None => t,
        });
    }
    acc.unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0)))
}

/// The bounding box with breathing room around it, so there's somewhere to
/// shove a screen while rearranging.
fn layout_bounds(displays: &[Display]) -> Rect {
    let r = content_bounds(displays);
    r.expand2(pad_of(r) * 0.5)
}

/// Fit a logical bounding box into the canvas.
fn fit(bbox: Rect, canvas: Rect) -> View {
    let avail = canvas.shrink(30.0);
    let sx = avail.width() / bbox.width().max(1.0);
    let sy = avail.height() / bbox.height().max(1.0);
    let scale = sx.min(sy).clamp(0.002, 0.5);
    let size = bbox.size() * scale;
    let origin = canvas.center() - size * 0.5 - bbox.min.to_vec2() * scale;
    View { scale, origin }
}

/// Pull a dragged tile towards the edges and centres of the others.
fn snap(
    raw: (i32, i32),
    size: (i32, i32),
    places: &[(i32, i32, i32, i32)],
    skip: usize,
    tol: i32,
) -> (i32, i32) {
    let mut best_x = raw.0;
    let mut dx = tol + 1;
    let mut best_y = raw.1;
    let mut dy = tol + 1;

    for (i, &(ox, oy, ow, oh)) in places.iter().enumerate() {
        if i == skip {
            continue;
        }
        for cand in [
            ox,                     // left edges flush
            ox + ow,                // butted up on the right
            ox - size.0,            // butted up on the left
            ox + ow - size.0,       // right edges flush
            ox + (ow - size.0) / 2, // centred
        ] {
            let d = (cand - raw.0).abs();
            if d < dx {
                dx = d;
                best_x = cand;
            }
        }
        for cand in [
            oy,                     // tops flush
            oy + oh,                // stacked below
            oy - size.1,            // stacked above
            oy + oh - size.1,       // bottoms flush
            oy + (oh - size.1) / 2, // centred
        ] {
            let d = (cand - raw.1).abs();
            if d < dy {
                dy = d;
                best_y = cand;
            }
        }
    }

    // Fall back to a tidy 2px grid when nothing is nearby.
    if dx > tol {
        best_x = (raw.0 as f32 / 2.0).round() as i32 * 2;
    }
    if dy > tol {
        best_y = (raw.1 as f32 / 2.0).round() as i32 * 2;
    }
    (best_x, best_y)
}

/// The edges a tile has locked onto, in logical coordinates: the vertical
/// guides (x positions) and the horizontal ones (y positions).
///
/// Every edge of the dragged tile is compared against every edge of the
/// others, because the interesting case is one screen's left edge meeting its
/// neighbour's right edge.
fn snap_guides(
    pos: (i32, i32),
    size: (i32, i32),
    places: &[(i32, i32, i32, i32)],
    skip: usize,
) -> (Vec<i32>, Vec<i32>) {
    let (x, y, w, h) = (pos.0, pos.1, size.0, size.1);
    let mut verticals = Vec::new();
    let mut horizontals = Vec::new();

    for (i, &(ox, oy, ow, oh)) in places.iter().enumerate() {
        if i == skip {
            continue;
        }
        for mine in [x, x + w] {
            for theirs in [ox, ox + ow] {
                if mine == theirs && !verticals.contains(&mine) {
                    verticals.push(mine);
                }
            }
        }
        for mine in [y, y + h] {
            for theirs in [oy, oy + oh] {
                if mine == theirs && !horizontals.contains(&mine) {
                    horizontals.push(mine);
                }
            }
        }
    }
    (verticals, horizontals)
}

// ────────────────────────────────────────────────────────────────────────────
// Painting
// ────────────────────────────────────────────────────────────────────────────

/// A screen is a filled rectangle, a hairline border, and a label block.
fn paint_tile(painter: &egui::Painter, rect: Rect, d: &Display, selected: bool, overlapping: bool) {
    if rect.width() < 3.0 || rect.height() < 3.0 {
        return;
    }
    let on = d.draft.enabled;

    painter.rect_filled(
        rect,
        theme::radius(),
        if on { theme::surface0() } else { theme::mantle() },
    );

    if rect.width() > 34.0 && rect.height() > 13.0 {
        let text = painter.with_clip_rect(rect.shrink(2.0));
        let size = (rect.height() * 0.13).clamp(9.0, 12.0);
        let line = size * 1.5;
        let pad = 6.0;
        let rows = (((rect.height() - pad * 2.0) / line).floor() as usize).min(3);
        let x = rect.left() + pad;
        let mut y = rect.top() + pad;

        if !on {
            text.text(
                pos2(x, y),
                Align2::LEFT_TOP,
                format!("{} [off]", d.pretty_name()),
                FontId::monospace(size),
                theme::overlay0(),
            );
        } else {
            if rows >= 1 {
                text.text(
                    pos2(x, y),
                    Align2::LEFT_TOP,
                    d.pretty_name(),
                    FontId::new(size, theme::bold()),
                    theme::text(),
                );
                y += line;
            }
            if rows >= 2 {
                text.text(
                    pos2(x, y),
                    Align2::LEFT_TOP,
                    d.draft.mode.pixel_label(),
                    FontId::monospace(size),
                    theme::subtext0(),
                );
                y += line;
            }
            if rows >= 3 {
                text.text(
                    pos2(x, y),
                    Align2::LEFT_TOP,
                    format!(
                        "{}  {}x",
                        d.draft.mode.hz_label(),
                        trim_scale(d.draft.scale)
                    ),
                    FontId::monospace(size),
                    theme::overlay0(),
                );
            }
        }
    }

    let stroke = if overlapping {
        Stroke::new(1.0_f32, theme::bad())
    } else if selected {
        Stroke::new(1.0_f32, theme::accent())
    } else if on {
        Stroke::new(1.0_f32, theme::surface2())
    } else {
        Stroke::new(1.0_f32, theme::surface1())
    };
    painter.rect_stroke(rect, theme::radius(), stroke, StrokeKind::Inside);
}

fn trim_scale(s: f32) -> String {
    let t = format!("{s:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_to_a_neighbours_edges() {
        // One 1920x1080 screen parked at the origin.
        let places = [(0, 0, 1920, 1080)];
        // Just shy of butting up on the right, a few px low: both should grab.
        assert_eq!(snap((1915, 4), (1920, 1080), &places, 99, 12), (1920, 0));
        // Stacked directly underneath, slightly off-centre.
        assert_eq!(snap((4, 1085), (1920, 1080), &places, 99, 12), (0, 1080));
        // Tops flush, hanging off to the left.
        assert_eq!(snap((-1914, 3), (1920, 1080), &places, 99, 12), (-1920, 0));
    }

    #[test]
    fn centres_under_a_wider_screen() {
        let places = [(0, 0, 3840, 2160)];
        // A 1920-wide screen 15px off centre: too far, so it stays put.
        assert_eq!(snap((975, 2200), (1920, 1080), &places, 99, 12), (976, 2200));
        // 6px off centre: snaps to dead centre.
        assert_eq!(snap((966, 2200), (1920, 1080), &places, 99, 12), (960, 2200));
    }

    #[test]
    fn falls_back_to_a_two_pixel_grid() {
        // Nothing nearby, so round to even numbers.
        assert_eq!(snap((5001, 3003), (800, 600), &[], 0, 12), (5002, 3004));
    }

    #[test]
    fn a_tile_never_snaps_to_itself() {
        let places = [(5001, 3003, 800, 600)];
        // skip = 0, and it's the only entry, so the grid fallback applies
        // instead of the tile locking onto its own edges.
        assert_eq!(snap((5001, 3003), (800, 600), &places, 0, 12), (5002, 3004));
    }

    #[test]
    fn guides_appear_only_on_edges_that_line_up() {
        let places = [(0, 0, 1920, 1080), (1920, 0, 1920, 1080)];
        // Second screen flush to the right of the first: one vertical guide on
        // the shared edge (its left edge meeting the other's right edge), and
        // two horizontal ones where the tops and bottoms are flush.
        let (v, h) = snap_guides((1920, 0), (1920, 1080), &places, 1);
        assert_eq!(v, vec![1920]);
        assert_eq!(h, vec![0, 1080]);

        // A screen sitting below its neighbour shares a horizontal edge.
        let (v, h) = snap_guides((0, 1080), (1920, 1080), &places, 99);
        assert_eq!(v, vec![0, 1920]);
        assert_eq!(h, vec![1080]);

        // Nothing lines up, so nothing is drawn.
        let (v, h) = snap_guides((4000, 4000), (1920, 1080), &places, 99);
        assert!(v.is_empty() && h.is_empty());
    }
}
