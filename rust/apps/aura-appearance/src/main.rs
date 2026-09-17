//! aura-appearance — how Aura OS looks, in one window.
//!
//! Theme: pick one; your own panel/border colours reset to follow it.
//! Panels & menus: glass or fill, colour, how solid, blur.
//! Windows: border width, active and inactive border colour.
//! Motion: animations on/off and how fast.
//!
//! Choices are written to ~/.config/aura-os/look (the same key=value file
//! the aura-look script reads) and applied with `aura-look apply`, which
//! writes the sway, waybar and menu files. Slider drags apply once, shortly
//! after you let go, so sway isn't reloaded 60 times a second.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aura_ui::{theme, widgets};
use egui::{Color32, Frame, Key, Margin, RichText, ScrollArea};

const DEFAULTS: &[(&str, &str)] = &[
    ("panels", "glass"),
    ("panel_color", "theme"),
    ("panel_alpha", "85"),
    ("blur", "6"),
    ("border_width", "4"),
    ("border_color", "theme"),
    ("border_inactive", "theme"),
    ("animations", "on"),
    ("anim_ms", "220"),
];

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".into()))
}

fn config() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|_| home().join(".config"))
}

fn state_file() -> PathBuf {
    config().join("aura-os").join("look")
}

fn load() -> BTreeMap<String, String> {
    let mut m: BTreeMap<String, String> = DEFAULTS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    if let Ok(text) = std::fs::read_to_string(state_file()) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                // first value wins, like the script's grep -m1
                if !text.lines().take_while(|l| *l != line).any(|l| l.starts_with(&format!("{k}="))) {
                    m.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
    }
    m
}

fn save(m: &BTreeMap<String, String>) {
    let body: String = m.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    let path = state_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(tmp, path);
    }
}

fn run_bg(cmd: Vec<String>) {
    std::thread::spawn(move || {
        let _ = Command::new(&cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });
}

fn look_script() -> String {
    let local = home().join(".local/bin/aura-look");
    if local.exists() { local.display().to_string() } else { "aura-look".into() }
}

fn themes() -> (Vec<String>, String) {
    let out = Command::new("timeout")
        .args(["3", "aura-os-theme", "list"])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let mut current = String::new();
    let names = out
        .lines()
        .filter_map(|l| {
            let name = l.trim_start_matches('*').trim();
            if l.starts_with('*') {
                current = name.to_string();
            }
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect();
    (names, current)
}

fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Three swatches for a theme row (bg, accent, fg), from its palette file.
fn swatches(name: &str) -> Option<[Color32; 3]> {
    let repo = std::env::var("AURA_OS_REPO").map(PathBuf::from).unwrap_or_else(|_| home().join("aura-os"));
    for dir in [config().join("aura-os/themes"), PathBuf::from("/usr/share/aura-os/themes"), repo.join("themes")] {
        if let Ok(text) = std::fs::read_to_string(dir.join(name).join("palette")) {
            let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}=")).and_then(parse_hex));
            return Some([get("bg")?, get("accent")?, get("fg")?]);
        }
    }
    None
}

/// A left-aligned label in a fixed-width column, so rows line up.
fn fixed(ui: &mut egui::Ui, width: f32, text: RichText) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.add_space(8.0);
            ui.label(text);
        },
    );
}

