//! Desktop entries: what is installed, what it looks like, how it starts,
//! and which of them live in the right-click menu.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct App {
    /// The desktop id: file name without ".desktop" (dev.zed.Zed).
    pub id: String,
    pub name: String,
    pub exec: String,
    pub terminal: bool,
    pub icon: String,
    pub wm_class: String,
    pub comment: String,
    /// Shown in app menus (not NoDisplay, Hidden, or meant for another desktop).
    pub listed: bool,
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/root".into())
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("aura")
}

fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/state")).join("aura")
}

/// Where desktop entries and icons live, most important first: XDG order,
/// then flatpak's exports (in case the session never added them).
pub fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"))];
    let sys = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs.push(home().join(".local/share/flatpak/exports/share"));
    dirs.push("/var/lib/flatpak/exports/share".into());
    let mut seen = BTreeSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

fn on_path(cmd: &str) -> bool {
    if cmd.contains('/') {
        return Path::new(cmd).exists();
    }
    std::env::var("PATH").unwrap_or_default().split(':').any(|d| Path::new(d).join(cmd).exists())
}

fn parse(id: &str, text: &str) -> Option<App> {
    let mut in_entry = false;
    let mut kv: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            kv.entry(k.trim()).or_insert(v.trim());
        }
    }
    if kv.get("Type").copied().unwrap_or("Application") != "Application" {
        return None;
    }
    let exec = kv.get("Exec")?.to_string();
    let yes = |k: &str| kv.get(k).is_some_and(|v| *v == "true");
    let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "sway".into()).to_lowercase();
    let ours = |list: &str| list.split(';').any(|d| !d.is_empty() && desktops.split(':').any(|x| x == d.to_lowercase()));
    let listed = !yes("NoDisplay")
        && !yes("Hidden")
        && kv.get("OnlyShowIn").is_none_or(|l| ours(l))
        && !kv.get("NotShowIn").is_some_and(|l| ours(l))
        && kv.get("TryExec").is_none_or(|t| on_path(t));
    let get = |k: &str| kv.get(k).map(|v| v.to_string()).unwrap_or_default();
    Some(App {
        id: id.to_string(),
        name: kv.get("Name").map(|v| v.to_string()).unwrap_or_else(|| id.to_string()),
        exec,
        terminal: yes("Terminal"),
        icon: get("Icon"),
        wm_class: get("StartupWMClass"),
        comment: get("Comment"),
        listed,
    })
}

fn load(id: &str, path: &Path) -> Option<App> {
    parse(id, &fs::read_to_string(path).ok()?)
}

/// Desktop files under one applications/ dir (and one level of subdirs,
/// whose id joins the path with '-', as the spec says).
fn entries(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(list) = fs::read_dir(&dir) else { continue };
        for e in list.flatten() {
            let p = e.path();
            if p.is_dir() {
                if dir == root {
                    dirs.push(p);
                }
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("desktop") {
                continue;
            }
            if let Ok(rel) = p.strip_prefix(root) {
                let id = rel.with_extension("").to_string_lossy().replace('/', "-");
                out.push((id, p));
            }
        }
    }
    out
}

