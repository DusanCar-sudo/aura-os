//! aura-tree — the desktop file tree, a pane on the right edge.
//!
//! Folders expand with a click; a file opens with a double-click (a folder
//! double-click opens it in Files). Right-click opens a menu: open, open with,
//! rename, new folder/file, trash, copy path, terminal here. Home or /
//! as the root, hidden files on demand. Sway keeps it in the scratchpad:
//! aura-tree-toggle (Super+. or the top bar's tree button) slides it in and
//! out. Folders are read when opened, so a huge disk costs nothing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aura_ui::{icons, theme, widgets};
use egui::{vec2, Align2, Frame, Key, Margin, Rect, RichText, ScrollArea, Sense, TextEdit};

const ROW: f32 = 24.0;
const INDENT: f32 = 14.0;

#[derive(Clone)]
struct Node {
    name: String,
    path: PathBuf,
    is_dir: bool,
}

/// Folders first, then files, each A→Z ignoring case.
fn read_dir(dir: &Path, hidden: bool) -> Vec<Node> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<Node> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !hidden && name.starts_with('.') {
                return None;
            }
            let path = e.path();
            Some(Node { is_dir: path.is_dir(), name, path })
        })
        .collect();
    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    v
}

/// The Aura pack icon for a file name (same mapping as Files).
fn icon_for(name: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "files";
    }
    let ext = name.to_lowercase().rsplit_once('.').map(|(_, x)| x.to_string()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "file-image",
        "mp4" | "mkv" | "webm" | "mov" | "avi" => "media",
        "mp3" | "flac" | "wav" | "ogg" | "opus" => "audio",
        "pdf" => "file-pdf",
        "zip" | "gz" | "xz" | "zst" | "tar" | "7z" | "rar" | "deb" | "pkg" => "package",
        "rs" | "py" | "sh" | "js" | "ts" | "c" | "h" | "go" | "toml" | "json" | "yaml" | "yml" => "code",
        "md" | "txt" | "log" | "conf" | "ini" | "csv" => "file-text",
        "appimage" | "exe" | "bin" | "run" => "file-exec",
        _ => "file-plain",
    }
}

