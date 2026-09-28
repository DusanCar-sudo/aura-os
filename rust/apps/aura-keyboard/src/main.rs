//! aura-keyboard — which languages you type in, and the key that switches them.
//!
//! Pick one or more layouts (search the full X11 list), choose the switch
//! key, try it in the test box, then Apply: it writes
//! ~/.config/sway/config.d/60-keyboard.conf and tells sway right away, so
//! it also survives the next login.

use std::env;
use std::fs;
use std::process::{Command, Stdio};

/// If SWAYSOCK points at a dead socket (sway crashed/restarted), find the
/// newest live sway-ipc.*.sock in XDG_RUNTIME_DIR and point SWAYSOCK at it.
fn fix_socket() {
    if let Ok(sock) = env::var("SWAYSOCK") {
        if fs::metadata(&sock).is_ok() { return; }
    }
    let dir = match env::var("XDG_RUNTIME_DIR") { Ok(d) => d, Err(_) => return };
    if let Ok(entries) = fs::read_dir(&dir) {
        let mut best: Option<(std::time::SystemTime, String)> = None;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("sway-ipc.") && name.ends_with(".sock") {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if best.as_ref().map_or(true, |(t, _)| mtime > *t) {
                            best = Some((mtime, entry.path().to_string_lossy().into_owned()));
                        }
                    }
                }
            }
        }
        if let Some((_, path)) = best { env::set_var("SWAYSOCK", &path); }
    }
}

use aura_ui::{theme, widgets};
use egui::{Frame, Key, Margin, RichText, ScrollArea, TextEdit};

const CONF: &str = "sway/config.d/60-keyboard.conf";

/// The switch shortcuts worth offering, as xkb option and plain words.
const SWITCHES: &[(&str, &str)] = &[
    ("grp:alt_shift_toggle", "Alt + Shift"),
    ("grp:win_space_toggle", "Super + Space"),
    ("grp:ctrl_shift_toggle", "Ctrl + Shift"),
    ("grp:caps_toggle", "Caps Lock"),
    ("", "no switch key"),
];

#[derive(Clone, PartialEq)]
struct Layout {
    /// "us" or "rs(latin)"
    code: String,
    label: String,
}

fn config_dir() -> std::path::PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"))
}

