//! aura-wifi — Wi-Fi in one small window.
//!
//! Switch; the networks around you, strongest first, the one you're on at
//! the top. Click a saved or open network to join; click a locked new one
//! and a password line opens right under it. × forgets a saved network.
//! The password goes to nmcli on stdin (--ask), never on a command line
//! where other programs could read it. All nmcli calls run off the UI
//! thread with a timeout.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aura_ui::{theme, widgets};
use egui::{Frame, Key, Margin, RichText, ScrollArea, TextEdit};

#[derive(Clone, Debug, PartialEq)]
struct Net {
    ssid: String,
    signal: u8,
    secure: bool,
    in_use: bool,
    saved: bool,
}

#[derive(Clone, Default)]
struct State {
    on: bool,
    nets: Vec<Net>,
    scanning: bool,
    busy: Option<String>,
    status: String,
    status_bad: bool,
    loaded: bool,
}

type Shared = Arc<Mutex<State>>;

fn nmcli(secs: u32, args: &[&str], stdin: Option<&str>) -> (bool, String) {
    let child = Command::new("timeout")
        .arg(secs.to_string())
        .arg("nmcli")
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else { return (false, String::new()) };
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(text.as_bytes());
    }
    match child.wait_with_output() {
        Ok(o) => {
            let mut out = String::from_utf8_lossy(&o.stdout).to_string();
            out.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), out)
        }
        Err(_) => (false, String::new()),
    }
}

/// nmcli -t fields are ':'-separated with '\:' inside values.
fn split_terse(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.last_mut().unwrap().push(n);
                }
            }
            ':' => out.push(String::new()),
            _ => out.last_mut().unwrap().push(c),
        }
    }
    out
}

fn parse_list(list: &str, saved: &[String]) -> Vec<Net> {
    let mut nets: Vec<Net> = Vec::new();
    for line in list.lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[1].is_empty() {
            continue; // hidden networks have no name to show
        }
        let net = Net {
            in_use: f[0] == "*",
            ssid: f[1].clone(),
            signal: f[2].parse().unwrap_or(0),
            secure: !f[3].is_empty() && f[3] != "--",
            saved: saved.contains(&f[1]),
        };
        // the same network on several access points: keep the best one
        match nets.iter_mut().find(|n| n.ssid == net.ssid) {
            Some(n) => {
                n.in_use |= net.in_use;
                n.signal = n.signal.max(net.signal);
            }
            None => nets.push(net),
        }
    }
    nets.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    nets
}

fn refresh(s: &Shared, rescan: bool) {
    let on = nmcli(3, &["radio", "wifi"], None).1.trim() == "enabled";
    let saved: Vec<String> = nmcli(3, &["-t", "-f", "NAME,TYPE", "connection", "show"], None)
        .1
        .lines()
        .filter_map(|l| {
            let f = split_terse(l);
            (f.len() >= 2 && f[1] == "802-11-wireless").then(|| f[0].clone())
        })
        .collect();
    let list = if on {
        nmcli(
            if rescan { 15 } else { 5 },
            &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", if rescan { "yes" } else { "no" }],
            None,
        )
        .1
    } else {
        String::new()
    };
    let mut st = s.lock().unwrap();
    st.on = on;
    st.nets = parse_list(&list, &saved);
    st.loaded = true;
}

fn bars(signal: u8) -> &'static str {
    match signal {
        75.. => "▂▄▆█",
        50..=74 => "▂▄▆ ",
        25..=49 => "▂▄  ",
        _ => "▂   ",
    }
}

fn set_status(s: &Shared, text: impl Into<String>, bad: bool) {
    let mut st = s.lock().unwrap();
    st.status = text.into();
    st.status_bad = bad;
}

/// The first line nmcli explains a failure with, without the "Error: " noise.
fn reason(out: &str) -> String {
    out.lines()
        .find(|l| l.contains("Error") || l.contains("error"))
        .map(|l| l.replace("Error: ", "").trim().to_string())
        .unwrap_or_default()
}

