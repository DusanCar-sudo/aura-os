//! aura-rclick — the Aura right-click menu.
//!
//! Right-click the bare desktop: your own apps first (Zed, Code, whatever
//! you pinned), then the Aura tools, whatever text is selected, and the
//! desktop's own settings. Right-click a window's border, or its title in
//! the top bar: what you can do to that window. Every row shows its
//! keyboard shortcut, so the menu teaches them while it helps.
//!
//! When a new app is installed, a small window asks whether it belongs in
//! the menu (a systemd path unit runs `aura-rclick scan`).
//!
//!   aura-rclick desktop [--at X,Y]   the desktop menu (from aura-desk-catch)
//!   aura-rclick window  [--at X,Y]   the focused window's menu
//!   aura-rclick edit                 choose what the desktop menu holds
//!   aura-rclick ask <id>…            "new app installed: add it here?"
//!   aura-rclick scan [--seed]        look for new apps (--seed: remember, ask nothing)
//!   aura-rclick pin|unpin <id>…      add or remove apps from a terminal
//!   aura-rclick list                 what is pinned

mod apps;
mod sway;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use apps::{App, Pin};
use aura_ui::widgets::ROW_H;
use aura_ui::{icons, theme, widgets};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, FontId, Frame, Key, Margin, Rect, Response, RichText, ScrollArea,
    Sense, Stroke, StrokeKind, TextStyle, Ui,
};

const MENU_W: f32 = 292.0;
const USAGE: &str = "usage: aura-rclick desktop|window [--at X,Y] | edit | ask <id>… | scan [--seed] | pin|unpin <id>… | list";

fn main() {
    // sway and waybar start us without ~/.local/bin, where Aura's tools live
    let mut path = std::env::var("PATH").unwrap_or_default();
    for extra in [apps::home().join(".local/bin"), PathBuf::from("/usr/local/bin")] {
        if !path.split(':').any(|p| Path::new(p) == extra) {
            path = format!("{}:{path}", extra.display());
        }
    }
    std::env::set_var("PATH", path);

    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let at = args.iter().position(|a| a == "--at").and_then(|i| args.get(i + 1)).and_then(|v| {
        let (x, y) = v.split_once(',')?;
        Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
    });
    let code = match args.first().map(String::as_str).unwrap_or("desktop") {
        "desktop" => menu(false, at),
        "window" => menu(true, at),
        "edit" => edit(),
        "ask" => ask(rest),
        "scan" => scan(rest.iter().any(|a| a == "--seed")),
        "pin" | "unpin" => pin_cli(args[0] == "pin", rest),
        "list" => {
            for p in apps::read_pins() {
                match p {
                    Pin::App(id) => println!("{id}\t{}", apps::find(&id).map(|a| a.name).unwrap_or_else(|| "(not installed)".into())),
                    Pin::Cmd { label, cmd } => println!("{label}\t{cmd}"),
                }
            }
            0
        }
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            0
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

fn self_exe() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "aura-rclick".into())
}

fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "Aura", "-t", "2500", title, body])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn install_theme(ctx: &egui::Context) {
    theme::install_fonts(ctx);
    theme::apply(ctx, &theme::ThemeId::detect());
}

// ── icons ─────────────────────────────────────────────────────────────────

enum Loaded {
    Svg(String, egui::load::Bytes),
    Tex(egui::TextureHandle),
    Missing,
}

/// App icons, read once each: SVGs through egui's loader, PNGs decoded
/// here (small), and a letter tile when an app has no icon at all.
#[derive(Default)]
struct IconCache {
    map: HashMap<String, Loaded>,
}

impl IconCache {
    fn paint(&mut self, ui: &Ui, icon: &str, name: &str, rect: Rect) {
        let loaded = self.map.entry(icon.to_string()).or_insert_with(|| load_icon(ui.ctx(), icon));
        match loaded {
            Loaded::Svg(uri, bytes) => {
                egui::Image::from_bytes(uri.clone(), bytes.clone()).paint_at(ui, rect);
            }
            Loaded::Tex(t) => {
                egui::Image::new(egui::load::SizedTexture::from_handle(t)).paint_at(ui, rect);
            }
            Loaded::Missing => letter_tile(ui, rect, name),
        }
    }
}

fn load_icon(ctx: &egui::Context, icon: &str) -> Loaded {
    let Some(path) = apps::icon_file(icon) else { return Loaded::Missing };
    let Ok(bytes) = fs::read(&path) else { return Loaded::Missing };
    if path.extension().is_some_and(|e| e == "svg") {
        return Loaded::Svg(format!("bytes://app-icon{}", path.display()), egui::load::Bytes::Shared(bytes.into()));
    }
    let Ok(img) = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) else { return Loaded::Missing };
    let mut img = img.into_rgba8();
    if img.width() > 64 {
        let h = (64 * img.height() / img.width()).max(1);
        img = image::imageops::resize(&img, 64, h, image::imageops::FilterType::Triangle);
    }
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Loaded::Tex(ctx.load_texture(format!("app-icon:{icon}"), color, egui::TextureOptions::LINEAR))
}