fn spawn(cmd: &str, arg: &Path) {
    let _ = Command::new("setsid")
        .args(["-f", cmd])
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// An installed app that can open some kind of file.
#[derive(Clone)]
struct AppEntry {
    name: String,
    desktop: PathBuf,
}

/// Installed apps that list `mime` among the types they open, A→Z.
fn apps_for(mime: &str) -> Vec<AppEntry> {
    let mut dirs = vec![PathBuf::from("/usr/share/applications"), PathBuf::from("/usr/local/share/applications")];
    dirs.push(home().join(".local/share/applications"));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let (mut name, mut types, mut hidden) = (None, "", false);
            let mut in_entry = false;
            for line in text.lines() {
                if line.starts_with('[') {
                    in_entry = line == "[Desktop Entry]";
                } else if in_entry {
                    if let Some(v) = line.strip_prefix("Name=") {
                        name.get_or_insert(v.to_string());
                    } else if let Some(v) = line.strip_prefix("MimeType=") {
                        types = v;
                    } else if line == "NoDisplay=true" || line == "Hidden=true" {
                        hidden = true;
                    }
                }
            }
            let id = path.file_name().map(|f| f.to_owned());
            if hidden || !types.split(';').any(|t| t == mime) || !seen.insert(id) {
                continue;
            }
            if let Some(name) = name {
                out.push(AppEntry { name, desktop: path });
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn mime_of(path: &Path) -> String {
    if path.is_dir() {
        return "inode/directory".into();
    }
    Command::new("xdg-mime")
        .args(["query", "filetype"])
        .arg(path)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "application/octet-stream".into())
}

/// What a right-click choice asks for; applied after the row is drawn.
enum Act {
    Open(PathBuf),
    OpenWith(PathBuf, PathBuf),
    InFiles(PathBuf),
    Terminal(PathBuf),
    CopyPath(PathBuf),
    Trash(PathBuf),
    Dialog(Dialog),
}

enum Dialog {
    Rename { from: PathBuf, to: String },
    NewFolder { in_dir: PathBuf, name: String },
    NewFile { in_dir: PathBuf, name: String },
    Command { file: PathBuf, cmd: String },
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

struct App {
    root: PathBuf,
    open: HashSet<PathBuf>,
    /// read folders, dropped every few seconds so the tree stays current
    cache: HashMap<PathBuf, Vec<Node>>,
    cache_at: Instant,
    hidden: bool,
    selected: Option<PathBuf>,
    status: String,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    /// apps for the row that was last right-clicked
    open_with: Vec<AppEntry>,
    dialog: Option<Dialog>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        let root = std::env::args().nth(1).map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(home);
        Self {
            root,
            open: HashSet::new(),
            cache: HashMap::new(),
            cache_at: Instant::now(),
            hidden: false,
            selected: None,
            status: String::new(),
            theme_id,
            next_theme_check: 0.0,
            open_with: Vec::new(),
            dialog: None,
        }
    }

    fn children(&mut self, dir: &Path) -> Vec<Node> {
        let hidden = self.hidden;
        self.cache.entry(dir.to_path_buf()).or_insert_with(|| read_dir(dir, hidden)).clone()
    }

    fn set_root(&mut self, p: PathBuf) {
        self.root = p;
        self.open.clear();
        self.cache.clear();
        self.selected = None;
    }

    fn refresh(&mut self) {
        self.cache.clear();
        self.cache_at = Instant::now();
    }

    fn apply(&mut self, act: Act) {
        let run = |cmd: &str, args: &[&dyn AsRef<std::ffi::OsStr>]| {
            let mut c = Command::new("setsid");
            c.args(["-f", cmd]);
            for a in args {
                c.arg(a.as_ref());
            }
            let _ = c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
        };
        match act {
            Act::Open(p) => {
                run("xdg-open", &[&p]);
                self.status = format!("opened {}", p.file_name().unwrap_or_default().to_string_lossy());
            }
            Act::OpenWith(desktop, p) => {
                run("gio", &[&"launch", &desktop, &p]);
                self.status = format!("opened {}", p.file_name().unwrap_or_default().to_string_lossy());
            }
            Act::InFiles(p) => {
                run("aura-files-rs", &[&p]);
                self.status = "opened in Files".into();
            }
            Act::Terminal(p) => {
                run("aura-term", &[&format!("--working-directory={}", p.display())]);
                self.status = "terminal opened".into();
            }
            Act::CopyPath(p) => {
                let _ = Command::new("wl-copy").arg("--").arg(&p).stdin(Stdio::null()).status();
                self.status = "path copied".into();
            }
            Act::Trash(p) => {
                let ok = Command::new("gio").arg("trash").arg(&p).status().map(|s| s.success()).unwrap_or(false);
                self.status = if ok { format!("trashed {}", p.file_name().unwrap_or_default().to_string_lossy()) } else { "couldn't trash it".into() };
                self.selected = None;
                self.refresh();
            }
            Act::Dialog(d) => self.dialog = Some(d),
        }
    }

    /// The small window behind Rename / New / Other command.
    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else { return };
        let (title, hint) = match &d {
            Dialog::Rename { .. } => ("Rename", "new name"),
            Dialog::NewFolder { .. } => ("New folder", "folder name"),
            Dialog::NewFile { .. } => ("New file", "file name"),
            Dialog::Command { .. } => ("Open with", "command, e.g. mpv"),
        };
        let (mut go, mut cancel) = (false, ctx.input(|i| i.key_pressed(Key::Escape)));
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 120.0))
            .frame(Frame::new().fill(theme::mantle()).inner_margin(Margin::same(12)).stroke(egui::Stroke::new(1.0_f32, theme::surface1())))
            .show(ctx, |ui| {
                let text = match &mut d {
                    Dialog::Rename { to, .. } => to,
                    Dialog::NewFolder { name, .. } | Dialog::NewFile { name, .. } => name,
                    Dialog::Command { cmd, .. } => cmd,
                };
                let edit = ui.add(TextEdit::singleline(text).desired_width(240.0).hint_text(hint));
                edit.request_focus();
                if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    go = true;
                }
                ui.horizontal(|ui| {
                    go |= ui.button("OK").clicked();
                    cancel |= ui.button("Cancel").clicked();
                });
            });
        if cancel {
            return;
        }
        if !go {
            self.dialog = Some(d);
            return;
        }
        let bad = |s: &str| s.is_empty() || s.contains('/') || s == "." || s == "..";
        match d {
            Dialog::Rename { from, to } => {
                let to = to.trim().to_string();
                let target = from.with_file_name(&to);
                if bad(&to) || (target != from && target.exists()) {
                    self.status = "that name won't work".into();
                } else if let Err(e) = std::fs::rename(&from, &target) {
                    self.status = format!("couldn't rename: {e}");
                } else {
                    self.status = format!("renamed to {to}");
                    if self.open.remove(&from) {
                        self.open.insert(target.clone());
                    }
                    self.selected = Some(target);
                }
            }
            Dialog::NewFolder { in_dir, name } | Dialog::NewFile { in_dir, name } if bad(name.trim()) => {
                let _ = in_dir;
                self.status = "that name won't work".into();
            }
            Dialog::NewFolder { in_dir, name } => {
                let t = in_dir.join(name.trim());
                match std::fs::create_dir(&t) {
                    Ok(()) => { self.open.insert(in_dir); self.selected = Some(t); self.status = "folder created".into() }
                    Err(e) => self.status = format!("couldn't create: {e}"),
                }
            }
            Dialog::NewFile { in_dir, name } => {
                let t = in_dir.join(name.trim());
                match std::fs::OpenOptions::new().write(true).create_new(true).open(&t) {
                    Ok(_) => { self.open.insert(in_dir); self.selected = Some(t); self.status = "file created".into() }
                    Err(e) => self.status = format!("couldn't create: {e}"),
                }
            }
            Dialog::Command { file, cmd } => {
                let mut parts = cmd.split_whitespace();
                if let Some(prog) = parts.next() {
                    let _ = Command::new("setsid").args(["-f", prog]).args(parts).arg(&file).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
                    self.status = format!("opened with {prog}");
                }
            }
        }
        self.refresh();
    }

    /// One row, then its children when open. Returns nothing; mutates state.
    fn branch(&mut self, ui: &mut egui::Ui, dir: &Path, depth: usize) {
        for n in self.children(dir) {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
            let is_open = n.is_dir && self.open.contains(&n.path);
            let picked = self.selected.as_deref() == Some(n.path.as_path());
            if picked || resp.hovered() {
                ui.painter().rect_filled(rect, theme::radius(), if picked { theme::surface1() } else { theme::surface0() });
            }
            let x = rect.left() + 6.0 + depth as f32 * INDENT;
            let colour = if n.name.starts_with('.') { theme::overlay0() } else if n.is_dir { theme::accent() } else { theme::text() };
            let p = ui.painter();
            if n.is_dir {
                p.text(egui::pos2(x + 4.0, rect.center().y), Align2::CENTER_CENTER, if is_open { "▾" } else { "▸" }, egui::FontId::monospace(12.0), theme::overlay0());
            }
            icons::paint(ui, icon_for(&n.name, n.is_dir), Rect::from_center_size(egui::pos2(x + 20.0, rect.center().y), vec2(15.0, 15.0)), colour);
            ui.painter().text(egui::pos2(x + 34.0, rect.center().y), Align2::LEFT_CENTER, &n.name, egui::FontId::monospace(13.0), theme::text());

            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                self.selected = Some(n.path.clone());
                if n.is_dir && !self.open.remove(&n.path) {
                    self.open.insert(n.path.clone());
                }
            }
            if resp.double_clicked() {
                if n.is_dir {
                    spawn("aura-files-rs", &n.path);
                    self.status = format!("opened {} in Files", n.name);
                } else {
                    spawn("xdg-open", &n.path);
                    self.status = format!("opened {}", n.name);
                }
            }
            if resp.secondary_clicked() {
                self.selected = Some(n.path.clone());
                self.open_with = apps_for(&mime_of(&n.path));
            }
            let mut act = None;
            resp.context_menu(|ui| {
                ui.set_min_width(190.0);
                let here = if n.is_dir { n.path.clone() } else { n.path.parent().map(Path::to_path_buf).unwrap_or_else(home) };
                if ui.button("Open").clicked() {
                    act = Some(if n.is_dir { Act::InFiles(n.path.clone()) } else { Act::Open(n.path.clone()) });
                    ui.close_menu();
                }
                ui.menu_button("Open with", |ui| {
                    ui.set_min_width(180.0);
                    for a in &self.open_with {
                        if ui.button(&a.name).clicked() {
                            act = Some(Act::OpenWith(a.desktop.clone(), n.path.clone()));
                            ui.close_menu();
                        }
                    }
                    if !self.open_with.is_empty() {
                        ui.separator();
                    }
                    if ui.button("Other command…").clicked() {
                        act = Some(Act::Dialog(Dialog::Command { file: n.path.clone(), cmd: String::new() }));
                        ui.close_menu();
                    }
                });
                if ui.button("Show in Files").clicked() {
                    act = Some(Act::InFiles(here.clone()));
                    ui.close_menu();
                }
                if ui.button("Terminal here").clicked() {
                    act = Some(Act::Terminal(here.clone()));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Rename").clicked() {
                    act = Some(Act::Dialog(Dialog::Rename { from: n.path.clone(), to: n.name.clone() }));
                    ui.close_menu();
                }
                if ui.button("New folder").clicked() {
                    act = Some(Act::Dialog(Dialog::NewFolder { in_dir: here.clone(), name: String::new() }));
                    ui.close_menu();
                }
                if ui.button("New file").clicked() {
                    act = Some(Act::Dialog(Dialog::NewFile { in_dir: here.clone(), name: String::new() }));
                    ui.close_menu();
                }
                ui.separator();
                if ui.button("Copy path").clicked() {
                    act = Some(Act::CopyPath(n.path.clone()));
                    ui.close_menu();
                }
                if ui.button("Move to trash").clicked() {
                    act = Some(Act::Trash(n.path.clone()));
                    ui.close_menu();
                }
            });
            if let Some(a) = act {
                self.apply(a);
            }
            resp.on_hover_text_at_pointer(n.path.to_string_lossy());

            if is_open {
                self.branch(ui, &n.path, depth + 1);
            }
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
        if self.cache_at.elapsed() > Duration::from_secs(3) {
            self.cache.clear(); // re-read open folders: new files show up
            self.cache_at = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_secs(3));
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            let _ = Command::new("aura-tree-toggle").arg("hide").status();
        }

        self.dialogs(ctx);
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(10)))
            .show(ctx, |ui| {
                widgets::header(ui, "tree", "click opens · double-click launches · right-click for more");
                ui.horizontal(|ui| {
                    let home = home();
                    if ui.selectable_label(self.root == home, "home").clicked() {
                        self.set_root(home);
                    }
                    if ui.selectable_label(self.root == Path::new("/"), "/").clicked() {
                        self.set_root(PathBuf::from("/"));
                    }
                    if ui.selectable_label(self.hidden, "hidden").clicked() {
                        self.hidden = !self.hidden;
                        self.cache.clear();
                    }
                    if ui.button("collapse").clicked() {
                        self.open.clear();
                    }
                });
                ui.label(RichText::new(self.root.to_string_lossy()).size(11.0).color(theme::overlay0()));
                widgets::hairline(ui);
                let bottom = if self.status.is_empty() { 0.0 } else { 20.0 };
                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(ui.available_height() - bottom)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let root = self.root.clone();
                        self.branch(ui, &root, 0);
                    });
                if !self.status.is_empty() {
                    ui.label(RichText::new(&self.status).size(11.0).color(theme::subtext0()));
                }
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura tree")
            .with_app_id("aura-tree")
            .with_inner_size([340.0, 900.0])
            .with_min_inner_size([220.0, 300.0]),
        ..Default::default()
    };
    eframe::run_native("Aura tree", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_first_then_names() {
        let d = std::env::temp_dir().join(format!("aura-tree-test-{}", std::process::id()));
        std::fs::create_dir_all(d.join("zeta")).unwrap();
        std::fs::write(d.join("Alpha.txt"), "").unwrap();
        std::fs::write(d.join(".secret"), "").unwrap();
        let names: Vec<String> = read_dir(&d, false).into_iter().map(|n| n.name).collect();
        assert_eq!(names, ["zeta", "Alpha.txt"]);
        assert_eq!(read_dir(&d, true).len(), 3);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn icons() {
        assert_eq!(icon_for("a.RS", false), "code");
        assert_eq!(icon_for("x", true), "files");
        assert_eq!(icon_for("noext", false), "file-plain");
    }
}
