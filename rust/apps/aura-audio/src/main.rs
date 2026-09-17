//! aura-audio — sound in one small window.
//!
//! Output: every speaker, HDMI port and headset — click one to make it the
//! sound output; a level bar and a mute switch for it. Input: the same for
//! microphones. Talks to PipeWire through wpctl, off the UI thread.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aura_ui::{theme, widgets};
use egui::{Frame, Key, Margin, RichText, ScrollArea};

#[derive(Clone, Debug, PartialEq)]
struct Node {
    id: u32,
    name: String,
    volume: f32,
    muted: bool,
    default: bool,
}

#[derive(Clone, Default)]
struct State {
    sinks: Vec<Node>,
    sources: Vec<Node>,
    loaded: bool,
    status: String,
}

type Shared = Arc<Mutex<State>>;

fn wpctl(args: &[&str]) -> String {
    Command::new("timeout")
        .arg("3")
        .arg("wpctl")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

/// The "Sinks:" / "Sources:" blocks of `wpctl status` (Audio section only).
fn parse_status(text: &str) -> (Vec<Node>, Vec<Node>) {
    let mut sinks = Vec::new();
    let mut sources = Vec::new();
    let mut section = "";
    for raw in text.lines() {
        if raw.starts_with("Video") || raw.starts_with("Settings") {
            break; // Audio comes first; stop before Video's own Sinks/Sources
        }
        let line = raw.trim_start_matches(|c: char| " │├└─".contains(c));
        if line.starts_with("Sinks:") {
            section = "sinks";
            continue;
        }
        if line.starts_with("Sources:") {
            section = "sources";
            continue;
        }
        if line.ends_with(':') {
            section = "";
            continue;
        }
        if section.is_empty() {
            continue;
        }
        let default = line.trim_start().starts_with('*');
        let body = line.trim_start().trim_start_matches('*').trim_start();
        let Some((id, rest)) = body.split_once(". ") else { continue };
        let Ok(id) = id.trim().parse::<u32>() else { continue };
        let (name, tail) = match rest.rfind("[vol:") {
            Some(i) => (rest[..i].trim(), &rest[i..]),
            None => (rest.trim(), ""),
        };
        let volume = tail
            .trim_start_matches("[vol:")
            .split_whitespace()
            .next()
            .and_then(|v| v.trim_end_matches(']').parse().ok())
            .unwrap_or(1.0);
        let node = Node { id, name: name.to_string(), volume, muted: tail.contains("MUTED"), default };
        if section == "sinks" { sinks.push(node) } else { sources.push(node) }
    }
    (sinks, sources)
}

fn refresh(s: &Shared) {
    let (sinks, sources) = parse_status(&wpctl(&["status"]));
    let mut st = s.lock().unwrap();
    st.sinks = sinks;
    st.sources = sources;
    st.loaded = true;
}

/// Friendlier names: "Radeon High Definition Audio Controller HDMI / DisplayPort 2 Output" → "HDMI / DisplayPort 2"
fn short(name: &str) -> String {
    let n = name
        .replace("Radeon High Definition Audio Controller ", "")
        .replace("Ryzen HD Audio Controller ", "")
        .replace(" Output", "");
    if n.is_empty() { name.to_string() } else { n }
}

fn icon(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("hdmi") || n.contains("displayport") {
        "▭"
    } else if n.contains("headset") || n.contains("headphone") || n.contains("bluez") {
        "♪"
    } else if n.contains("mic") {
        "◎"
    } else {
        "◉"
    }
}

struct App {
    state: Shared,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    /// don't let a refresh yank the bar back while it's being dragged
    last_drag: Instant,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        let state: Shared = Arc::new(Mutex::new(State::default()));
        let (s, ctx) = (state.clone(), cc.egui_ctx.clone());
        std::thread::spawn(move || loop {
            refresh(&s);
            ctx.request_repaint();
            std::thread::sleep(Duration::from_secs(2));
        });
        Self { state, theme_id, next_theme_check: 0.0, last_drag: Instant::now() - Duration::from_secs(10) }
    }

    fn run(&self, ctx: &egui::Context, args: Vec<String>) {
        let (s, ctx) = (self.state.clone(), ctx.clone());
        std::thread::spawn(move || {
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            wpctl(&a);
            refresh(&s);
            ctx.request_repaint();
        });
    }

    fn section(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, title: &str, nodes: &mut Vec<Node>) {
        widgets::caption(ui, title);
        if nodes.is_empty() {
            widgets::caption(ui, "none found");
            return;
        }
        for n in nodes.iter() {
            if widgets::row(ui, icon(&n.name), &short(&n.name), if n.default { "in use" } else { "" }).clicked() && !n.default {
                self.run(ctx, vec!["set-default".into(), n.id.to_string()]);
                self.state.lock().unwrap().status = format!("Sound now uses {}", short(&n.name));
            }
        }
        if let Some(cur) = nodes.iter_mut().find(|n| n.default) {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let pct = format!("{:>3}%", (cur.volume * 100.0).round() as i32);
                ui.label(RichText::new(pct).color(theme::subtext0()));
                let mute_label = if cur.muted { "unmute" } else { "mute" };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(mute_label).clicked() {
                        self.run(ctx, vec!["set-mute".into(), cur.id.to_string(), "toggle".into()]);
                    }
                    let mut v = cur.volume.min(1.0);
                    if widgets::level(ui, &mut v, cur.muted) {
                        cur.volume = v;
                        self.last_drag = Instant::now();
                        self.run(ctx, vec!["set-volume".into(), cur.id.to_string(), format!("{v:.2}")]);
                    }
                });
            });
        }
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
            self.next_theme_check = now + 2.0;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // while dragging, keep the local value instead of the 2 s refresh
        let st = self.state.lock().unwrap().clone();
        let (mut sinks, mut sources) = (st.sinks.clone(), st.sources.clone());

        if !st.status.is_empty() {
            egui::TopBottomPanel::bottom("status")
                .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(12, 6)))
                .show_separator_line(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&st.status).color(theme::subtext0()));
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "audio", "where sound goes, and how loud");
                widgets::hairline(ui);
                if !st.loaded {
                    widgets::caption(ui, "reading devices…");
                }
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.section(ui, ctx, "output  ·  click to use", &mut sinks);
                    widgets::hairline(ui);
                    self.section(ui, ctx, "microphone  ·  click to use", &mut sources);
                });
            });

        // write dragged values back so the bar doesn't jump until wpctl catches up
        if self.last_drag.elapsed() < Duration::from_millis(1500) {
            let mut m = self.state.lock().unwrap();
            m.sinks = sinks;
            m.sources = sources;
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura audio")
            .with_app_id("aura-audio")
            .with_inner_size([380.0, 480.0])
            .with_min_inner_size([320.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native("Aura audio", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "PipeWire 'pipewire-0' [1.4.7]\nAudio\n ├─ Devices:\n │      45. Radeon [alsa]\n │  \n ├─ Sinks:\n │  *   35. Ryzen HD Audio Controller Speaker   [vol: 0.95]\n │      97. Lenovo Wireless Stereo Headset      [vol: 0.66 MUTED]\n │  \n ├─ Sources:\n │  *   52. Ryzen HD Audio Controller Digital Microphone [vol: 1.00]\n │  \n ├─ Filters:\n │      90. bluez_input.50:1B [Audio/Source]\n\nVideo\n ├─ Sinks:\n │      5. Not audio [vol: 1.00]\n";

    #[test]
    fn parses_audio_sinks_and_sources_only() {
        let (sinks, sources) = parse_status(STATUS);
        assert_eq!(sinks.len(), 2);
        assert!(sinks[0].default && sinks[0].id == 35 && (sinks[0].volume - 0.95).abs() < 1e-6);
        assert!(sinks[1].muted && !sinks[1].default);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "Ryzen HD Audio Controller Digital Microphone");
    }

    #[test]
    fn short_names() {
        assert_eq!(short("Radeon High Definition Audio Controller HDMI / DisplayPort 2 Output"), "HDMI / DisplayPort 2");
    }
}