fn letter_tile(ui: &Ui, rect: Rect, name: &str) {
    let p = ui.painter();
    p.rect_filled(rect.shrink(1.0), CornerRadius::same(4), theme::dim(theme::accent(), 70));
    let letter: String = name.chars().next().unwrap_or('?').to_uppercase().collect();
    p.text(rect.center(), Align2::CENTER_CENTER, letter, FontId::new(11.0, theme::bold()), theme::text());
}

/// Text that fits `max_w`, with an ellipsis when it had to be cut.
fn fit_text(ui: &Ui, text: &str, font: &FontId, max_w: f32) -> String {
    let width = |s: &str| ui.fonts(|f| f.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x);
    if width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for n in (1..chars.len()).rev() {
        let s: String = chars[..n].iter().collect::<String>().trim_end().to_string() + "…";
        if width(&s) <= max_w {
            return s;
        }
    }
    "…".into()
}

// ── the menu ──────────────────────────────────────────────────────────────

#[derive(Clone)]
enum Glyph {
    /// One of aura-ui's own icons.
    Pack(&'static str),
    /// An app's icon (Icon= of its desktop file), its name for the fallback tile.
    App { icon: String, name: String },
    Text(&'static str),
}

#[derive(Clone)]
enum Act {
    Launch(App),
    Shell(String),
    Sway(String),
    Pin(App),
    Unpin(App),
}

#[derive(Clone)]
struct Item {
    glyph: Glyph,
    label: String,
    hint: String,
    act: Act,
    danger: bool,
}

fn item(glyph: Glyph, label: &str, hint: &str, act: Act) -> Entry {
    Entry::Item(Item { glyph, label: label.into(), hint: hint.into(), act, danger: false })
}

enum Entry {
    Caption(String),
    Line,
    Item(Item),
    /// Desk chips 1–9 that move this window there.
    Desks { con: i64, current: Option<i64> },
}

fn run(act: &Act) {
    match act {
        Act::Launch(app) => apps::launch(app),
        Act::Shell(cmd) => apps::spawn(cmd),
        Act::Sway(cmd) => {
            sway::run(cmd);
        }
        Act::Pin(app) => {
            apps::pin(&app.id);
            notify("Right-click menu", &format!("{} added. Right-click the desktop to open it.", app.name));
        }
        Act::Unpin(app) => {
            apps::unpin(&app.id);
            notify("Right-click menu", &format!("{} removed.", app.name));
        }
    }
}

/// The primary selection (whatever text is highlighted), if any. Read in
/// the background while the menu builds; a slow clipboard never holds it up.
fn selection() -> std::sync::mpsc::Receiver<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = Command::new("timeout")
            .args(["0.4", "wl-paste", "--primary", "--no-newline", "--type", "text"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if let Ok(o) = out {
            let _ = tx.send(String::from_utf8_lossy(&o.stdout).into_owned());
        }
    });
    rx
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix('~') {
        Some(rest) => apps::home().join(rest.trim_start_matches('/')),
        None => PathBuf::from(p),
    }
}

fn short_dir(p: &Path) -> String {
    let home = apps::home();
    let shown = match p.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    };
    let parts: Vec<&str> = shown.split('/').collect();
    if parts.len() > 3 {
        format!("…/{}", parts[parts.len() - 2..].join("/"))
    } else {
        shown
    }
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn selection_entry(sel: &str) -> Option<Entry> {
    let s = sel.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() || s.chars().count() > 400 {
        return None;
    }
    let short: String = if s.chars().count() > 22 { s.chars().take(21).collect::<String>() + "…" } else { s.clone() };
    if !s.contains(' ') && (s.starts_with("http://") || s.starts_with("https://") || s.starts_with("www.")) {
        let url = if s.starts_with("www.") { format!("https://{s}") } else { s.clone() };
        let host = url.split('/').nth(2).unwrap_or("").to_string();
        return Some(item(Glyph::Pack("network"), "Open link", &host, Act::Shell(format!("xdg-open {}", apps::quote(&url)))));
    }
    let path = expand(&s);
    if (s.starts_with('/') || s.starts_with('~')) && path.exists() {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| s.clone());
        let glyph = Glyph::Pack(if path.is_dir() { "files" } else { "file-plain" });
        return Some(item(glyph, &format!("Open {name}"), "", Act::Shell(format!("xdg-open {}", apps::quote(&path.display().to_string())))));
    }
    let url = format!("https://www.google.com/search?q={}", url_encode(&s));
    Some(item(Glyph::Pack("search"), &format!("Search “{short}”"), "web", Act::Shell(format!("xdg-open {}", apps::quote(&url)))))
}

