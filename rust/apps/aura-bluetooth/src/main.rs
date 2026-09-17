//! aura-bluetooth — Bluetooth in one small window.
//!
//! Power switch; your paired devices with their state (click to connect or
//! disconnect, × to forget); "Find new devices" scans for 12 seconds and
//! lists what's nearby — click one to pair, trust and connect it in one go.
//! Mice, keyboards and headsets need nothing more. Every bluetoothctl call
//! runs off the UI thread with a timeout, so nothing can freeze the window.

use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aura_ui::{theme, widgets};
use egui::{Frame, Key, Margin, RichText, ScrollArea};

#[derive(Clone, Debug, PartialEq)]
struct Device {
    mac: String,
    name: String,
    icon: String,
    connected: bool,
    battery: Option<u8>,
}

#[derive(Clone, Default)]
struct State {
    powered: bool,
    paired: Vec<Device>,
    nearby: Vec<Device>,
    scanning: bool,
    /// the device a connect/pair is running for
    busy: Option<String>,
    status: String,
    status_bad: bool,
    loaded: bool,
}

type Shared = Arc<Mutex<State>>;

/// bluetoothctl with a hard timeout; stdout, empty on failure.
fn btctl(secs: u32, args: &[&str]) -> (bool, String) {
    let out = Command::new("timeout")
        .arg(secs.to_string())
        .arg("bluetoothctl")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match out {
        Ok(o) => (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string()),
        Err(_) => (false, String::new()),
    }
}

/// "Device AA:BB:… Name" lines → (mac, name)
fn parse_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
            (mac.len() == 17).then(|| (mac.to_string(), name.to_string()))
        })
        .collect()
}

fn info(mac: &str, name: &str) -> Device {
    let (_, text) = btctl(3, &["info", mac]);
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string()))
    };
    let battery = field("Battery Percentage:").and_then(|v| {
        // "0x5a (90)"
        v.split_once('(')
            .and_then(|(_, b)| b.trim_end_matches(')').parse().ok())
    });
    Device {
        mac: mac.to_string(),
        name: field("Name:").unwrap_or_else(|| name.to_string()),
        icon: field("Icon:").unwrap_or_default(),
        connected: field("Connected:").as_deref() == Some("yes"),
        battery,
    }
}

fn refresh(s: &Shared) {
    let powered = btctl(3, &["show"]).1.contains("Powered: yes");
    // "your devices": paired, trusted or connected — a headset can be
    // connected and trusted without bluez listing it as paired
    let mut known: Vec<(String, String)> = Vec::new();
    for filter in ["Paired", "Trusted", "Connected"] {
        for d in parse_devices(&btctl(3, &["devices", filter]).1) {
            if !known.iter().any(|k| k.0 == d.0) {
                known.push(d);
            }
        }
    }
    let paired: Vec<Device> = known.iter().map(|(m, n)| info(m, n)).collect();
    let mut st = s.lock().unwrap();
    st.powered = powered;
    st.paired = paired;
    st.loaded = true;
}

fn icon_for(d: &Device) -> &'static str {
    match d.icon.as_str() {
        i if i.starts_with("audio") => "♪",
        "input-mouse" => "◉",
        "input-keyboard" => "⌨",
        "input-gaming" => "◎",
        "phone" => "▯",
        _ => "◈",
    }
}

fn set_status(s: &Shared, text: impl Into<String>, bad: bool) {
    let mut st = s.lock().unwrap();
    st.status = text.into();
    st.status_bad = bad;
}

/// Run `job` off the UI thread, then refresh and repaint.
fn background(s: &Shared, ctx: &egui::Context, job: impl FnOnce(&Shared) + Send + 'static) {
    let (s, ctx) = (s.clone(), ctx.clone());
    std::thread::spawn(move || {
        job(&s);
        refresh(&s);
        s.lock().unwrap().busy = None;
        ctx.request_repaint();
    });
}

struct App {
    state: Shared,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        let state: Shared = Arc::new(Mutex::new(State::default()));
        let (s, ctx) = (state.clone(), cc.egui_ctx.clone());
        std::thread::spawn(move || loop {
            // while a job runs it refreshes by itself
            if s.lock().unwrap().busy.is_none() {
                refresh(&s);
                ctx.request_repaint();
            }
            std::thread::sleep(Duration::from_secs(4));
        });
        Self { state, theme_id, next_theme_check: 0.0 }
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