fn background(s: &Shared, ctx: &egui::Context, job: impl FnOnce(&Shared) + Send + 'static) {
    let (s, ctx) = (s.clone(), ctx.clone());
    std::thread::spawn(move || {
        job(&s);
        refresh(&s, false);
        s.lock().unwrap().busy = None;
        ctx.request_repaint();
    });
}

struct App {
    state: Shared,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    /// the locked network whose password line is open, and what's typed
    asking: Option<String>,
    password: String,
    show_password: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        let state: Shared = Arc::new(Mutex::new(State::default()));
        let (s, ctx) = (state.clone(), cc.egui_ctx.clone());
        std::thread::spawn(move || {
            let mut first = true;
            loop {
                let busy = { let st = s.lock().unwrap(); st.busy.is_some() || st.scanning };
                if !busy {
                    // one real scan when the window opens, then cheap reads
                    refresh(&s, first);
                    first = false;
                    ctx.request_repaint();
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        });
        Self { state, theme_id, next_theme_check: 0.0, asking: None, password: String::new(), show_password: false }
    }

    fn join(&mut self, ctx: &egui::Context, net: &Net, password: Option<String>) {
        let ssid = net.ssid.clone();
        self.state.lock().unwrap().busy = Some(ssid.clone());
        set_status(&self.state, format!("Joining {ssid}…"), false);
        let saved = net.saved;
        background(&self.state, ctx, move |s| {
            let (ok, out) = if saved && password.is_none() {
                nmcli(30, &["connection", "up", "id", &ssid], None)
            } else if let Some(pw) = password {
                nmcli(30, &["--ask", "device", "wifi", "connect", &ssid], Some(&format!("{pw}\n")))
            } else {
                nmcli(30, &["device", "wifi", "connect", &ssid], None)
            };
            if ok {
                set_status(s, format!("Connected to {ssid}"), false);
            } else {
                let r = reason(&out);
                let wrong_pw = r.to_lowercase().contains("secrets") || r.to_lowercase().contains("password");
                set_status(
                    s,
                    if wrong_pw { format!("Wrong password for {ssid}") }
                    else if r.is_empty() { format!("Couldn't join {ssid}") }
                    else { format!("Couldn't join {ssid}: {r}") },
                    true,
                );
            }
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
            self.next_theme_check = now + 2.0;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.asking.is_some() {
                self.asking = None;
                self.password.clear();
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        let st = self.state.lock().unwrap().clone();

        if !st.status.is_empty() {
            egui::TopBottomPanel::bottom("status")
                .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(12, 6)))
                .show_separator_line(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&st.status).color(if st.status_bad { theme::bad() } else { theme::subtext0() }));
                });
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "wi-fi", "networks around you");
                widgets::hairline(ui);

                if widgets::toggle_row(ui, "Wi-Fi", "", st.on).clicked() {
                    let on = st.on;
                    self.state.lock().unwrap().on = !on;
                    background(&self.state, ctx, move |s| {
                        let ok = nmcli(8, &["radio", "wifi", if on { "off" } else { "on" }], None).0;
                        if !ok {
                            set_status(s, "Couldn't switch Wi-Fi", true);
                        }
                    });
                }
                if !st.loaded {
                    widgets::caption(ui, "looking for networks…");
                }

                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    widgets::hairline(ui);
                    if st.on {
                        let scan_label = if st.scanning { "Scanning…" } else { "Scan again" };
                        if widgets::row(ui, "⊙", scan_label, "").clicked() && !st.scanning {
                            self.state.lock().unwrap().scanning = true;
                            let (s, c) = (self.state.clone(), ctx.clone());
                            std::thread::spawn(move || {
                                refresh(&s, true);
                                s.lock().unwrap().scanning = false;
                                c.request_repaint();
                            });
                        }
                        widgets::caption(ui, "click to join · × forgets a saved network");
                    } else if st.loaded {
                        widgets::caption(ui, "turn Wi-Fi on to see networks");
                    }