fn desktop_entries(selected: Option<String>) -> Vec<Entry> {
    let mut e = vec![Entry::Caption("Your apps".into())];
    let edit = Act::Shell(format!("{} edit", apps::quote(&self_exe())));
    let mut any = false;
    for p in apps::read_pins() {
        match p {
            Pin::App(id) => {
                if let Some(app) = apps::find(&id) {
                    let glyph = Glyph::App { icon: app.icon.clone(), name: app.name.clone() };
                    e.push(item(glyph, &app.name.clone(), "", Act::Launch(app)));
                    any = true;
                }
            }
            Pin::Cmd { label, cmd } => {
                e.push(item(Glyph::Pack("file-exec"), &label, "", Act::Shell(cmd)));
                any = true;
            }
        }
    }
    if !any {
        e.push(item(Glyph::Pack("package"), "Add your apps here…", "", edit.clone()));
    }
    if let Some(sel) = selected.as_deref().and_then(selection_entry) {
        e.push(Entry::Line);
        e.push(sel);
    }
    e.push(Entry::Line);
    e.push(item(Glyph::Pack("terminal"), "Terminal", "Super+Enter", Act::Shell("aura-term".into())));
    let files = apps::find("aura-files").map(Act::Launch).unwrap_or_else(|| Act::Shell("aura-term --app-id=aura-files -e aura-files".into()));
    e.push(item(Glyph::Pack("files"), "Files", "", files));
    e.push(item(Glyph::Pack("aura"), "Ask Aura", "Super+A", Act::Shell("aura-os-prompt".into())));
    e.push(item(Glyph::Pack("history"), "Recent…", "", Act::Shell("aura-os-recent pick".into())));
    e.push(Entry::Line);
    // a moment for the menu to leave the screen before the crosshair comes
    e.push(item(Glyph::Pack("file-image"), "Screenshot an area", "Shift+Print", Act::Shell("sleep 0.3; aura-os-shot region".into())));
    e.push(item(Glyph::Pack("media"), "Wallpaper…", "", Act::Shell("aura-os-wallpaper pick".into())));
    e.push(item(Glyph::Pack("settings"), "Settings", "Super+,", Act::Shell("aura-os-settings".into())));
    e.push(item(Glyph::Pack("package"), "Edit this menu…", "", edit));
    e.push(item(Glyph::Pack("system"), "Power…", "Super+Esc", Act::Shell("aura-os-power".into())));
    e
}