        let st = self.state.lock().unwrap().clone();
        let s = &self.state;

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "bluetooth", "mouse, keyboard, headphones");
                widgets::hairline(ui);

                if widgets::toggle_row(ui, "Bluetooth", "", st.powered).clicked() {
                    let on = st.powered;
                    s.lock().unwrap().powered = !on;
                    background(s, ctx, move |s| {
                        let ok = if on {
                            btctl(5, &["power", "off"]).0
                        } else {
                            let _ = Command::new("rfkill").args(["unblock", "bluetooth"]).status();
                            btctl(5, &["power", "on"]).0
                        };
                        if !ok {
                            set_status(s, "Couldn't switch Bluetooth — is the adapter blocked?", true);
                        }
                    });
                }

                if !st.loaded {
                    widgets::caption(ui, "reading devices…");
                }

                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    widgets::hairline(ui);
                    widgets::caption(ui, "your devices  ·  click to connect or disconnect");
                    if st.loaded && st.paired.is_empty() {
                        widgets::caption(ui, "none yet — find new devices below");
                    }
                    for d in &st.paired {
                        let busy = st.busy.as_deref() == Some(&d.mac);
                        let mut value = if busy {
                            "…".to_string()
                        } else if d.connected {
                            "connected".to_string()
                        } else {
                            String::new()
                        };
                        if let Some(b) = d.battery {
                            value = format!("{b}%  {value}");
                        }
                        let (clicked, forget) =
                            widgets::row_with_remove(ui, icon_for(d), &d.name, &value, "forget this device");
                        if forget && st.busy.is_none() {
                            let (mac, name) = (d.mac.clone(), d.name.clone());
                            s.lock().unwrap().busy = Some(mac.clone());
                            background(s, ctx, move |s| {
                                let ok = btctl(8, &["remove", &mac]).0;
                                set_status(s, if ok { format!("Forgot {name}") } else { format!("Couldn't forget {name}") }, !ok);
                            });
                        }
                        if clicked && st.powered && st.busy.is_none() {
                            let (mac, name, conn) = (d.mac.clone(), d.name.clone(), d.connected);
                            s.lock().unwrap().busy = Some(mac.clone());
                            set_status(s, if conn { format!("Disconnecting {name}…") } else { format!("Connecting {name}…") }, false);
                            background(s, ctx, move |s| {
                                let ok = btctl(15, &[if conn { "disconnect" } else { "connect" }, &mac]).0;
                                let msg = match (conn, ok) {
                                    (false, true) => format!("Connected {name}"),
                                    (true, true) => format!("Disconnected {name}"),
                                    (false, false) => format!("Couldn't connect {name} — is it on and nearby?"),
                                    (true, false) => format!("Couldn't disconnect {name}"),
                                };
                                set_status(s, msg, !ok);
                            });
                        }
                    }

                    widgets::hairline(ui);
                    let label = if st.scanning { "Searching nearby…" } else { "Find new devices" };
                    if widgets::row(ui, "⊕", label, if st.scanning { "12 s" } else { "" }).clicked()
                        && st.powered
                        && !st.scanning
                    {
                        {
                            let mut m = s.lock().unwrap();
                            m.scanning = true;
                            m.nearby.clear();
                        }
                        set_status(s, "Put your device in pairing mode", false);
                        let (s2, ctx2) = (s.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            let _ = btctl(13, &["--timeout", "12", "scan", "on"]);
                            let paired: Vec<String> = s2.lock().unwrap().paired.iter().map(|d| d.mac.clone()).collect();
                            let nearby: Vec<Device> = parse_devices(&btctl(3, &["devices"]).1)
                                .into_iter()
                                .filter(|(m, _)| !paired.contains(m))
                                .map(|(m, n)| info(&m, &n))
                                // unnamed devices are just MACs — nobody can tell what they are
                                .filter(|d| !d.name.is_empty() && d.name.replace('-', ":") != d.mac)
                                .collect();
                            let mut m = s2.lock().unwrap();
                            m.status = if nearby.is_empty() {
                                "Nothing found — is the device in pairing mode?".into()
                            } else {
                                "Click a device to pair it".into()
                            };
                            m.status_bad = false;
                            m.nearby = nearby;
                            m.scanning = false;
                            ctx2.request_repaint();
                        });
                    }
                    if !st.powered && st.loaded {
                        widgets::caption(ui, "turn Bluetooth on to connect or find devices");
                    }
                    for d in &st.nearby {
                        let busy = st.busy.as_deref() == Some(&d.mac);
                        if widgets::row(ui, icon_for(d), &d.name, if busy { "pairing…" } else { "pair" }).clicked()
                            && st.busy.is_none()
                        {
                            let (mac, name) = (d.mac.clone(), d.name.clone());
                            s.lock().unwrap().busy = Some(mac.clone());
                            set_status(s, format!("Pairing {name}…"), false);
                            background(s, ctx, move |s| {
                                let paired = btctl(25, &["pair", &mac]).0;
                                let _ = btctl(5, &["trust", &mac]);
                                let connected = paired && btctl(15, &["connect", &mac]).0;
                                let msg = match (paired, connected) {
                                    (true, true) => format!("{name} is paired and connected"),
                                    (true, false) => format!("{name} is paired — click it above to connect"),
                                    _ => format!("Couldn't pair {name} — keep it in pairing mode and try again"),
                                };
                                set_status(s, msg, !paired);
                                let mut m = s.lock().unwrap();
                                if paired {
                                    m.nearby.retain(|d| d.mac != mac);
                                }
                            });
                        }
                    }
                });
            });

        // status line along the bottom
        if !st.status.is_empty() {
            egui::TopBottomPanel::bottom("status")
                .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(12, 6)))
                .show_separator_line(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&st.status).color(if st.status_bad { theme::bad() } else { theme::subtext0() }));
                });
        }
        if st.scanning || st.busy.is_some() {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura bluetooth")
            .with_app_id("aura-bluetooth")
            .with_inner_size([380.0, 460.0])
            .with_min_inner_size([320.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native("Aura bluetooth", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    #[test]
    fn devices_lines_parse() {
        let v = super::parse_devices("Device 50:1B:6A:8A:10:F9 Lenovo Wireless Stereo Headset\nnoise\n");
        assert_eq!(v, vec![("50:1B:6A:8A:10:F9".to_string(), "Lenovo Wireless Stereo Headset".to_string())]);
    }
}
