//! The few building blocks every Aura tool is made of. Slim on purpose:
//! one text size, hairlines, a row is one line. Change them here and every
//! menu changes with them.

use egui::{Align, Layout, RichText, Sense, Stroke, Ui, vec2};

use crate::theme;

/// Row height for every list in every tool.
pub const ROW_H: f32 = 26.0;

/// The window header: a bold accent title and a quiet one-line hint.
pub fn header(ui: &mut Ui, title: &str, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .family(theme::bold())
                .size(15.0)
                .color(theme::accent()),
        );
        ui.label(RichText::new(hint).color(theme::overlay0()));
    });
}

/// A hairline across the full width.
pub fn hairline(ui: &mut Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, theme::surface1()),
    );
}

/// A section caption, dim and small.
pub fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(RichText::new(text).size(11.0).color(theme::overlay0()));
}

/// One clickable line: an icon, a label, and a value on the right.
/// Hover lights the whole row; the response is clicked when the row is.
pub fn row(ui: &mut Ui, icon: &str, label: &str, value: &str) -> egui::Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(8.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.label(RichText::new(icon).color(theme::accent()));
    child.add_space(4.0);
    child.label(RichText::new(label).color(theme::text()));
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(RichText::new(value).color(theme::subtext0()));
    });
    resp
}

/// A row whose value is an on/off switch. Clicking anywhere on it toggles.
pub fn toggle_row(ui: &mut Ui, label: &str, detail: &str, on: bool) -> egui::Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    // the switch: a small pill on the right
    let pill = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 26.0, rect.center().y),
        vec2(30.0, 14.0),
    );
    let (fill, knob_x) = if on {
        (theme::accent(), pill.right() - 7.0)
    } else {
        (theme::surface1(), pill.left() + 7.0)
    };
    ui.painter().rect_filled(pill, 7.0, fill);
    ui.painter()
        .circle_filled(egui::pos2(knob_x, pill.center().y), 5.0, theme::base());

    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                rect.min + vec2(8.0, 0.0),
                egui::pos2(pill.left() - 8.0, rect.max.y),
            ))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.label(RichText::new(if on { "●" } else { "○" }).color(if on {
        theme::accent()
    } else {
        theme::overlay0()
    }));
    child.add_space(4.0);
    child.label(RichText::new(label).color(theme::text()));
    if !detail.is_empty() {
        child.label(RichText::new(detail).color(theme::subtext0()));
    }
    resp
}

/// A row with a small × at its right end (forget, remove, close).
/// Returns (row clicked, × clicked) — never both.
pub fn row_with_remove(ui: &mut Ui, icon: &str, label: &str, value: &str, remove_hint: &str) -> (bool, bool) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
    let x_rect = egui::Rect::from_min_max(egui::pos2(rect.right() - 24.0, rect.top()), rect.max);
    let x_resp = ui.interact(x_rect, resp.id.with("remove"), Sense::click()).on_hover_text(remove_hint);
    if resp.hovered() || x_resp.hovered() {
        ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().text(
        x_rect.center(),
        egui::Align2::CENTER_CENTER,
        "×",
        egui::FontId::monospace(14.0),
        if x_resp.hovered() { theme::bad() } else { theme::overlay0() },
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(rect.min + vec2(8.0, 0.0), egui::pos2(x_rect.left() - 4.0, rect.max.y)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.label(RichText::new(icon).color(theme::accent()));
    child.add_space(4.0);
    child.label(RichText::new(label).color(theme::text()));
    child.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(RichText::new(value).color(theme::subtext0()));
    });
    let x = x_resp.clicked();
    (resp.clicked() && !x, x)
}

/// A slim level bar (volume, brightness): drag or click anywhere on it.
/// `value` is 0.0..=1.0; returns true while the user changed it this frame.
pub fn level(ui: &mut Ui, value: &mut f32, muted: bool) -> bool {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 18.0), Sense::click_and_drag());
    let track = egui::Rect::from_center_size(rect.center(), vec2(rect.width() - 12.0, 4.0));
    let fill_to = track.left() + track.width() * value.clamp(0.0, 1.0);
    let on = if muted { theme::overlay0() } else { theme::accent() };
    ui.painter().rect_filled(track, 2.0, theme::surface1());
    ui.painter().rect_filled(
        egui::Rect::from_min_max(track.min, egui::pos2(fill_to, track.max.y)),
        2.0,
        on,
    );
    ui.painter().circle_filled(egui::pos2(fill_to, track.center().y), if resp.hovered() || resp.dragged() { 7.0 } else { 6.0 }, on);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let v = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let changed = (v - *value).abs() > 0.001;
            *value = v;
            return changed;
        }
    }
    false
}