fn children(pid: i32) -> Vec<i32> {
    fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// The folder a window is "in". A terminal says it in its title
/// ("you@aura:~/Projects"); anything else, its process or its children
/// (an editor's language servers run in the project). One process behind
/// many windows (the terminal server) tells us nothing, so we don't guess.
fn window_dir(w: &sway::Win, shared_pid: bool) -> Option<PathBuf> {
    let home = apps::home();
    let usable = |p: PathBuf| (p.is_dir() && p != home && p != Path::new("/")).then_some(p);
    let t = w.title.trim();
    let from_title = t
        .rsplit_once(':')
        .map(|(_, p)| p.trim())
        .filter(|p| p.starts_with('~') || p.starts_with('/'))
        .or_else(|| (t.starts_with('~') || t.starts_with('/')).then_some(t));
    if let Some(p) = from_title {
        return usable(expand(p));
    }
    if shared_pid {
        return None;
    }
    std::iter::once(w.pid)
        .chain(children(w.pid))
        .filter_map(|pid| fs::read_link(format!("/proc/{pid}/cwd")).ok())
        .find_map(usable)
}

fn pretty(app_id: &str) -> String {
    let last = app_id.rsplit('.').next().unwrap_or(app_id);
    let mut c = last.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_else(|| "Window".into())
}

fn window_entries(tree: &serde_json::Value, w: &sway::Win) -> Vec<Entry> {
    let app = apps::for_window(&w.app_id);
    let name = app.as_ref().map(|a| a.name.clone()).unwrap_or_else(|| pretty(&w.app_id));
    let on = |cmd: &str| format!("[con_id={}] {cmd}", w.id);
    let mut e = vec![Entry::Caption(name)];
    e.push(item(Glyph::Text("▁"), "Minimize", "Super+Z", Act::Shell(format!("swaymsg -q {} && aura-os-minimize", apps::quote(&on("focus"))))));
    e.push(item(Glyph::Text("◫"), if w.floating { "Tile" } else { "Float" }, "Super+V", Act::Sway(on("floating toggle"))));
    e.push(item(Glyph::Text("□"), if w.fullscreen { "Leave fullscreen" } else { "Fullscreen" }, "Super+F", Act::Sway(on("fullscreen toggle"))));
    let outs = sway::outputs();
    if outs.len() > 1 {
        let i = outs.iter().position(|o| o.name == w.output).unwrap_or(0);
        let next = &outs[(i + 1) % outs.len()];
        let label = if next.name.starts_with("eDP") { "Move to laptop screen" } else { "Move to other screen" };
        e.push(item(Glyph::Text("⇥"), label, "Super+/", Act::Sway(on(&format!("move container to output {}", next.name)))));
    }
    e.push(Entry::Caption("Move to desk".into()));
    e.push(Entry::Desks { con: w.id, current: w.desk });
    e.push(Entry::Line);
    let shared = sway_windows_with_pid(tree, w.pid) > 1;
    if let Some(dir) = window_dir(w, shared) {
        let d = apps::quote(&dir.display().to_string());
        e.push(item(Glyph::Pack("terminal"), "Terminal here", &short_dir(&dir), Act::Shell(format!("aura-term --working-directory={d}"))));
        e.push(item(Glyph::Pack("files"), "Files here", "", Act::Shell(format!("aura-term --app-id=aura-files -e aura-files {d}"))));
    }
    if let Some(app) = app.filter(|a| a.listed) {
        if apps::is_pinned(&app.id) {
            e.push(item(Glyph::Pack("package"), "Remove from right-click", "", Act::Unpin(app)));
        } else {
            e.push(item(Glyph::Pack("package"), "Add to right-click", "", Act::Pin(app)));
        }
    }
    e.push(Entry::Line);
    e.push(Entry::Item(Item { glyph: Glyph::Text("×"), label: "Close".into(), hint: "Super+Q".into(), act: Act::Sway(on("kill")), danger: true }));
    e
}

fn sway_windows_with_pid(tree: &serde_json::Value, pid: i32) -> usize {
    fn count(n: &serde_json::Value, pid: i32) -> usize {
        let me = usize::from(n["pid"].as_i64() == Some(pid as i64) && n["nodes"].as_array().is_none_or(|a| a.is_empty()));
        me + ["nodes", "floating_nodes"].iter().flat_map(|k| n[*k].as_array().into_iter().flatten()).map(|k| count(k, pid)).sum::<usize>()
    }
    count(tree, pid)
}

fn item_row(ui: &mut Ui, icons: &mut IconCache, it: &Item, lit: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    if lit || resp.hovered() {
        ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let (fg, icon_fg) = if it.danger { (theme::bad(), theme::bad()) } else { (theme::text(), theme::accent()) };
    let font = TextStyle::Body.resolve(ui.style());
    let icon_rect = Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(16.0, 16.0));
    match &it.glyph {
        Glyph::Pack(name) => icons::paint(ui, name, icon_rect, icon_fg),
        Glyph::App { icon, name } => icons.paint(ui, icon, name, icon_rect.expand(1.0)),
        Glyph::Text(t) => {
            ui.painter().text(icon_rect.center(), Align2::CENTER_CENTER, *t, font.clone(), icon_fg);
        }
    }
    let mut right = rect.right() - 8.0;
    if !it.hint.is_empty() {
        let hint_font = FontId::new(font.size - 1.5, font.family.clone());
        let g = ui.painter().layout_no_wrap(it.hint.clone(), hint_font, theme::overlay0());
        ui.painter().galley(pos2(right - g.size().x, rect.center().y - g.size().y / 2.0), g.clone(), theme::overlay0());
        right -= g.size().x + 10.0;
    }
    let left = rect.left() + 32.0;
    let label = fit_text(ui, &it.label, &font, right - left);
    ui.painter().text(pos2(left, rect.center().y), Align2::LEFT_CENTER, label, font, fg);
    resp
}

fn desk_row(ui: &mut Ui, current: Option<i64>) -> Option<i64> {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::hover());
    let gap = 3.0;
    let cw = (w - 8.0 * gap) / 9.0;
    let font = TextStyle::Body.resolve(ui.style());
    let mut picked = None;
    for i in 0..9i64 {
        let r = Rect::from_min_size(pos2(rect.left() + i as f32 * (cw + gap), rect.top()), vec2(cw, ROW_H));
        let resp = ui.interact(r, ui.id().with(("desk", i)), Sense::click());
        let here = current == Some(i + 1);
        let p = ui.painter();
        p.rect_filled(r, theme::radius(), if resp.hovered() && !here { theme::surface1() } else { theme::surface0() });
        if here {
            p.rect_stroke(r, theme::radius(), Stroke::new(1.0_f32, theme::accent()), StrokeKind::Inside);
        }
        p.text(r.center(), Align2::CENTER_CENTER, (i + 1).to_string(), font.clone(), if here { theme::accent() } else { theme::text() });
        if resp.hovered() && !here {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() && !here {
            picked = Some(i + 1);
        }
    }
    picked
}

/// Where the menu goes: the size it measured, and whether it has been
/// moved there (until then it draws nothing, so it never jumps).
#[derive(Default)]
struct Place {
    size: Option<(i32, i32)>,
    /// Where the pointer first entered us, relative to our window.
    pointer: Option<(i32, i32)>,
    placed: bool,
}

/// Next to the cursor, flipped to the other side near an edge, never
/// under a bar.
fn fit(at: i32, len: i32, start: i32, span: i32) -> i32 {
    let mut p = at + 2;
    if p + len > start + span {
        p = at - len - 2;
    }
    p.min(start + span - len).max(start)
}

/// sway puts the new window centred on the cursor (see 85-rightclick.conf),
/// so its centre *is* the cursor when nobody told us where the click was.
fn place_window(at: Option<(i32, i32)>, place: Arc<Mutex<Place>>, ctx: egui::Context) {
    let finish = |place: &Arc<Mutex<Place>>| {
        place.lock().unwrap().placed = true;
        ctx.request_repaint();
    };
    let pid = std::process::id() as i32;
    let mut win = None;
    for _ in 0..100 {
        win = sway::get("get_tree").and_then(|t| sway::window_of_pid(&t, pid));
        if win.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let Some(win) = win else { return finish(&place) };
    let (mut size, mut pointer) = (None, None);
    for i in 0..200 {
        (size, pointer) = {
            let p = place.lock().unwrap();
            (p.size, p.pointer)
        };
        // the pointer arrives with the first frames, if it is over us at all
        if size.is_some() && (pointer.is_some() || at.is_some() || i > 15) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let Some((w, h)) = size else { return finish(&place) };
    // sway keeps the starting window on screen, so near an edge its centre
    // is not the cursor; where the pointer entered us is
    let anchor = at
        .or(pointer.map(|(px, py)| (win.rect.x + px, win.rect.y + py)))
        .unwrap_or(win.rect.center());
    let outs = sway::outputs();
    // screens can overlap in sway's layout; then the one we opened on wins
    let area = outs
        .iter()
        .filter(|o| o.rect.contains(anchor))
        .min_by_key(|o| o.name != win.output)
        .or_else(|| outs.iter().find(|o| o.name == win.output))
        .map(|o| o.usable)
        .unwrap_or(sway::Rect { x: 0, y: 0, w: 1 << 15, h: 1 << 15 });
    let (x, y) = (fit(anchor.0, w, area.x, area.w), fit(anchor.1, h, area.y, area.h));
    sway::run(&format!("[con_id={}] resize set {w} {h}, move absolute position {x} {y}", win.id));
    finish(&place)
}

struct Menu {
    entries: Vec<Entry>,
    icons: IconCache,
    sel: Option<usize>,
    had_focus: bool,
    place: Arc<Mutex<Place>>,
    measured: egui::Vec2,
    started: Instant,
}

impl Menu {
    fn items(&self) -> usize {
        self.entries.iter().filter(|e| matches!(e, Entry::Item(_))).count()
    }

    fn nth(&self, k: usize) -> Option<&Item> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                Entry::Item(i) => Some(i),
                _ => None,
            })
            .nth(k)
    }

    fn desk_con(&self) -> Option<i64> {
        self.entries.iter().find_map(|e| match e {
            Entry::Desks { con, .. } => Some(*con),
            _ => None,
        })
    }

    /// Visible once moved into place and resized (or after a grace period,
    /// if sway would not say where we are).
    fn visible(&self, ctx: &egui::Context) -> bool {
        let placed = self.place.lock().unwrap().placed;
        let fits = (ctx.screen_rect().size() - self.measured).abs().max_elem() < 2.0;
        (placed && fits) || self.started.elapsed() > Duration::from_millis(900)
    }
}