struct App {
    look: BTreeMap<String, String>,
    themes: Vec<String>,
    current_theme: String,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    /// a change waiting to be applied (slider drags settle first)
    pending: Option<Instant>,
    status: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        let (themes, current_theme) = themes();
        Self { look: load(), themes, current_theme, theme_id, next_theme_check: 0.0, pending: None, status: String::new() }
    }

    fn get(&self, k: &str) -> String {
        self.look.get(k).cloned().unwrap_or_default()
    }

    fn num(&self, k: &str) -> f32 {
        self.get(k).parse().unwrap_or(0.0)
    }

    fn set(&mut self, k: &str, v: impl Into<String>, now: bool) {
        self.look.insert(k.to_string(), v.into());
        save(&self.look);
        self.pending = Some(if now { Instant::now() - Duration::from_secs(1) } else { Instant::now() });
    }

    /// A labelled number slider snapped to `step`.
    fn slider(&mut self, ui: &mut egui::Ui, label: &str, key: &str, min: f32, max: f32, step: f32, unit: &str) {
        let mut v = (self.num(key) - min) / (max - min);
        ui.horizontal(|ui| {
            fixed(ui, 150.0, RichText::new(label).color(theme::text()));
            let shown = format!("{}{unit}", self.num(key) as i32);
            fixed(ui, 64.0, RichText::new(shown).color(theme::subtext0()));
            if widgets::level(ui, &mut v, false) {
                let raw = min + v * (max - min);
                let snapped = ((raw / step).round() * step).clamp(min, max);
                if (snapped - self.num(key)).abs() > f32::EPSILON {
                    self.set(key, (snapped as i32).to_string(), false);
                }
            }
        });
    }

    /// "follow theme" or a colour you pick.
    fn colour(&mut self, ui: &mut egui::Ui, label: &str, key: &str) {
        ui.horizontal(|ui| {
            fixed(ui, 150.0, RichText::new(label).color(theme::text()));
            let value = self.get(key);
            let follows = value == "theme" || value.is_empty();
            if ui.selectable_label(follows, "theme").clicked() && !follows {
                self.set(key, "theme", true);
            }
            let mut c = parse_hex(&value).unwrap_or(theme::accent());
            let before = c;
            let resp = egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque);
            if c != before {
                self.set(key, hex(c), false);
            }
            if !follows {
                ui.label(RichText::new(&value).color(theme::overlay0()));
            }
            resp
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);
        if now >= self.next_theme_check {
            let t = theme::ThemeId::detect();
            if t != self.theme_id {
                theme::apply(ctx, &t);
                self.theme_id = t;
            }
            self.next_theme_check = now + 1.0;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // apply 400 ms after the last change, once
        if let Some(t) = self.pending {
            if t.elapsed() >= Duration::from_millis(400) {
                self.pending = None;
                run_bg(vec![look_script(), "apply".into()]);
                self.status = "applied".into();
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        if !self.status.is_empty() {
            egui::TopBottomPanel::bottom("status")
                .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(12, 6)))
                .show_separator_line(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&self.status).color(theme::subtext0()));
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "appearance", "theme, glass, borders, motion");
                widgets::hairline(ui);
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    widgets::caption(ui, "theme  ·  picking one resets your own colours");
                    for name in self.themes.clone() {
                        let on = name == self.current_theme;
                        let (row_rect, clicked) = {
                            let r = widgets::row(ui, if on { "●" } else { "○" }, &name, "");
                            (r.rect, r.clicked())
                        };
                        if let Some(sw) = swatches(&name) {
                            for (i, c) in sw.iter().enumerate() {
                                let x = row_rect.right() - 16.0 - (2 - i) as f32 * 16.0;
                                let r = egui::Rect::from_center_size(egui::pos2(x, row_rect.center().y), egui::vec2(12.0, 12.0));
                                ui.painter().rect_filled(r, 2.0, *c);
                                ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.0_f32, theme::surface1()), egui::StrokeKind::Inside);
                            }
                        }
                        if clicked && !on {
                            self.current_theme = name.clone();
                            // the bar's picker, then our reset (covers an older aura-os-theme too)
                            let look = look_script();
                            std::thread::spawn(move || {
                                let _ = Command::new("aura-os-theme").args(["apply", &name])
                                    .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
                                let _ = Command::new(look).arg("theme-changed")
                                    .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
                            });
                            for k in ["panel_color", "border_color", "border_inactive"] {
                                self.look.insert(k.into(), "theme".into());
                            }
                            self.status = format!("theme: {}", self.current_theme);
                        }
                    }

                    widgets::hairline(ui);
                    widgets::caption(ui, "panels & menus");
                    let glass = self.get("panels") != "fill";
                    if widgets::toggle_row(ui, "Glass", if glass { "see the wallpaper through" } else { "solid fill colour" }, glass).clicked() {
                        self.set("panels", if glass { "fill" } else { "glass" }, true);
                    }
                    self.colour(ui, "Panel colour", "panel_color");
                    if glass {
                        self.slider(ui, "Solid", "panel_alpha", 50.0, 100.0, 5.0, "%");
                        self.slider(ui, "Blur", "blur", 0.0, 10.0, 1.0, "");
                    }

                    widgets::hairline(ui);
                    widgets::caption(ui, "window borders");
                    self.slider(ui, "Width", "border_width", 0.0, 12.0, 1.0, " px");
                    self.colour(ui, "Active", "border_color");
                    self.colour(ui, "Inactive", "border_inactive");

                    widgets::hairline(ui);
                    widgets::caption(ui, "motion  ·  needs SwayFX");
                    let anim = self.get("animations") == "on";
                    if widgets::toggle_row(ui, "Animations", "", anim).clicked() {
                        self.set("animations", if anim { "off" } else { "on" }, true);
                    }
                    if anim {
                        self.slider(ui, "Speed", "anim_ms", 100.0, 500.0, 20.0, " ms");
                    }

                    widgets::hairline(ui);
                    if widgets::row(ui, "⊙", "Reset to defaults", "").clicked() {
                        self.look = DEFAULTS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
                        let _ = std::fs::remove_file(state_file());
                        self.pending = Some(Instant::now() - Duration::from_secs(1));
                        self.status = "back to defaults".into();
                    }
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura appearance")
            .with_app_id("aura-appearance")
            .with_inner_size([420.0, 640.0])
            .with_min_inner_size([360.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native("Aura appearance", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