/// Every app a launcher would show, by name. The first file for an id
/// wins, so a hidden copy in ~/.local/share hides the system one too.
pub fn all_apps() -> Vec<App> {
    let mut claimed = BTreeSet::new();
    let mut apps = Vec::new();
    for dir in data_dirs() {
        for (id, path) in entries(&dir.join("applications")) {
            if !claimed.insert(id.clone()) {
                continue;
            }
            if let Some(app) = load(&id, &path).filter(|a| a.listed) {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

pub fn find(id: &str) -> Option<App> {
    data_dirs().iter().find_map(|d| {
        let p = d.join("applications").join(format!("{id}.desktop"));
        p.exists().then(|| load(id, &p)).flatten()
    })
}

/// The app a window belongs to: sway gives the Wayland app_id (or the X11
/// class); desktop files name it by id, StartupWMClass, or their command.
pub fn for_window(app_id: &str) -> Option<App> {
    if app_id.is_empty() {
        return None;
    }
    if let Some(app) = find(app_id) {
        return Some(app);
    }
    let want = app_id.to_lowercase();
    let apps = all_apps();
    let last = |id: &str| id.rsplit('.').next().unwrap_or(id).to_lowercase();
    let cmd = |a: &App| {
        let first = a.exec.split_whitespace().find(|t| !t.contains('=')).unwrap_or("");
        first.rsplit('/').next().unwrap_or("").to_lowercase()
    };
    apps.iter()
        .find(|a| a.id.to_lowercase() == want || a.wm_class.to_lowercase() == want)
        .or_else(|| apps.iter().find(|a| last(&a.id) == want || cmd(a) == want))
        .cloned()
}

fn strip_field_codes(exec: &str) -> String {
    let mut out = String::new();
    let mut it = exec.chars().peekable();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.peek() {
            Some('%') => {
                out.push('%');
                it.next();
            }
            Some(n) if "fFuUdDnNickvm".contains(*n) => {
                it.next();
            }
            _ => out.push(c),
        }
    }
    out.trim().to_string()
}

/// The shell command that starts an app (terminal apps inside aura-term).
pub fn command(app: &App) -> String {
    let cmd = strip_field_codes(&app.exec);
    if app.terminal {
        format!("aura-term -e {cmd}")
    } else {
        cmd
    }
}

/// Run a shell command on its own, outliving this menu.
pub fn spawn(cmd: &str) {
    let _ = Command::new("setsid")
        .args(["-f", "sh", "-c", cmd])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn launch(app: &App) {
    // the sidebar's "recent" list learns about it like any other launch
    let _ = Command::new("aura-os-recent")
        .args(["log", "a", &app.id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    spawn(&command(app));
}

pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

// ── icons ─────────────────────────────────────────────────────────────────

fn ini_value(path: &Path, key: &str) -> Option<String> {
    fs::read_to_string(path).ok()?.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

fn icon_roots(theme: &str) -> Vec<PathBuf> {
    let mut roots = vec![home().join(".icons").join(theme)];
    roots.extend(data_dirs().iter().map(|d| d.join("icons").join(theme)));
    roots.retain(|r| r.is_dir());
    roots
}

/// The icon theme and the ones it inherits from, ending at hicolor.
fn themes() -> &'static [String] {
    static CHAIN: OnceLock<Vec<String>> = OnceLock::new();
    CHAIN.get_or_init(|| {
        let mut chain: Vec<String> = Vec::new();
        let mut next = ini_value(&home().join(".config/gtk-3.0/settings.ini"), "gtk-icon-theme-name");
        while let Some(t) = next.take() {
            if chain.contains(&t) || chain.len() > 4 {
                break;
            }
            next = icon_roots(&t)
                .iter()
                .find_map(|r| ini_value(&r.join("index.theme"), "Inherits"))
                .and_then(|v| v.split(',').next().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty());
            chain.push(t);
        }
        if !chain.iter().any(|t| t == "hicolor") {
            chain.push("hicolor".into());
        }
        chain
    })
}

/// The file behind an Icon= value: a path, a themed icon, or a pixmap.
pub fn icon_file(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let p = Path::new(name);
    if p.is_absolute() {
        return p.exists().then(|| p.to_path_buf());
    }
    const SUBDIRS: &[&str] = &[
        "48x48/apps", "scalable/apps", "64x64/apps", "128x128/apps", "96x96/apps", "256x256/apps",
        "32x32/apps", "512x512/apps", "apps/48", "apps/64", "apps/scalable", "apps/32",
    ];
    for theme in themes() {
        for root in icon_roots(theme) {
            for sub in SUBDIRS {
                for ext in ["svg", "png"] {
                    let f = root.join(sub).join(format!("{name}.{ext}"));
                    if f.exists() {
                        return Some(f);
                    }
                }
            }
        }
    }
    data_dirs().iter().find_map(|d| {
        ["svg", "png"].iter().map(|ext| d.join("pixmaps").join(format!("{name}.{ext}"))).find(|f| f.exists())
    })
}

// ── the pinned list ─────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Pin {
    App(String),
    Cmd { label: String, cmd: String },
}

impl Pin {
    pub fn line(&self) -> String {
        match self {
            Pin::App(id) => id.clone(),
            Pin::Cmd { label, cmd } => format!("{label} = {cmd}"),
        }
    }
}

const HEADER: &str = "\
# Aura right-click menu: your apps, top to bottom, one per line.
# A desktop id (dev.zed.Zed), or  Label = shell command  for anything else.
# Change it here, with `aura-rclick pin <id>`, or right-click > Edit this menu.
";

/// Seeded with the developer tools that are installed, so the first
/// right-click already has something worth clicking.
const SEED: &[&str] = &["dev.zed.Zed", "code-oss", "code", "codium"];

fn pins_file() -> PathBuf {
    config_dir().join("rightclick.list")
}

pub fn read_pins() -> Vec<Pin> {
    match fs::read_to_string(pins_file()) {
        Ok(text) => text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| match l.split_once('=') {
                Some((label, cmd)) if !label.trim().is_empty() && !cmd.trim().is_empty() => {
                    Pin::Cmd { label: label.trim().into(), cmd: cmd.trim().into() }
                }
                _ => Pin::App(l.to_string()),
            })
            .collect(),
        Err(_) => {
            let seeded: Vec<Pin> = SEED.iter().filter(|id| find(id).is_some_and(|a| a.listed)).map(|id| Pin::App(id.to_string())).collect();
            write_pins(&seeded);
            seeded
        }
    }
}

pub fn write_pins(pins: &[Pin]) {
    let path = pins_file();
    let _ = fs::create_dir_all(config_dir());
    let mut text = HEADER.to_string();
    for p in pins {
        text.push_str(&p.line());
        text.push('\n');
    }
    let tmp = path.with_extension("list.tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

pub fn is_pinned(id: &str) -> bool {
    read_pins().contains(&Pin::App(id.to_string()))
}

pub fn pin(id: &str) -> bool {
    let mut pins = read_pins();
    let p = Pin::App(id.to_string());
    if pins.contains(&p) {
        return false;
    }
    pins.push(p);
    write_pins(&pins);
    true
}

pub fn unpin(id: &str) -> bool {
    let mut pins = read_pins();
    let before = pins.len();
    pins.retain(|p| *p != Pin::App(id.to_string()));
    if pins.len() == before {
        return false;
    }
    write_pins(&pins);
    true
}

// ── "a new app was installed" ───────────────────────────────────────────────

fn noask_file() -> PathBuf {
    config_dir().join("rightclick-noask")
}

pub fn asks() -> bool {
    !noask_file().exists()
}

pub fn set_asks(on: bool) {
    if on {
        let _ = fs::remove_file(noask_file());
    } else {
        let _ = fs::create_dir_all(config_dir());
        let _ = fs::write(noask_file(), "the right-click menu does not ask about new apps\n");
    }
}

fn seen_file() -> PathBuf {
    state_dir().join("rightclick-seen")
}

pub fn read_seen() -> Option<BTreeSet<String>> {
    let text = fs::read_to_string(seen_file()).ok()?;
    Some(text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

pub fn write_seen(ids: &BTreeSet<String>) {
    let _ = fs::create_dir_all(state_dir());
    let text: String = ids.iter().map(|id| format!("{id}\n")).collect();
    let tmp = seen_file().with_extension("tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, seen_file());
    }
}