impl eframe::App for Menu {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let visible = self.visible(ctx);
        let focused = ctx.input(|i| i.viewport().focused);
        if focused == Some(true) {
            self.had_focus = true;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) || (visible && self.had_focus && focused == Some(false)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let n = self.items();
        let mut chosen: Option<Act> = None;
        ctx.input(|i| {
            if n > 0 && i.key_pressed(Key::ArrowDown) {
                self.sel = Some(self.sel.map_or(0, |s| (s + 1) % n));
            }
            if n > 0 && i.key_pressed(Key::ArrowUp) {
                self.sel = Some(self.sel.map_or(n - 1, |s| (s + n - 1) % n));
            }
        });
        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            chosen = self.sel.and_then(|k| self.nth(k)).map(|i| i.act.clone());
        }
        if let Some(con) = self.desk_con() {
            const DIGITS: [Key; 9] = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
            if let Some(d) = DIGITS.iter().position(|k| ctx.input(|i| i.key_pressed(*k))) {
                chosen = Some(Act::Sway(format!("[con_id={con}] move container to workspace number {}", d + 1)));
            }
        }
        let moved = ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO);

        let Menu { entries, icons, sel, .. } = self;
        let mut hovered = None;
        let frame = Frame::new()
            .fill(theme::base())
            .stroke(Stroke::new(1.0_f32, theme::accent()))
            .corner_radius(theme::radius())
            .inner_margin(Margin::same(6));
        let shown = egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            if !visible {
                ui.set_invisible();
            }
            frame.show(ui, |ui| {
                ui.set_width(MENU_W - 14.0);
                ui.spacing_mut().item_spacing.y = 1.0;
                let mut k = 0;
                for entry in entries.iter() {
                    match entry {
                        Entry::Caption(t) => {
                            ui.add_space(3.0);
                            let font = FontId::new(11.0, TextStyle::Body.resolve(ui.style()).family);
                            let t = fit_text(ui, t, &font, MENU_W - 30.0);
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label(RichText::new(t).font(font).color(theme::overlay0()));
                            });
                            ui.add_space(2.0);
                        }
                        Entry::Line => widgets::hairline(ui),
                        Entry::Item(it) => {
                            let r = item_row(ui, icons, it, *sel == Some(k));
                            if r.hovered() {
                                hovered = Some(k);
                            }
                            if r.clicked() {
                                chosen = Some(it.act.clone());
                            }
                            k += 1;
                        }
                        Entry::Desks { con, current } => {
                            if let Some(d) = desk_row(ui, *current) {
                                chosen = Some(Act::Sway(format!("[con_id={con}] move container to workspace number {d}")));
                            }
                        }
                    }
                }
            })
            .response
            .rect
            .size()
        });
        if hovered.is_some() && moved {
            *sel = hovered;
        }

        self.measured = shown.inner;
        let z = ctx.zoom_factor();
        let logical = ((self.measured.x * z).ceil() as i32, (self.measured.y * z).ceil() as i32);
        {
            let mut p = self.place.lock().unwrap();
            if p.size != Some(logical) {
                p.size = Some(logical);
            }
            if p.pointer.is_none() && !p.placed {
                p.pointer = ctx.input(|i| i.pointer.latest_pos()).map(|q| ((q.x * z) as i32, (q.y * z) as i32));
            }
        }
        if !visible {
            ctx.request_repaint_after(Duration::from_millis(30));
        }

        if let Some(act) = chosen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            run(&act);
        }
    }
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
}

