//! The Aura icon pack (assets/icons), compiled in and drawn tinted: every
//! glyph is one-color line art, so one SVG serves any theme color, and it
//! stays sharp at any size.

use egui::{Color32, Rect, Ui};

macro_rules! pack {
    ($($name:literal),* $(,)?) => {
        const PACK: &[(&str, &str)] = &[
            $(($name, include_str!(concat!("../../../../assets/icons/", $name, ".svg")))),*
        ];
    };
}

pack!(
    "files", "file-plain", "file-text", "file-image", "file-pdf", "file-exec",
    "code", "media", "audio", "package", "terminal", "settings", "search",
    "history", "restore", "snapshot", "workspace", "network", "system", "aura",
);

/// Call once at startup (aura_ui::theme::install_fonts does it).
pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// Draw icon `name` into `rect`, in `color`. Unknown names draw nothing.
pub fn paint(ui: &Ui, name: &str, rect: Rect, color: Color32) {
    let Some((_, svg)) = PACK.iter().find(|(n, _)| *n == name) else { return };
    // white line art + tint = the theme color, whatever it is
    let white = svg.replace("currentColor", "#ffffff").replace("color=\"#", "data-c=\"#");
    egui::Image::from_bytes(format!("bytes://aura-icon/{name}.svg"), white.into_bytes())
        .tint(color)
        .paint_at(ui, rect);
}
