//! Aura Optanomai — a small window for arranging your monitors the way they
//! really sit on the desk, with a dial for the resolution.
//!
//! It talks to the **sway** compositor through `swaymsg`.

mod app;
mod canvas;
mod slider;
mod displays;
use aura_ui::theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura Optanomai")
            .with_app_id("aura-optanomai")
            .with_inner_size([900.0, 560.0])
            .with_min_inner_size([720.0, 420.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Aura Optanomai",
        options,
        Box::new(|cc| Ok(Box::new(app::AuraOptanomai::new(cc)))),
    )
}