fn is_rclick(pid: i32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == "aura-rclick")
}

/// One menu at a time: a new right-click replaces the open menu. The pid
/// file also lets aura-desk-catch close the menu on a desktop click.
fn claim_menu() -> PathBuf {
    let file = runtime_dir().join("aura-rclick.pid");
    let me = std::process::id() as i32;
    if let Ok(pid) = fs::read_to_string(&file).unwrap_or_default().trim().parse::<i32>() {
        if pid != me && is_rclick(pid) {
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
    let _ = fs::write(&file, format!("{me}\n"));
    file
}

fn menu(window: bool, at: Option<(i32, i32)>) -> i32 {
    let pidfile = claim_menu();
    let sel = (!window).then(selection);
    let tree = sway::get("get_tree").unwrap_or_default();
    // read before our own window opens and takes the focus
    let target = window.then(|| sway::focused_window(&tree)).flatten();
    let entries = match &target {
        Some(w) => window_entries(&tree, w),
        None => desktop_entries(sel.and_then(|rx| rx.recv_timeout(Duration::from_millis(450)).ok())),
    };
    let place = Arc::new(Mutex::new(Place::default()));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura menu")
            .with_app_id("aura-rclick")
            // sway's smallest floating window; it is centred on the cursor
            .with_inner_size([76.0, 52.0])
            .with_decorations(false)
            .with_transparent(true),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Aura menu",
        options,
        Box::new(move |cc| {
            install_theme(&cc.egui_ctx);
            let (p, ctx) = (place.clone(), cc.egui_ctx.clone());
            std::thread::spawn(move || place_window(at, p, ctx));
            Ok(Box::new(Menu {
                entries,
                icons: IconCache::default(),
                sel: None,
                had_focus: false,
                place,
                measured: egui::Vec2::ZERO,
                started: Instant::now(),
            }))
        }),
    );
    if fs::read_to_string(&pidfile).is_ok_and(|t| t.trim() == std::process::id().to_string()) {
        let _ = fs::remove_file(&pidfile);
    }
    i32::from(result.is_err())
}

// ── edit: choose the apps ───────────────────────────────────────────────────

enum PinOp {
    Up,
    Down,
    Remove,
}

fn icon_and_name(p: &Pin, all: &[App]) -> (String, String) {
    match p {
        Pin::App(id) => all
            .iter()
            .find(|a| &a.id == id)
            .cloned()
            .or_else(|| apps::find(id))
            .map(|a| (a.icon, a.name))
            .unwrap_or_else(|| (String::new(), format!("{id} (not installed)"))),
        Pin::Cmd { label, .. } => (String::new(), label.clone()),
    }
}

fn pin_row(ui: &mut Ui, icons: &mut IconCache, icon: &str, name: &str, i: usize, n: usize) -> Option<PinOp> {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    if resp.hovered() {
        ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
    }
    let icon_rect = Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(18.0, 18.0));
    icons.paint(ui, icon, name, icon_rect);
    let font = TextStyle::Body.resolve(ui.style());
    let label = fit_text(ui, name, &font, rect.width() - 32.0 - 90.0);
    ui.painter().text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, label, font.clone(), theme::text());
    let mut op = None;
    let buttons: [(&str, bool, Color32, PinOp); 3] = [
        ("×", true, theme::bad(), PinOp::Remove),
        ("↓", i + 1 < n, theme::text(), PinOp::Down),
        ("↑", i > 0, theme::text(), PinOp::Up),
    ];
    for (k, (glyph, enabled, color, what)) in buttons.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.right() - 6.0 - (k as f32 + 1.0) * 26.0, rect.top() + 2.0), vec2(24.0, ROW_H - 4.0));
        let b = ui.interact(r, ui.id().with(("pin", i, k)), Sense::click());
        if enabled && b.hovered() {
            ui.painter().rect_filled(r, theme::radius(), theme::surface1());
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let c = if enabled { color } else { theme::dim(theme::overlay0(), 90) };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, font.clone(), c);
        if enabled && b.clicked() {
            op = Some(what);
        }
    }
    op
}

fn add_row(ui: &mut Ui, icons: &mut IconCache, app: &App, lit: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    if lit || resp.hovered() {
        ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons.paint(ui, &app.icon, &app.name, Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(18.0, 18.0)));
    let font = TextStyle::Body.resolve(ui.style());
    let label = fit_text(ui, &app.name, &font, rect.width() - 32.0 - 60.0);
    ui.painter().text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, label, font.clone(), theme::text());
    if lit || resp.hovered() {
        ui.painter().text(pos2(rect.right() - 8.0, rect.center().y), Align2::RIGHT_CENTER, "+ add", font, theme::accent());
    }
    resp
}