/// Every layout and variant X11 knows, as picker rows.
fn all_layouts() -> Vec<Layout> {
    let text = std::fs::read_to_string("/usr/share/X11/xkb/rules/evdev.lst").unwrap_or_default();
    let mut out = Vec::new();
    let mut section = "";
    for line in text.lines() {
        if line.starts_with("! ") {
            section = line[2..].trim();
            continue;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((code, label)) = line.split_once(char::is_whitespace) else { continue };
        let label = label.trim();
        match section {
            "layout" => out.push(Layout { code: code.to_string(), label: label.to_string() }),
            "variant" => {
                // "latin           rs: Serbian (Latin)"
                if let Some((base, human)) = label.split_once(':') {
                    out.push(Layout {
                        code: format!("{}({})", base.trim(), code),
                        label: human.trim().to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// What sway is using right now (from the config file we write).
fn current() -> (Vec<Layout>, String) {
    let text = std::fs::read_to_string(config_dir().join(CONF)).unwrap_or_default();
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(k))
            .map(|v| v.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let layouts = field("xkb_layout");
    let variants = field("xkb_variant");
    let options = field("xkb_options");
    let all = all_layouts();
    let mut picked = Vec::new();
    for (i, code) in layouts.split(',').filter(|s| !s.is_empty()).enumerate() {
        let variant = variants.split(',').nth(i).unwrap_or("").trim();
        let full = if variant.is_empty() { code.trim().to_string() } else { format!("{}({})", code.trim(), variant) };
        let label = all.iter().find(|l| l.code == full).map(|l| l.label.clone()).unwrap_or_else(|| full.clone());
        picked.push(Layout { code: full, label });
    }
    if picked.is_empty() {
        picked.push(Layout { code: "us".into(), label: "English (US)".into() });
    }
    (picked, options)
}

fn write_config(picked: &[Layout], option: &str) -> std::io::Result<()> {
    let mut layouts = Vec::new();
    let mut variants = Vec::new();
    for l in picked {
        match l.code.split_once('(') {
            Some((base, v)) => {
                layouts.push(base.to_string());
                variants.push(v.trim_end_matches(')').to_string());
            }
            None => {
                layouts.push(l.code.clone());
                variants.push(String::new());
            }
        }
    }
    let body = format!(
        "# written by aura-keyboard (Settings → Keyboard)\n\
         # layouts: {}\n\
         input type:keyboard {{\n    xkb_layout {}\n    xkb_variant \"{}\"\n    xkb_options {}\n}}\n",
        picked.iter().map(|l| l.label.as_str()).collect::<Vec<_>>().join(", "),
        layouts.join(","),
        variants.join(","),
        option
    );
    let path = config_dir().join(CONF);
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&path, body)?;
    fix_socket();
    let _ = Command::new("swaymsg")
        .arg("reload")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

struct App {
    all: Vec<Layout>,
    picked: Vec<Layout>,
    option: String,
    search: String,
    test: String,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    status: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        let (picked, option) = current();
        Self {
            all: all_layouts(),
            picked,
            option,
            search: String::new(),
            test: String::new(),
            theme_id,
            next_theme_check: 0.0,
            status: String::new(),
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

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "keyboard", "languages you type in");
                widgets::hairline(ui);

                widgets::caption(ui, "your layouts  ·  the first one is the default");
                let mut remove = None;
                let mut move_up = None;
                for (i, l) in self.picked.clone().iter().enumerate() {
                    let (clicked, x) = widgets::row_with_remove(ui, if i == 0 { "●" } else { "○" }, &l.label, &l.code, "remove this layout");
                    if x && self.picked.len() > 1 {
                        remove = Some(i);
                    } else if clicked && i > 0 {
                        move_up = Some(i);
                    }
                }
                if let Some(i) = remove {
                    self.picked.remove(i);
                }
                if let Some(i) = move_up {
                    self.picked.swap(0, i); // clicking one makes it the default
                }

                widgets::hairline(ui);
                widgets::caption(ui, "switch with");
                ui.horizontal_wrapped(|ui| {
                    for (opt, label) in SWITCHES {
                        if ui.selectable_label(self.option == *opt, *label).clicked() {
                            self.option = opt.to_string();
                        }
                    }
                });

                widgets::hairline(ui);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        self.status = match write_config(&self.picked, &self.option) {
                            Ok(()) => "applied — and kept for the next login".into(),
                            Err(e) => format!("couldn't write the config: {e}"),
                        };
                    }
                    ui.label(RichText::new("try it:").color(theme::overlay0()));
                    ui.add(TextEdit::singleline(&mut self.test).hint_text("type here").desired_width(ui.available_width()));
                });
                if !self.status.is_empty() {
                    ui.label(RichText::new(&self.status).color(theme::subtext0()));
                }

                widgets::hairline(ui);
                widgets::caption(ui, "add a layout");
                ui.add(TextEdit::singleline(&mut self.search).hint_text("search: serbian, german, cyrillic…").desired_width(f32::INFINITY));
                let needle = self.search.to_lowercase();
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if needle.len() < 2 {
                        widgets::caption(ui, "type at least two letters");
                        return;
                    }
                    let mut shown = 0;
                    for l in self.all.clone() {
                        if shown >= 40 {
                            break;
                        }
                        if !l.label.to_lowercase().contains(&needle) && !l.code.to_lowercase().contains(&needle) {
                            continue;
                        }
                        if self.picked.iter().any(|p| p.code == l.code) {
                            continue;
                        }
                        shown += 1;
                        if widgets::row(ui, "+", &l.label, &l.code).clicked() {
                            self.picked.push(l.clone());
                        }
                    }
                    if shown == 0 {
                        widgets::caption(ui, "nothing matches");
                    }
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura keyboard")
            .with_app_id("aura-keyboard")
            .with_inner_size([420.0, 560.0])
            .with_min_inner_size([360.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native("Aura keyboard", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
