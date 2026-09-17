//! aura-files-rs — the file manager as a real window.
//!
//! Icons or a list, places down the left, a path bar you can copy from and
//! paste into, and the usual work: open, new folder, copy, cut, paste,
//! rename, delete, with a right-click menu on any item. File work runs on a
//! worker thread, so copying a big folder never freezes the window.
//!
//! Keys: Enter open · Backspace up · Alt+←/→ back/forward · Ctrl+C/X/V ·
//! F2 rename · Delete · Ctrl+Shift+N new folder · Ctrl+L path · Ctrl+H
//! hidden files · Ctrl+F filter · F5 refresh · Esc clears.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use aura_ui::{theme, widgets};
use egui::{Align, Frame, Key, Layout, Margin, Modifiers, RichText, ScrollArea, Sense, TextEdit, Vec2, vec2};

#[derive(Clone, PartialEq)]
struct Entry {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<std::time::SystemTime>,
    hidden: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Name,
    Size,
    Modified,
    Kind,
}

#[derive(Clone, PartialEq)]
enum Dialog {
    None,
    Rename { from: String, to: String },
    NewFolder { name: String },
    Confirm { what: String, names: Vec<String> },
    Properties { name: String },
    GoTo { text: String },
}

/// Work that must not happen on the drawing thread.
enum Job {
    Paste { paths: Vec<PathBuf>, to: PathBuf, cut: bool },
    Delete { paths: Vec<PathBuf> },
}

fn human(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[u]) }
}

fn when(t: Option<std::time::SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // date maths without a calendar crate: days since epoch → y/m/d
    let days = secs / 86_400;
    let (mut y, mut d) = (1970i64, days as i64);
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if d < len {
            break;
        }
        d -= len;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let lens = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut m = 0;
    while m < 12 && d >= lens[m] {
        d -= lens[m];
        m += 1;
    }
    let (h, min) = ((secs % 86_400) / 3600, (secs % 3600) / 60);
    format!("{:02} {} {} {:02}:{:02}", d + 1, names[m.min(11)], y, h, min)
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

fn places() -> Vec<(String, PathBuf)> {
    let h = home();
    let mut v: Vec<(String, PathBuf)> = vec![("Home".into(), h.clone())];
    for name in ["Documents", "Downloads", "Pictures", "Music", "Videos"] {
        let p = h.join(name);
        if p.is_dir() {
            v.push((name.into(), p));
        }
    }
    for p in ["/mnt/data", "/mnt/bigdata", "/"] {
        let path = PathBuf::from(p);
        if path.is_dir() {
            let label = if p == "/" { "Root".to_string() } else { path.file_name().unwrap().to_string_lossy().to_string() };
            v.push((label, path));
        }
    }
    v
}

fn icon_for(e: &Entry) -> &'static str {
    if e.is_dir {
        return "▸";
    }
    let n = e.name.to_lowercase();
    let ext = n.rsplit_once('.').map(|(_, x)| x.to_string()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "▣",
        "mp4" | "mkv" | "webm" | "mov" | "avi" => "▶",
        "mp3" | "flac" | "wav" | "ogg" | "opus" => "♪",
        "pdf" => "▤",
        "zip" | "gz" | "xz" | "zst" | "tar" | "7z" | "rar" => "◰",
        "rs" | "py" | "sh" | "js" | "ts" | "c" | "h" | "go" | "toml" | "json" | "yaml" | "yml" => "◇",
        "md" | "txt" | "log" | "conf" | "ini" | "csv" => "≡",
        _ => "·",
    }
}

fn is_text(path: &Path) -> bool {
    let Ok(data) = std::fs::read(path) else { return false };
    let head = &data[..data.len().min(4096)];
    !head.contains(&0) && String::from_utf8(head.to_vec()).is_ok()
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dst = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dst)?;
        } else {
            std::fs::copy(entry.path(), dst)?;
        }
    }
    Ok(())
}