struct Editor {
    pins: Vec<Pin>,
    all: Vec<App>,
    query: String,
    asks: bool,
    icons: IconCache,
    focused_once: bool,
    sel: usize,
}

impl eframe::App for Editor {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let mut changed = false;
        egui::CentralPanel::default().frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12))).show(ctx, |ui| {
            widgets::header(ui, "Right-click menu", "your apps, one right-click away");
            widgets::hairline(ui);
            widgets::caption(ui, "In the menu, top to bottom");
            if self.pins.is_empty() {
                ui.label(RichText::new("Nothing yet. Pick apps below.").color(theme::overlay0()));
            }
            let mut op = None;
            let n = self.pins.len();
            for (i, p) in self.pins.iter().enumerate() {
                let (icon, name) = icon_and_name(p, &self.all);
                if let Some(o) = pin_row(ui, &mut self.icons, &icon, &name, i, n) {
                    op = Some((i, o));
                }
            }
            match op {
                Some((i, PinOp::Up)) => self.pins.swap(i, i - 1),
                Some((i, PinOp::Down)) => self.pins.swap(i, i + 1),
                Some((i, PinOp::Remove)) => {
                    self.pins.remove(i);
                }
                None => {}
            }
            changed |= op.is_some();
            ui.add_space(6.0);
            if widgets::toggle_row(ui, "Ask when a new app is installed", "", self.asks).clicked() {
                self.asks = !self.asks;
                apps::set_asks(self.asks);
            }
            widgets::hairline(ui);
            widgets::caption(ui, "Add an app");
            let search = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text("search apps…").desired_width(f32::INFINITY));
            if !self.focused_once {
                search.request_focus();
                self.focused_once = true;
            }
            if search.changed() {
                self.sel = 0;
            }
            ui.add_space(4.0);
            let q = self.query.to_lowercase();
            let pinned: BTreeSet<&str> = self
                .pins
                .iter()
                .filter_map(|p| match p {
                    Pin::App(id) => Some(id.as_str()),
                    _ => None,
                })
                .collect();
            let list: Vec<&App> = self
                .all
                .iter()
                .filter(|a| !pinned.contains(a.id.as_str()))
                .filter(|a| q.is_empty() || [&a.name, &a.id, &a.comment].iter().any(|s| s.to_lowercase().contains(&q)))
                .collect();
            if !list.is_empty() {
                if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
                    self.sel = (self.sel + 1).min(list.len() - 1);
                }
                if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
                    self.sel = self.sel.saturating_sub(1);
                }
            }
            let enter = ui.input(|i| i.key_pressed(Key::Enter)) && !self.query.is_empty();
            let mut add = enter.then(|| list.get(self.sel).map(|a| a.id.clone())).flatten();
            let row_h = ROW_H + ui.spacing().item_spacing.y;
            let sel = self.sel;
            let icons = &mut self.icons;
            ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, list.len(), |ui, range| {
                for k in range {
                    if add_row(ui, icons, list[k], k == sel && !q.is_empty()).clicked() {
                        add = Some(list[k].id.clone());
                    }
                }
            });
            if let Some(id) = add {
                self.pins.push(Pin::App(id));
                self.query.clear();
                self.sel = 0;
                changed = true;
            }
        });
        if changed {
            apps::write_pins(&self.pins);
        }
    }
}

