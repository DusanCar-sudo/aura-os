//! aura-tree — the desktop file tree, a pane on the right edge.
//!
//! Folders expand with a click; a file opens with a double-click (a folder
//! double-click opens it in Files). Right-click copies the path. Home or /
//! as the root, hidden files on demand. Sway keeps it in the scratchpad:
//! aura-tree-toggle (Super+. or the top bar's tree button) slides it in and
//! out. Folders are read when opened, so a huge disk costs nothing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aura_ui::{icons, theme, widgets};
use egui::{vec2, Align2, Frame, Key, Margin, Rect, RichText, ScrollArea, Sense};

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
                let _ = Command::new("wl-copy").arg("--").arg(&n.path).stdin(Stdio::null()).status();
                self.status = "path copied".into();
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

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(10)))
            .show(ctx, |ui| {
                widgets::header(ui, "tree", "click opens · double-click launches");
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