/// A free name when the target exists: "notes (copy).txt", then " (copy 2)".
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let mut target = dir.join(name);
    if !target.exists() {
        return target;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for n in 1..1000 {
        let suffix = if n == 1 { " (copy)".to_string() } else { format!(" (copy {n})") };
        target = dir.join(format!("{stem}{suffix}{ext}"));
        if !target.exists() {
            return target;
        }
    }
    target
}

fn wl_copy(text: &str) {
    let _ = Command::new("wl-copy").arg("--").arg(text).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
}

fn wl_paste() -> String {
    Command::new("wl-paste")
        .arg("-n")
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

struct App {
    path: PathBuf,
    entries: Vec<Entry>,
    selected: Vec<String>,
    last_click: Option<String>,
    history: Vec<PathBuf>,
    future: Vec<PathBuf>,
    clipboard: (Vec<PathBuf>, bool), // paths, cut?
    icons_view: bool,
    sort: Sort,
    reverse: bool,
    show_hidden: bool,
    filter: String,
    dialog: Dialog,
    status: Arc<Mutex<String>>,
    jobs: Sender<Job>,
    done: Receiver<String>,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, start: PathBuf) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);

        let (jobs, rx) = channel::<Job>();
        let (tx_done, done) = channel::<String>();
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            for job in rx {
                let msg = match job {
                    Job::Paste { paths, to, cut } => {
                        let mut n = 0;
                        let mut err = None;
                        for p in &paths {
                            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            let dst = free_name(&to, &name);
                            let res = if cut && std::fs::rename(p, &dst).is_ok() {
                                Ok(())
                            } else if p.is_dir() {
                                copy_dir(p, &dst).and_then(|_| if cut { std::fs::remove_dir_all(p) } else { Ok(()) })
                            } else {
                                std::fs::copy(p, &dst).map(|_| ()).and_then(|_| if cut { std::fs::remove_file(p) } else { Ok(()) })
                            };
                            match res {
                                Ok(()) => n += 1,
                                Err(e) => { err = Some(format!("{name}: {e}")); break; }
                            }
                        }
                        match err {
                            Some(e) => e,
                            None => format!("{} {n} item{}", if cut { "Moved" } else { "Copied" }, if n == 1 { "" } else { "s" }),
                        }
                    }
                    Job::Delete { paths } => {
                        let mut n = 0;
                        let mut err = None;
                        for p in &paths {
                            let res = if p.is_dir() && !p.is_symlink() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
                            match res {
                                Ok(()) => n += 1,
                                Err(e) => { err = Some(format!("{}: {e}", p.display())); break; }
                            }
                        }
                        err.unwrap_or_else(|| format!("Deleted {n} item{}", if n == 1 { "" } else { "s" }))
                    }
                };
                let _ = tx_done.send(msg);
                ctx.request_repaint();
            }
        });

        let mut app = Self {
            path: start,
            entries: Vec::new(),
            selected: Vec::new(),
            last_click: None,
            history: Vec::new(),
            future: Vec::new(),
            clipboard: (Vec::new(), false),
            icons_view: true,
            sort: Sort::Name,
            reverse: false,
            show_hidden: false,
            filter: String::new(),
            dialog: Dialog::None,
            status: Arc::new(Mutex::new(String::new())),
            jobs,
            done,
            theme_id,
            next_theme_check: 0.0,
        };
        app.read_dir();
        app
    }

    fn say(&self, msg: impl Into<String>) {
        *self.status.lock().unwrap() = msg.into();
    }

    fn read_dir(&mut self) {
        let mut v = Vec::new();
        match std::fs::read_dir(&self.path) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    let md = e.metadata().ok();
                    v.push(Entry {
                        hidden: name.starts_with('.'),
                        is_dir: md.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                        size: md.as_ref().map(|m| m.len()).unwrap_or(0),
                        modified: md.as_ref().and_then(|m| m.modified().ok()),
                        name,
                    });
                }
            }
            Err(e) => self.say(format!("Can't open {}: {e}", self.path.display())),
        }
        self.entries = v;
        self.sort_entries();
        self.selected.clear();
    }

    fn sort_entries(&mut self) {
        let sort = self.sort;
        self.entries.sort_by(|a, b| {
            let first = b.is_dir.cmp(&a.is_dir); // folders first, always
            first.then(match sort {
                Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                Sort::Size => a.size.cmp(&b.size),
                Sort::Modified => a.modified.cmp(&b.modified),
                Sort::Kind => {
                    let k = |e: &Entry| e.name.rsplit_once('.').map(|(_, x)| x.to_lowercase()).unwrap_or_default();
                    k(a).cmp(&k(b)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                }
            })
        });
        if self.reverse {
            let dirs = self.entries.iter().filter(|e| e.is_dir).count();
            self.entries[..dirs].reverse();
            self.entries[dirs..].reverse();
        }
    }

    fn shown(&self) -> Vec<Entry> {
        let f = self.filter.to_lowercase();
        self.entries
            .iter()
            .filter(|e| (self.show_hidden || !e.hidden) && (f.is_empty() || e.name.to_lowercase().contains(&f)))
            .cloned()
            .collect()
    }

    fn go(&mut self, to: PathBuf, remember: bool) {
        let to = if to.is_absolute() { to } else { self.path.join(to) };
        let Ok(to) = to.canonicalize() else {
            self.say(format!("No such folder: {}", to.display()));
            return;
        };
        if !to.is_dir() {
            self.say(format!("Not a folder: {}", to.display()));
            return;
        }
        if remember && to != self.path {
            self.history.push(self.path.clone());
            self.future.clear();
        }
        self.path = to;
        self.filter.clear();
        self.read_dir();
    }

    fn open(&mut self, e: &Entry) {
        let full = self.path.join(&e.name);
        if e.is_dir {
            self.go(full, true);
            return;
        }
        if is_text(&full) {
            let editor = home().join(".local/bin/aura-edit");
            let cmd = if editor.exists() {
                format!("aura-term --app-id=aura-edit -e {} {:?}", editor.display(), full)
            } else {
                format!("xdg-open {full:?}")
            };
            let _ = Command::new("setsid").args(["-f", "sh", "-c", &cmd]).stdin(Stdio::null())
                .stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        } else {
            let _ = Command::new("setsid").args(["-f", "xdg-open"]).arg(&full).stdin(Stdio::null())
                .stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        }
        self.say(format!("Opening {}…", e.name));
    }

    fn targets(&self) -> Vec<PathBuf> {
        self.selected.iter().map(|n| self.path.join(n)).collect()
    }

    fn copy_cut(&mut self, cut: bool) {
        let t = self.targets();
        if t.is_empty() {
            self.say("Nothing selected");
            return;
        }
        let n = t.len();
        self.clipboard = (t, cut);
        self.say(format!("{} {n} item{} — open a folder and paste", if cut { "Cut" } else { "Copied" }, if n == 1 { "" } else { "s" }));
    }

    fn paste(&mut self) {
        let (paths, cut) = self.clipboard.clone();
        if paths.is_empty() {
            // a path on the system clipboard? then go there instead
            let t = wl_paste();
            if !t.is_empty() && Path::new(&t).exists() {
                let p = PathBuf::from(&t);
                if p.is_dir() { self.go(p, true) } else if let Some(d) = p.parent() { self.go(d.to_path_buf(), true) }
                return;
            }
            self.say("Nothing to paste — copy or cut first");
            return;
        }
        self.say("Working…");
        let _ = self.jobs.send(Job::Paste { paths, to: self.path.clone(), cut });
        if cut {
            self.clipboard.0.clear();
        }
    }

    fn selected_or_current(&self) -> Vec<String> {
        if !self.selected.is_empty() {
            self.selected.clone()
        } else {
            self.last_click.iter().cloned().collect()
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
        while let Ok(msg) = self.done.try_recv() {
            self.say(msg);
            self.read_dir();
        }

        self.keys(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.side(ctx);
        self.body(ctx);
        self.dialogs(ctx);
    }
}

impl App {
    fn keys(&mut self, ctx: &egui::Context) {
        if self.dialog != Dialog::None {
            return;
        }
        let (mut back, mut forward) = (false, false);
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::ALT, Key::ArrowLeft) { back = true }
            if i.consume_key(Modifiers::ALT, Key::ArrowRight) { forward = true }
            if i.consume_key(Modifiers::COMMAND, Key::C) { }
        });
        if back {
            if let Some(p) = self.history.pop() {
                self.future.push(self.path.clone());
                self.go(p, false);
            }
        }
        if forward {
            if let Some(p) = self.future.pop() {
                self.history.push(self.path.clone());
                self.go(p, false);
            }
        }
        let i = ctx.input(|i| i.clone());
        let cmd = i.modifiers.command;
        let shift = i.modifiers.shift;
        if i.key_pressed(Key::Backspace) {
            if let Some(p) = self.path.parent().map(Path::to_path_buf) {
                self.go(p, true);
            }
        }
        if i.key_pressed(Key::F5) {
            self.read_dir();
        }
        if i.key_pressed(Key::Escape) {
            if self.filter.is_empty() {
                self.selected.clear();
            } else {
                self.filter.clear();
            }
        }
        if cmd && i.key_pressed(Key::H) {
            self.show_hidden = !self.show_hidden;
        }
        if cmd && i.key_pressed(Key::L) {
            self.dialog = Dialog::GoTo { text: self.path.display().to_string() };
        }
        if cmd && i.key_pressed(Key::C) {
            self.copy_cut(false);
        }
        if cmd && i.key_pressed(Key::X) {
            self.copy_cut(true);
        }
        if cmd && i.key_pressed(Key::V) {
            self.paste();
        }
        if cmd && i.key_pressed(Key::A) {
            self.selected = self.shown().iter().map(|e| e.name.clone()).collect();
        }
        if cmd && shift && i.key_pressed(Key::N) {
            self.dialog = Dialog::NewFolder { name: "New folder".into() };
        }
        if i.key_pressed(Key::F2) {
            if let Some(n) = self.selected_or_current().first().cloned() {
                self.dialog = Dialog::Rename { from: n.clone(), to: n };
            }
        }
        if i.key_pressed(Key::Delete) {
            let names = self.selected_or_current();
            if !names.is_empty() {
                self.dialog = Dialog::Confirm {
                    what: if names.len() == 1 { names[0].clone() } else { format!("{} items", names.len()) },
                    names,
                };
            }
        }
        if i.key_pressed(Key::Enter) {
            if let Some(n) = self.selected_or_current().first().cloned() {
                if let Some(e) = self.entries.iter().find(|e| e.name == n).cloned() {
                    self.open(&e);
                }
            }
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar")
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(10, 8)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.history.is_empty(), egui::Button::new("←")).clicked() {
                        if let Some(p) = self.history.pop() {
                            self.future.push(self.path.clone());
                            self.go(p, false);
                        }
                    }
                    if ui.add_enabled(!self.future.is_empty(), egui::Button::new("→")).clicked() {
                        if let Some(p) = self.future.pop() {
                            self.history.push(self.path.clone());
                            self.go(p, false);
                        }
                    }
                    if ui.button("↑").on_hover_text("up a folder (Backspace)").clicked() {
                        if let Some(p) = self.path.parent().map(Path::to_path_buf) {
                            self.go(p, true);
                        }
                    }
                    ui.separator();
                    if ui.button("New folder").clicked() {
                        self.dialog = Dialog::NewFolder { name: "New folder".into() };
                    }
                    if ui.button("Copy").clicked() { self.copy_cut(false) }
                    if ui.button("Cut").clicked() { self.copy_cut(true) }
                    if ui.add_enabled(!self.clipboard.0.is_empty(), egui::Button::new("Paste")).clicked() {
                        self.paste();
                    }
                    if ui.button("Rename").clicked() {
                        if let Some(n) = self.selected_or_current().first().cloned() {
                            self.dialog = Dialog::Rename { from: n.clone(), to: n };
                        }
                    }
                    if ui.button("Delete").clicked() {
                        let names = self.selected_or_current();
                        if !names.is_empty() {
                            self.dialog = Dialog::Confirm {
                                what: if names.len() == 1 { names[0].clone() } else { format!("{} items", names.len()) },
                                names,
                            };
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(if self.icons_view { "▦ icons" } else { "≡ list" }).clicked() {
                            self.icons_view = !self.icons_view;
                        }
                        egui::ComboBox::from_id_salt("sort")
                            .selected_text(match self.sort {
                                Sort::Name => "name",
                                Sort::Size => "size",
                                Sort::Modified => "changed",
                                Sort::Kind => "kind",
                            })
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                for (s, label) in [(Sort::Name, "name"), (Sort::Size, "size"), (Sort::Modified, "changed"), (Sort::Kind, "kind")] {
                                    if ui.selectable_label(self.sort == s, label).clicked() {
                                        self.sort = s;
                                        self.sort_entries();
                                    }
                                }
                                if ui.selectable_label(self.reverse, "reverse").clicked() {
                                    self.reverse = !self.reverse;
                                    self.sort_entries();
                                }
                                if ui.selectable_label(self.show_hidden, "hidden files").clicked() {
                                    self.show_hidden = !self.show_hidden;
                                }
                            });
                        ui.add(TextEdit::singleline(&mut self.filter).hint_text("filter").desired_width(120.0));
                    });
                });
                // path bar: click to type or paste a path, right-click to copy it
                let shown = self.path.display().to_string().replace(&home().display().to_string(), "~");
                let resp = ui.add(egui::Label::new(RichText::new(&shown).color(theme::accent())).sense(Sense::click()));
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    self.dialog = Dialog::GoTo { text: self.path.display().to_string() };
                }
                if resp.secondary_clicked() {
                    wl_copy(&self.path.display().to_string());
                    self.say(format!("Copied folder path: {}", self.path.display()));
                }
                resp.on_hover_text("click: go to a path · right-click: copy this path");
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        let status = self.status.lock().unwrap().clone();
        let shown = self.shown();
        egui::TopBottomPanel::bottom("status")
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(10, 6)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let sel = self.selected.len();
                    let left = if sel > 0 {
                        let bytes: u64 = self.entries.iter().filter(|e| self.selected.contains(&e.name)).map(|e| e.size).sum();
                        format!("{sel} selected  ·  {}", human(bytes))
                    } else {
                        format!("{} items", shown.len())
                    };
                    ui.label(RichText::new(left).color(theme::subtext0()));
                    if !status.is_empty() {
                        ui.label(RichText::new("·").color(theme::overlay0()));
                        ui.label(RichText::new(status).color(theme::overlay0()));
                    }
                });
            });
    }

    fn side(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("places")
            .exact_width(150.0)
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::symmetric(6, 8)))
            .show(ctx, |ui| {
                widgets::caption(ui, "places");
                for (label, path) in places() {
                    let here = path == self.path;
                    let text = RichText::new(&label).color(if here { theme::accent() } else { theme::text() });
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(rect, theme::radius(), theme::surface0());
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.painter().text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, text.text(), egui::FontId::monospace(13.0), if here { theme::accent() } else { theme::text() });
                    if resp.clicked() {
                        self.go(path, true);
                    }
                }
            });
    }

    fn body(&mut self, ctx: &egui::Context) {
        let entries = self.shown();
        let mut open_me: Option<Entry> = None;
        let mut context_for: Option<String> = None;

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(8)))
            .show(ctx, |ui| {
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if entries.is_empty() {
                        widgets::caption(ui, if self.filter.is_empty() { "this folder is empty" } else { "nothing matches the filter" });
                    }
                    if self.icons_view {
                        let tile = Vec2::new(116.0, 84.0);
                        let cols = ((ui.available_width() / tile.x).floor() as usize).max(1);
                        for row in entries.chunks(cols) {
                            ui.horizontal(|ui| {
                                for e in row {
                                    let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
                                    let picked = self.selected.contains(&e.name);
                                    if picked || resp.hovered() {
                                        ui.painter().rect_filled(rect, theme::radius(), if picked { theme::surface1() } else { theme::surface0() });
                                    }
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    let colour = if e.hidden { theme::overlay0() } else if e.is_dir { theme::accent() } else { theme::text() };
                                    ui.painter().text(rect.center() - vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, icon_for(e), egui::FontId::monospace(26.0), colour);
                                    let mut label = e.name.clone();
                                    if label.chars().count() > 14 {
                                        label = format!("{}…", label.chars().take(13).collect::<String>());
                                    }
                                    ui.painter().text(rect.center() + vec2(0.0, 22.0), egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(12.0), theme::text());
                                    self.click(ctx, &resp, e, &mut open_me, &mut context_for);
                                }
                            });
                        }
                    } else {
                        for e in &entries {
                            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                            let picked = self.selected.contains(&e.name);
                            if picked || resp.hovered() {
                                ui.painter().rect_filled(rect, theme::radius(), if picked { theme::surface1() } else { theme::surface0() });
                            }
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let colour = if e.hidden { theme::overlay0() } else if e.is_dir { theme::accent() } else { theme::text() };
                            let p = ui.painter();
                            p.text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, icon_for(e), egui::FontId::monospace(13.0), colour);
                            p.text(rect.left_center() + vec2(28.0, 0.0), egui::Align2::LEFT_CENTER, &e.name, egui::FontId::monospace(13.0), theme::text());
                            p.text(rect.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, when(e.modified), egui::FontId::monospace(12.0), theme::overlay0());
                            p.text(rect.right_center() - vec2(150.0, 0.0), egui::Align2::RIGHT_CENTER, if e.is_dir { String::new() } else { human(e.size) }, egui::FontId::monospace(12.0), theme::subtext0());
                            self.click(ctx, &resp, e, &mut open_me, &mut context_for);
                        }
                    }
                });
            });

        if let Some(e) = open_me {
            self.open(&e);
        }
        if let Some(name) = context_for {
            // the little menu is drawn by egui's popup, anchored on the item
            self.last_click = Some(name);
        }
    }

    /// Clicks on one item: select, add to selection, open, or context menu.
    fn click(&mut self, _ctx: &egui::Context, resp: &egui::Response, e: &Entry, open_me: &mut Option<Entry>, context_for: &mut Option<String>) {
        if resp.clicked() {
            let add = resp.ctx.input(|i| i.modifiers.ctrl || i.modifiers.shift);
            if add {
                if let Some(pos) = self.selected.iter().position(|n| n == &e.name) {
                    self.selected.remove(pos);
                } else {
                    self.selected.push(e.name.clone());
                }
            } else {
                self.selected = vec![e.name.clone()];
            }
            self.last_click = Some(e.name.clone());
        }
        if resp.double_clicked() {
            *open_me = Some(e.clone());
        }
        if resp.secondary_clicked() {
            if !self.selected.contains(&e.name) {
                self.selected = vec![e.name.clone()];
            }
            *context_for = Some(e.name.clone());
        }
        resp.context_menu(|ui| {
            ui.set_min_width(170.0);
            if ui.button("Open").clicked() {
                *open_me = Some(e.clone());
                ui.close_menu();
            }
            if ui.button("⧉ Copy location").clicked() {
                let full = self.path.join(&e.name);
                wl_copy(&full.display().to_string());
                self.say(format!("Copied location: {}", full.display()));
                ui.close_menu();
            }
            if ui.button("Copy").clicked() { self.copy_cut(false); ui.close_menu(); }
            if ui.button("Cut").clicked() { self.copy_cut(true); ui.close_menu(); }
            if ui.button("Rename").clicked() {
                self.dialog = Dialog::Rename { from: e.name.clone(), to: e.name.clone() };
                ui.close_menu();
            }
            if ui.button("Delete").clicked() {
                let names = if self.selected.is_empty() { vec![e.name.clone()] } else { self.selected.clone() };
                self.dialog = Dialog::Confirm {
                    what: if names.len() == 1 { names[0].clone() } else { format!("{} items", names.len()) },
                    names,
                };
                ui.close_menu();
            }
            ui.separator();
            if ui.button("ⓘ Properties").clicked() {
                self.dialog = Dialog::Properties { name: e.name.clone() };
                ui.close_menu();
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let mut dialog = std::mem::replace(&mut self.dialog, Dialog::None);
        let mut keep = true;
        match &mut dialog {
            Dialog::None => {}
            Dialog::GoTo { text } => {
                let mut go = false;
                self.window(ctx, "Go to folder", |ui, _here| {
                    let edit = ui.add(TextEdit::singleline(text).desired_width(420.0).hint_text("/path or ~/path"));
                    edit.request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Go").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                            go = true;
                        }
                        if ui.button("Paste").clicked() {
                            *text = wl_paste();
                        }
                        ui.label(RichText::new("Ctrl+V pastes too").color(theme::overlay0()));
                    });
                    if ui.input(|i| i.modifiers.command && i.key_pressed(Key::V)) {
                        *text = wl_paste();
                    }
                });
                if go {
                    let t = text.trim().trim_matches('"').to_string();
                    let t = if let Some(rest) = t.strip_prefix('~') { home().display().to_string() + rest } else { t };
                    let p = PathBuf::from(&t);
                    if p.is_file() {
                        if let Some(d) = p.parent() {
                            self.go(d.to_path_buf(), true);
                            self.selected = vec![p.file_name().unwrap().to_string_lossy().to_string()];
                        }
                    } else {
                        self.go(p, true);
                    }
                    keep = false;
                }
            }
            Dialog::NewFolder { name } => {
                let mut make = false;
                self.window(ctx, "New folder", |ui, _here| {
                    let edit = ui.add(TextEdit::singleline(name).desired_width(320.0));
                    edit.request_focus();
                    if ui.button("Create").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                        make = true;
                    }
                });
                if make {
                    match std::fs::create_dir(self.path.join(&name)) {
                        Ok(()) => { self.say(format!("Created {name}")); self.read_dir(); }
                        Err(e) => self.say(format!("Couldn't create: {e}")),
                    }
                    keep = false;
                }
            }
            Dialog::Rename { from, to } => {
                let mut rename = false;
                self.window(ctx, "Rename", |ui, _here| {
                    let edit = ui.add(TextEdit::singleline(to).desired_width(320.0));
                    edit.request_focus();
                    if ui.button("Rename").clicked() || (edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))) {
                        rename = true;
                    }
                });
                if rename {
                    if to.trim().is_empty() || to == from {
                        keep = false;
                    } else if self.path.join(&to).exists() {
                        self.say(format!("{to} already exists"));
                    } else {
                        match std::fs::rename(self.path.join(&from), self.path.join(&to)) {
                            Ok(()) => { self.say(format!("Renamed to {to}")); self.read_dir(); }
                            Err(e) => self.say(format!("Couldn't rename: {e}")),
                        }
                        keep = false;
                    }
                }
            }
            Dialog::Confirm { what, names } => {
                let mut yes = false;
                let what = what.clone();
                self.window(ctx, "Delete", |ui, _here| {
                    ui.label(RichText::new(format!("Delete {what} permanently?")).color(theme::text()));
                    ui.label(RichText::new("This cannot be undone.").color(theme::overlay0()));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() { yes = true }
                        if ui.button("Cancel").clicked() { }
                    });
                });
                if yes {
                    let paths: Vec<PathBuf> = names.iter().map(|n| self.path.join(n)).collect();
                    self.say("Deleting…");
                    let _ = self.jobs.send(Job::Delete { paths });
                    keep = false;
                }
            }
            Dialog::Properties { name } => {
                let full = self.path.join(&name);
                let md = std::fs::symlink_metadata(&full).ok();
                let name = name.clone();
                self.window(ctx, "Properties", |ui, here| {
                    let kind = match &md {
                        Some(m) if m.file_type().is_symlink() => format!("link → {}", std::fs::read_link(&full).map(|p| p.display().to_string()).unwrap_or_default()),
                        Some(m) if m.is_dir() => "folder".into(),
                        Some(_) => if is_text(&full) { "text file".into() } else { "file".into() },
                        None => "gone".into(),
                    };
                    let size = match &md {
                        Some(m) if m.is_dir() => {
                            let n = std::fs::read_dir(&full).map(|d| d.count()).unwrap_or(0);
                            format!("{n} item{}", if n == 1 { "" } else { "s" })
                        }
                        Some(m) => format!("{}  ({} bytes)", human(m.len()), m.len()),
                        None => String::new(),
                    };
                    for (k, v) in [
                        ("Name", name.clone()),
                        ("Type", kind),
                        ("Size", size),
                        ("Location", here.display().to_string()),
                        ("Changed", when(md.as_ref().and_then(|m| m.modified().ok()))),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{k:<10}")).color(theme::overlay0()));
                            ui.label(RichText::new(v).color(theme::text()));
                        });
                    }
                    if ui.button("⧉ Copy location").clicked() {
                        wl_copy(&full.display().to_string());
                    }
                });
            }
        }
        // Esc closes any dialog
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            keep = false;
        }
        if keep {
            self.dialog = dialog;
        }
    }

    /// A small centred window shared by every dialog. The closure gets the
    /// folder we're in — never the app itself, so nothing can be borrowed
    /// twice while the dialog draws.
    fn window(&mut self, ctx: &egui::Context, title: &str, add: impl FnOnce(&mut egui::Ui, &Path)) {
        let mut closed = false;
        let here = self.path.clone();
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0))
            .frame(Frame::new().fill(theme::mantle()).inner_margin(Margin::same(14)).stroke(egui::Stroke::new(1.0_f32, theme::surface1())))
            .show(ctx, |ui| {
                ui.label(RichText::new(title).family(theme::bold()).color(theme::accent()));
                ui.add_space(6.0);
                add(ui, &here);
                ui.add_space(4.0);
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    closed = true;
                }
            });
        if closed {
            self.dialog = Dialog::None;
        }
    }
}