                    for net in &st.nets {
                        let busy = st.busy.as_deref() == Some(net.ssid.as_str());
                        let mut value = bars(net.signal).to_string();
                        if busy {
                            value = format!("…  {value}");
                        } else if net.in_use {
                            value = format!("connected  {value}");
                        }
                        let icon = if net.secure { "◈" } else { "◇" };
                        let (clicked, forget) = if net.saved {
                            widgets::row_with_remove(ui, icon, &net.ssid, &value, "forget this network")
                        } else {
                            (widgets::row(ui, icon, &net.ssid, &value).clicked(), false)
                        };

                        if forget && st.busy.is_none() {
                            let ssid = net.ssid.clone();
                            self.state.lock().unwrap().busy = Some(ssid.clone());
                            background(&self.state, ctx, move |s| {
                                let ok = nmcli(8, &["connection", "delete", "id", &ssid], None).0;
                                set_status(s, if ok { format!("Forgot {ssid}") } else { format!("Couldn't forget {ssid}") }, !ok);
                            });
                        } else if clicked && st.busy.is_none() {
                            if net.in_use {
                                let ssid = net.ssid.clone();
                                self.state.lock().unwrap().busy = Some(ssid.clone());
                                background(&self.state, ctx, move |s| {
                                    let ok = nmcli(10, &["connection", "down", "id", &ssid], None).0;
                                    set_status(s, if ok { format!("Disconnected from {ssid}") } else { format!("Couldn't disconnect {ssid}") }, !ok);
                                });
                            } else if net.secure && !net.saved {
                                self.asking = Some(net.ssid.clone());
                                self.password.clear();
                            } else {
                                let n = net.clone();
                                self.join(ctx, &n, None);
                            }
                        }

                        // the password line, right under the network it's for
                        if self.asking.as_deref() == Some(net.ssid.as_str()) {
                            let mut submit = false;
                            ui.horizontal(|ui| {
                                ui.add_space(26.0);
                                let edit = ui.add(
                                    TextEdit::singleline(&mut self.password)
                                        .password(!self.show_password)
                                        .hint_text("password")
                                        .desired_width(ui.available_width() - 110.0),
                                );
                                edit.request_focus();
                                if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    submit = true;
                                }
                                if ui.button(if self.show_password { "hide" } else { "show" }).clicked() {
                                    self.show_password = !self.show_password;
                                }
                                if ui.button("join").clicked() {
                                    submit = true;
                                }
                            });
                            if submit && !self.password.is_empty() {
                                let pw = std::mem::take(&mut self.password);
                                self.asking = None;
                                let n = net.clone();
                                self.join(ctx, &n, Some(pw));
                            }
                        }
                    }
                });
            });

        if st.scanning || st.busy.is_some() {
            ctx.request_repaint_after(Duration::from_millis(300));
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura wi-fi")
            .with_app_id("aura-wifi")
            .with_inner_size([380.0, 520.0])
            .with_min_inner_size([320.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native("Aura wi-fi", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_fields_keep_escaped_colons() {
        assert_eq!(split_terse(r"*:Cafe\: Guest:80:WPA2"), vec!["*", "Cafe: Guest", "80", "WPA2"]);
    }

    #[test]
    fn list_merges_access_points_and_puts_current_first() {
        let list = ":Home:40:WPA2\n*:Office:60:WPA2\n:Home:90:WPA2\n:Open Cafe:70:\n::99:WPA2\n";
        let nets = parse_list(list, &["Home".to_string()]);
        assert_eq!(nets.iter().map(|n| n.ssid.as_str()).collect::<Vec<_>>(), ["Office", "Home", "Open Cafe"]);
        assert_eq!(nets[1].signal, 90);
        assert!(nets[1].saved && nets[1].secure);
        assert!(!nets[2].secure);
    }
}
