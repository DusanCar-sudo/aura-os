//! aura-settings — the settings hub (⚙ in the top bar).
//!
//! Every setting one click away, each row showing its current state, in the
//! shared aura-ui look. Toggles (Wi-Fi, Bluetooth, flight mode) switch in
//! place; everything else opens its own tool and the hub closes. Esc, or a
//! click outside the window, closes it too.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aura_ui::{theme, widgets};
use egui::{Frame, Key, Margin, ScrollArea};

/// What the rows show. Filled by a background thread so a slow probe
/// (bluetoothctl can hang with no controller) never freezes the window.
#[derive(Clone, Default)]
struct State {
    wifi: bool,
    ssid: String,
    bluetooth: bool,
    flight: bool,
    sink: String,
    loaded: bool,
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/root".into())
}

/// Run a probe with a hard timeout; stdout trimmed, empty on any failure.
fn probe(args: &[&str]) -> String {
    let mut cmd = Command::new("timeout");
    cmd.arg("3").args(args).stdin(Stdio::null()).stderr(Stdio::null());
    cmd.output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn read_state() -> State {
    let wifi = probe(&["nmcli", "radio", "wifi"]) == "enabled";
    let ssid = probe(&["nmcli", "-t", "-f", "NAME,TYPE", "connection", "show", "--active"])
        .lines()
        .find_map(|l| l.strip_suffix(":802-11-wireless").map(str::to_string))
        .unwrap_or_default();
    let bluetooth = probe(&["bluetoothctl", "show"]).contains("Powered: yes");
    let flight = probe(&[&format!("{}/.local/bin/aura-airplane", home()), "status"]) == "on";
    let sink = probe(&["wpctl", "inspect", "@DEFAULT_AUDIO_SINK@"])
        .lines()
        .find_map(|l| {
            l.trim()
                .trim_start_matches('*')
                .trim()
                .strip_prefix("node.description = ")
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_default();
    State { wifi, ssid, bluetooth, flight, sink, loaded: true }
}

/// Start a program detached from us, so closing the hub never closes it.
fn spawn(cmd: &str) {
    let _ = Command::new("setsid")
        .args(["-f", "sh", "-c", cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// A row: icon, label, and the shell command that opens it.
struct Item {
    icon: &'static str,
    label: &'static str,
    cmd: String,
}

fn items() -> Vec<(&'static str, Vec<Item>)> {
    let h = home();
    let b = |n: &str| format!("{h}/.local/bin/{n}");
    let i = |icon, label, cmd: String| Item { icon, label, cmd };
    vec![
        (
            "look & screens",
            vec![
                i("▭", "Screens", b("aura-optanomai")),
                i("◐", "Appearance: theme, glass, borders", b("aura-appearance")),
                i("▣", "Wallpaper", "aura-os-wallpaper pick".into()),
                i("⌨", "Keyboard", b("aura-keyboard")),
            ],
        ),
        (
            "apps",
            vec![
                i("▦", "Files", format!("aura-term --app-id=aura-files -e {}", b("aura-files"))),
                i("¶", "Text editor", format!("aura-term --app-id=aura-edit -e {}", b("aura-edit"))),
                i("▤", "System monitor", "aura-term --app-id=aura-float -e btop".into()),
                i("✕", "Processes: see and end", b("aura-os-procs")),
                i("◎", "Local AI speed test", format!(
                    "aura-term --app-id=aura-float -e bash -c '{}; printf \"\\n[any key]\"; read -rsn1'",
                    b("aura-os-llmtest")
                )),
            ],
        ),
        (
            "system",
            vec![
                i("⊕", "Updates", "aura-os-settings updates".into()),
                i("◉", "Snapshots & roll back", "aura-os-settings snapshots".into()),
                i("⌘", "Check my system", format!("{} --menu", b("aura-doctor"))),
                i("…", "Advanced settings", "aura-os-settings".into()),
                i("⏻", "Power: reboot, shut down, log out", "aura-os-power".into()),
            ],
        ),
    ]
}

struct Hub {
    state: Arc<Mutex<State>>,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    had_focus: bool,
}

impl Hub {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);

        let state = Arc::new(Mutex::new(State::default()));
        let (s, ctx) = (state.clone(), cc.egui_ctx.clone());
        std::thread::spawn(move || loop {
            let fresh = read_state();
            *s.lock().unwrap() = fresh;
            ctx.request_repaint();
            std::thread::sleep(Duration::from_secs(3));
        });
        Self { state, theme_id, next_theme_check: 0.0, had_focus: false }
    }

    /// Flip a toggle now, then re-read the real state.
    fn toggle(&self, ctx: &egui::Context, what: &str, on: bool) {
        let cmd = match (what, on) {
            ("wifi", true) => "nmcli radio wifi off".to_string(),
            ("wifi", false) => "nmcli radio wifi on".to_string(),
            ("bt", true) => "bluetoothctl power off".to_string(),
            ("bt", false) => "rfkill unblock bluetooth; bluetoothctl power on".to_string(),
            _ => format!("{}/.local/bin/aura-airplane", home()),
        };
        {
            let mut s = self.state.lock().unwrap();
            match what {
                "wifi" => s.wifi = !on,
                "bt" => s.bluetooth = !on,
                _ => s.flight = !on,
            }
        }
        let (s, ctx) = (self.state.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = Command::new("sh").args(["-c", &cmd]).stdin(Stdio::null())
                .stdout(Stdio::null()).stderr(Stdio::null()).status();
            std::thread::sleep(Duration::from_millis(600));
            *s.lock().unwrap() = read_state();
            ctx.request_repaint();
        });
    }
}

impl eframe::App for Hub {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // follow a theme switch while open
        let now = ctx.input(|i| i.time);
        if now >= self.next_theme_check {
            let t = theme::ThemeId::detect();
            if t != self.theme_id {
                theme::apply(ctx, &t);
                self.theme_id = t;
            }
            self.next_theme_check = now + 2.0;
        }

        // popup behaviour: Esc closes, and so does focus leaving the window
        let focused = ctx.input(|i| i.viewport().focused);
        if focused == Some(true) {
            self.had_focus = true;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) || (self.had_focus && focused == Some(false)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        let st = self.state.lock().unwrap().clone();
        let mut close = false;

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "settings", "everything, one click away");
                widgets::hairline(ui);

                let dash = |s: &str| if s.is_empty() { String::new() } else { format!("— {s}") };
                if widgets::toggle_row(ui, "Wi-Fi", &dash(&st.ssid), st.wifi).clicked() {
                    self.toggle(ctx, "wifi", st.wifi);
                }
                if widgets::toggle_row(ui, "Bluetooth", "", st.bluetooth).clicked() {
                    self.toggle(ctx, "bt", st.bluetooth);
                }
                if widgets::toggle_row(ui, "Flight mode", "", st.flight).clicked() {
                    self.toggle(ctx, "flight", st.flight);
                }
                let h = home();
                if widgets::row(ui, "≋", "Wi-Fi networks", "").clicked() {
                    spawn(&format!("{h}/.local/bin/aura-wifi")); close = true;
                }
                if widgets::row(ui, "◈", "Bluetooth devices", "").clicked() {
                    spawn(&format!("{h}/.local/bin/aura-bluetooth")); close = true;
                }
                if widgets::row(ui, "♪", "Audio", &st.sink).clicked() {
                    spawn(&format!("{h}/.local/bin/aura-audio")); close = true;
                }
                if !st.loaded {
                    widgets::caption(ui, "reading state…");
                }

                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (section, rows) in items() {
                        widgets::hairline(ui);
                        widgets::caption(ui, section);
                        for it in rows {
                            if widgets::row(ui, it.icon, it.label, "").clicked() {
                                spawn(&it.cmd);
                                close = true;
                            }
                        }
                    }
                });
            });

        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura settings")
            .with_app_id("aura-settings")
            .with_inner_size([380.0, 600.0])
            .with_min_inner_size([320.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native("Aura settings", options, Box::new(|cc| Ok(Box::new(Hub::new(cc)))))
}