fn edit() -> i32 {
    if sway::run("[app_id=\"^aura-rclick-edit$\"] focus") {
        return 0; // already open
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Right-click menu")
            .with_app_id("aura-rclick-edit")
            .with_inner_size([440.0, 620.0])
            .with_min_inner_size([360.0, 420.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Right-click menu",
        options,
        Box::new(|cc| {
            install_theme(&cc.egui_ctx);
            Ok(Box::new(Editor {
                pins: apps::read_pins(),
                all: apps::all_apps(),
                query: String::new(),
                asks: apps::asks(),
                icons: IconCache::default(),
                focused_once: false,
                sel: 0,
            }))
        }),
    );
    i32::from(result.is_err())
}

// ── ask: a new app was installed ────────────────────────────────────────────

struct Asker {
    apps: Vec<(App, bool)>,
    icons: IconCache,
    /// The height asked of sway, so the window fits what it says.
    fitted: f32,
}

impl eframe::App for Asker {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
        let mut add = ctx.input(|i| i.key_pressed(Key::Enter));
        let panel = egui::CentralPanel::default().frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(14))).show(ctx, |ui| {
            let title = if self.apps.len() == 1 { "New app installed" } else { "New apps installed" };
            widgets::header(ui, title, "");
            ui.add_space(2.0);
            ui.label(RichText::new("Add to your right-click menu? Then one right-click on the desktop opens it.").color(theme::subtext0()));
            ui.add_space(6.0);
            for (i, (app, on)) in self.apps.iter_mut().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H + 4.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                self.icons.paint(ui, &app.icon, &app.name, Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(22.0, 22.0)));
                let font = TextStyle::Body.resolve(ui.style());
                let label = fit_text(ui, &app.name, &font, rect.width() - 36.0 - 40.0);
                ui.painter().text(pos2(rect.left() + 36.0, rect.center().y), Align2::LEFT_CENTER, label, font.clone(), theme::text());
                let (mark, c) = if *on { ("✓", theme::accent()) } else { ("—", theme::overlay0()) };
                ui.painter().text(pos2(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, mark, font, c);
                if resp.clicked() {
                    *on = !*on;
                }
                let _ = i;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let any = self.apps.iter().any(|(_, on)| *on);
                let yes = egui::Button::new(RichText::new("  Add to right-click  ").color(theme::base()).family(theme::bold())).fill(theme::accent());
                if ui.add_enabled(any, yes).clicked() {
                    add = true;
                }
                if ui.button("  Not now  ").clicked() {
                    close = true;
                }
            });
            ui.add_space(6.0);
            let stop = ui.add(egui::Label::new(RichText::new("Don't ask about new apps").size(11.0).color(theme::overlay0())).sense(Sense::click()));
            if stop.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if stop.clicked() {
                apps::set_asks(false);
                notify("Right-click menu", "It won't ask again. Turn it back on in right-click > Edit this menu.");
                close = true;
            }
            ui.min_rect().bottom() + 14.0
        });
        let want = panel.inner.ceil();
        if (want - self.fitted).abs() > 1.0 {
            // sway decides floating sizes, so ask it (in its own pixels, border
            // included). Until our window is mapped there is nothing to resize.
            let pid = std::process::id() as i32;
            let node = sway::get("get_tree").and_then(|t| sway::window_of_pid(&t, pid));
            if let Some(win) = node {
                let z = ctx.zoom_factor();
                let border = win.rect.h - (ctx.screen_rect().height() * z).round() as i32;
                let h = (want * z).round() as i32 + border.max(0);
                if sway::run(&format!("[con_id={}] resize set height {h} px", win.id)) {
                    self.fitted = want;
                }
            }
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if add {
            let names: Vec<String> = self.apps.iter().filter(|(_, on)| *on).map(|(a, _)| a.name.clone()).collect();
            for (app, on) in &self.apps {
                if *on {
                    apps::pin(&app.id);
                }
            }
            if !names.is_empty() {
                notify("Right-click menu", &format!("{} added. Right-click the desktop to open it.", names.join(", ")));
            }
            close = true;
        }
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn ask(ids: &[String]) -> i32 {
    let list: Vec<(App, bool)> = ids
        .iter()
        .filter_map(|id| apps::find(id.trim_end_matches(".desktop")))
        .filter(|a| a.listed && !apps::is_pinned(&a.id))
        .map(|a| (a, true))
        .collect();
    if list.is_empty() {
        return 0;
    }
    let h = 150.0 + 30.0 * list.len().min(10) as f32;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("New app installed")
            .with_app_id("aura-rclick-ask")
            .with_inner_size([420.0, h]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "New app installed",
        options,
        Box::new(move |cc| {
            install_theme(&cc.egui_ctx);
            Ok(Box::new(Asker { apps: list, icons: IconCache::default(), fitted: 0.0 }))
        }),
    );
    i32::from(result.is_err())
}

/// Run on every change to the applications folders. The first run only
/// remembers what is there; later runs ask about whatever is new.
fn scan(seed: bool) -> i32 {
    if !seed {
        // an install writes several files; let it finish
        std::thread::sleep(Duration::from_secs(2));
    }
    let now: BTreeSet<String> = apps::all_apps().into_iter().map(|a| a.id).collect();
    let seen = apps::read_seen();
    let mut remembered = now.clone();
    remembered.extend(seen.iter().flatten().cloned());
    apps::write_seen(&remembered);
    let Some(seen) = seen else { return 0 };
    if seed || !apps::asks() {
        return 0;
    }
    let pinned = apps::read_pins();
    let new: Vec<String> = now
        .difference(&seen)
        .filter(|id| !pinned.contains(&Pin::App((*id).clone())))
        .filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || "._-+@".contains(c)))
        .cloned()
        .collect();
    if new.is_empty() {
        return 0;
    }
    println!("new apps: {}", new.join(" "));
    // through sway, so the window outlives this (systemd) process
    let exe = self_exe();
    if exe.contains(char::is_whitespace) || !sway::run(&format!("exec {exe} ask {}", new.join(" "))) {
        let _ = Command::new(&exe).arg("ask").args(&new).spawn();
    }
    0
}

fn pin_cli(add: bool, ids: &[String]) -> i32 {
    if ids.is_empty() {
        eprintln!("{USAGE}");
        return 2;
    }
    let mut code = 0;
    for id in ids {
        let id = id.trim_end_matches(".desktop");
        if add {
            match apps::find(id) {
                Some(app) if apps::pin(id) => println!("added {} ({id})", app.name),
                Some(_) => println!("{id} is already in the menu"),
                None => {
                    eprintln!("aura-rclick: no app called {id} (see: ls /usr/share/applications)");
                    code = 1;
                }
            }
        } else if apps::unpin(id) {
            println!("removed {id}");
        } else {
            println!("{id} was not in the menu");
        }
    }
    code
}
