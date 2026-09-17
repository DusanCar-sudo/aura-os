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