fn main() -> eframe::Result<()> {
    let start = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(home);
    let start = start.canonicalize().unwrap_or_else(|_| home());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura files")
            .with_app_id("aura-files")
            .with_inner_size([980.0, 620.0])
            .with_min_inner_size([560.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native("Aura files", options, Box::new(move |cc| Ok(Box::new(App::new(cc, start)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_sizes() {
        assert_eq!(human(999), "999 B");
        assert_eq!(human(1024), "1.0 KB");
        assert_eq!(human(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn copies_get_a_free_name() {
        let dir = std::env::temp_dir().join(format!("aura-files-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        assert_eq!(free_name(&dir, "notes.txt"), dir.join("notes (copy).txt"));
        std::fs::write(dir.join("notes (copy).txt"), "x").unwrap();
        assert_eq!(free_name(&dir, "notes.txt"), dir.join("notes (copy 2).txt"));
        assert_eq!(free_name(&dir, "fresh.txt"), dir.join("fresh.txt"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folders_copy_with_everything_inside() {
        let base = std::env::temp_dir().join(format!("aura-files-copy-{}", std::process::id()));
        let (from, to) = (base.join("from/deep"), base.join("to"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("a.txt"), "hello").unwrap();
        copy_dir(&base.join("from"), &to).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("deep/a.txt")).unwrap(), "hello");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn dates_are_readable() {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(when(Some(t)), "14 Nov 2023 22:13");
    }
}
