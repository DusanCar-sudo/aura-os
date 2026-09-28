//! Everything that talks to the compositor.
//!
//! This app drives **sway** through `swaymsg`, which is the only thing on this
//! machine that is allowed to move outputs around or change their mode. We
//! shell out rather than link against libsway because `swaymsg` is a stable,
//! documented interface and keeps the build dependency-free.

use serde::Deserialize;
use std::env;
use std::fs;
use std::process::Command;


// ────────────────────────────────────────────────────────────────────────────
// Socket auto-detection
// ────────────────────────────────────────────────────────────────────────────

/// If `SWAYSOCK` points at a dead socket (sway crashed/restarted), find the
/// newest live `sway-ipc.*.sock` for this user and point `SWAYSOCK` at it.
/// Called before every `swaymsg` invocation so the app always talks to the
/// current compositor, not a stale one.
fn fix_socket() {
    // If SWAYSOCK is set and still exists, trust it.
    if let Ok(sock) = env::var("SWAYSOCK") {
        if fs::metadata(&sock).is_ok() {
            return;
        }
    }
    // Find the newest sway IPC socket in XDG_RUNTIME_DIR (/run/user/<uid>).
    let dir = match env::var("XDG_RUNTIME_DIR") {
        Ok(d) => d,
        Err(_) => return,
    };
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
        if let Some((_, path)) = best {
            env::set_var("SWAYSOCK", &path);
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Modes
// ────────────────────────────────────────────────────────────────────────────

/// One video mode. `refresh` is in millihertz exactly as sway reports it, so
/// we can hand the very same number back without float rounding surprises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    pub refresh: i32,
}

impl Mode {
    /// `1920x1200@60.003Hz` — the exact string sway wants back.
    pub fn sway_arg(self) -> String {
        format!(
            "{}x{}@{}.{:03}Hz",
            self.width,
            self.height,
            self.refresh / 1000,
            self.refresh % 1000
        )
    }

    /// `1920x1200` — used as a fallback if sway rejects the exact refresh.
    pub fn sway_arg_loose(self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    pub fn size_label(self) -> String {
        format!("{} × {}", self.width, self.height)
    }

    /// `3840x2160` — compact, for the label block on a monitor tile.
    pub fn pixel_label(self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    /// `60 Hz`, `59.94 Hz`, `60.003 Hz` — trailing zeros trimmed.
    pub fn hz_label(self) -> String {
        let raw = format!("{}.{:03}", self.refresh / 1000, self.refresh % 1000);
        let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
        format!("{trimmed} Hz")
    }

    pub fn pixels(self) -> i64 {
        self.width as i64 * self.height as i64
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Transform (rotation / flipping)
// ────────────────────────────────────────────────────────────────────────────

const TRANSFORM_NAMES: [&str; 8] = [
    "normal",
    "90",
    "180",
    "270",
    "flipped",
    "flipped-90",
    "flipped-180",
    "flipped-270",
];

pub fn transform_name(t: u8) -> &'static str {
    TRANSFORM_NAMES[(t as usize) % 8]
}

/// sway 1.12 reports `transform` as a name ("normal", "90", "flipped-270"),
/// older builds reported the raw wl_output enum as an integer. Accept both.
fn transform_from_name(s: &str) -> Option<u8> {
    let norm = s.trim().to_ascii_lowercase().replace('_', "-");
    TRANSFORM_NAMES
        .iter()
        .position(|n| *n == norm)
        .map(|i| i as u8)
}

fn de_transform<'de, D>(d: D) -> Result<u8, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Name(String),
        Num(i64),
    }

    Ok(match Raw::deserialize(d)? {
        Raw::Num(n) => n.rem_euclid(8) as u8,
        Raw::Name(s) => transform_from_name(&s).unwrap_or(0),
    })
}

/// True when the output is on its side, so its logical width/height swap.
pub fn is_quarter_turned(t: u8) -> bool {
    matches!(t % 4, 1 | 3)
}

/// Step to the next rotation, keeping any mirroring the user already had.
pub fn next_transform(t: u8) -> u8 {
    if t < 4 {
        (t + 1) % 4
    } else {
        4 + (t - 4 + 1) % 4
    }
}

/// A friendly name for the rotation, for the UI.
pub fn transform_label(t: u8) -> &'static str {
    match t % 4 {
        1 => "90°",
        2 => "180°",
        3 => "270°",
        _ => "0°",
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Live vs. draft layout
// ────────────────────────────────────────────────────────────────────────────

/// The part of an output's setup the user can edit here.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub enabled: bool,
    pub x: i32,
    pub y: i32,
    pub scale: f32,
    pub transform: u8,
    pub mode: Mode,
}

/// A connected screen, with both the state the compositor is currently in and
/// the state the user is editing. Keeping both is what makes "Apply" and
/// "Revert" trivial.
#[derive(Debug, Clone)]
pub struct Display {
    pub name: String,
    pub make: String,
    pub model: String,
    pub focused: bool,
    pub workspace: Option<String>,
    /// Every mode the monitor advertises, untouched.
    pub modes: Vec<Mode>,
    pub live: Layout,
    pub draft: Layout,
}

impl Display {
    /// Size this screen takes up in the desktop's coordinate space, which is
    /// the mode size divided by the scale factor. Portrait rotation swaps it.
    pub fn logical_size(&self) -> (i32, i32) {
        let (w, h) = if is_quarter_turned(self.draft.transform) {
            (self.draft.mode.height, self.draft.mode.width)
        } else {
            (self.draft.mode.width, self.draft.mode.height)
        };
        let s = if self.draft.scale > 0.01 { self.draft.scale } else { 1.0 };
        (
            ((w as f32) / s).round().max(1.0) as i32,
            ((h as f32) / s).round().max(1.0) as i32,
        )
    }

    pub fn dirty(&self) -> bool {
        self.draft != self.live
    }

    /// A short, human name: the model if it looks like a real name, else the
    /// connector name.
    pub fn pretty_name(&self) -> String {
        let m = self.model.trim();
        if m.is_empty() || m.starts_with("0x") || m.eq_ignore_ascii_case("unknown") {
            self.name.clone()
        } else {
            m.to_string()
        }
    }

    /// Distinct resolutions this monitor can do, smallest first. Keeping only
    /// the fastest refresh per size is what makes the resolution dial readable.
    pub fn resolutions(&self) -> Vec<Mode> {
        let mut out: Vec<Mode> = Vec::new();
        for m in &self.modes {
            match out
                .iter_mut()
                .find(|e| e.width == m.width && e.height == m.height)
            {
                Some(e) => {
                    if m.refresh > e.refresh {
                        e.refresh = m.refresh;
                    }
                }
                None => out.push(*m),
            }
        }
        out.sort_by_key(|m| (m.pixels(), m.width));
        if out.is_empty() {
            out.push(self.draft.mode);
        }
        out
    }

    /// Every refresh rate available at one particular resolution.
    pub fn refresh_options(&self, w: i32, h: i32) -> Vec<Mode> {
        let mut out: Vec<Mode> = self
            .modes
            .iter()
            .copied()
            .filter(|m| m.width == w && m.height == h)
            .collect();
        out.sort_by_key(|m| m.refresh);
        out.dedup_by_key(|m| m.refresh);
        if out.is_empty() {
            out.push(self.draft.mode);
        }
        out
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Reading from sway
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
struct RawRect {
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    name: String,
    #[serde(default)]
    make: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    current_workspace: Option<String>,
    #[serde(default)]
    rect: RawRect,
    #[serde(default = "one")]
    scale: f32,
    #[serde(default, deserialize_with = "de_transform")]
    transform: u8,
    #[serde(default)]
    current_mode: Option<Mode>,
    #[serde(default)]
    modes: Vec<Mode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Sway,
    Unsupported,
}

/// Is there a compositor here we can actually talk to?
pub fn detect() -> (Backend, Option<String>) {
    fix_socket();
    match Command::new("swaymsg")
        .args(["-t", "get_version"])
        .output()
    {
        Ok(o) if o.status.success() => (Backend::Sway, None),
        Ok(_) => (
            Backend::Unsupported,
            Some("`swaymsg` is installed but isn't answering — this doesn't look like a sway session.".into()),
        ),
        Err(_) => (
            Backend::Unsupported,
            Some("Couldn't find `swaymsg`. This app arranges displays by talking to the sway compositor.".into()),
        ),
    }
}

/// The running compositor's version string, for the little badge in the header.
pub fn compositor_version() -> Option<String> {
    fix_socket();
    let out = Command::new("swaymsg").args(["-t", "get_version"]).output().ok()?;
    #[derive(Deserialize)]
    struct V {
        #[serde(default)]
        human_readable: String,
    }
    let v: V = serde_json::from_slice(&out.stdout).ok()?;
    (!v.human_readable.is_empty()).then_some(v.human_readable)
}

/// Ask sway for the current output topology.
pub fn query() -> Result<Vec<Display>, String> {
    fix_socket();
    let out = Command::new("swaymsg")
        .args(["-t", "get_outputs", "-r"])
        .output()
        .map_err(|e| format!("couldn't run swaymsg: {e}"))?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            "sway refused to report its outputs".into()
        } else {
            err
        });
    }

    let raw: Vec<RawOutput> =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("couldn't read sway's reply: {e}"))?;

    let mut displays: Vec<Display> = raw
        .into_iter()
        .filter(|o| !o.name.is_empty() && o.name != "__i3")
        .map(|o| {
            // A disabled output has no meaningful rect, so fall back to a sane
            // placeholder; `park_orphans` below tucks it next to the others.
            let mode = o
                .current_mode
                .or_else(|| o.modes.iter().copied().max_by_key(|m| (m.pixels(), m.refresh)))
                .unwrap_or(Mode { width: 1920, height: 1080, refresh: 60000 });

            let layout = Layout {
                enabled: o.active,
                x: o.rect.x,
                y: o.rect.y,
                scale: if o.scale > 0.01 { o.scale } else { 1.0 },
                transform: o.transform % 8,
                mode,
            };

            Display {
                name: o.name,
                make: o.make,
                model: o.model,
                focused: o.focused,
                workspace: o.current_workspace,
                modes: o.modes,
                live: layout.clone(),
                draft: layout,
            }
        })
        .collect();

    displays.sort_by_key(|d| (d.live.x, d.live.y, d.name.clone()));
    park_orphans(&mut displays);
    Ok(displays)
}

/// Disabled outputs (and outputs sway reported at a useless 0,0) get placed to
/// the right of everything else, so ghost tiles don't pile up in the corner.
fn park_orphans(displays: &mut [Display]) {
    let right = displays
        .iter()
        .filter(|d| d.live.enabled)
        .map(|d| d.live.x + d.logical_size().0)
        .max()
        .unwrap_or(0);

    let mut cursor = right;
    for d in displays.iter_mut() {
        if !d.live.enabled {
            let (w, _) = d.logical_size();
            d.live.x = cursor;
            d.live.y = 0;
            d.draft.x = cursor;
            d.draft.y = 0;
            cursor += w + 80;
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Writing back to sway
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawReply {
    #[serde(default)]
    success: bool,
    /// sway is inconsistent here: sometimes a string, sometimes `true`.
    #[serde(default)]
    parse_error: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<String>,
}

fn reply_error(r: &RawReply) -> String {
    if let Some(e) = &r.error {
        if !e.is_empty() {
            return e.clone();
        }
    }
    match &r.parse_error {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Bool(_)) | None => "sway couldn't parse that command".into(),
        Some(other) => other.to_string(),
    }
}

/// Run one sway command and turn swaymsg's JSON verdict into a Rust `Result`.
fn run_sway(cmd: &str) -> Result<(), String> {
    fix_socket();
    let out = Command::new("swaymsg")
        .arg(cmd)
        .output()
        .map_err(|e| format!("couldn't run swaymsg: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    match serde_json::from_str::<Vec<RawReply>>(stdout.trim()) {
        Ok(replies) => {
            if let Some(bad) = replies.iter().find(|r| !r.success) {
                return Err(reply_error(bad));
            }
            // An empty array means sway had nothing to say, which is a failure
            // in itself — don't let it pass for success.
            if replies.is_empty() {
                return Err("sway ignored the command".into());
            }
            return Ok(());
        }
        Err(e) => {
            // No parseable verdict: fall back to the process result.
            if out.status.success() && stderr.trim().is_empty() {
                return Err(format!("couldn't read sway's answer: {e}"));
            }
        }
    }

    let msg = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };
    Err(if msg.is_empty() {
        "sway didn't answer".into()
    } else {
        msg
    })
}

/// What happened when we tried to push the layout to the compositor.
#[derive(Debug, Default)]
pub struct Report {
    pub commands: Vec<String>,
    pub errors: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// One sway command, plus a looser version to try if sway turns the first down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub cmd: String,
    pub fallback: Option<String>,
}

impl Step {
    fn plain(cmd: String) -> Self {
        Self { cmd, fallback: None }
    }
}

/// Work out exactly which sway commands turn the live layout into the draft.
///
/// This is pure — no compositor is touched — which is what makes it testable.
/// Order matters: mode, scale and rotation all change how much room a screen
/// takes up, so they come first and the position always comes last.
pub fn plan(displays: &[Display]) -> Vec<Step> {
    let mut steps = Vec::new();

    for d in displays {
        if !d.dirty() {
            continue;
        }

        // Switching a screen off is one command, and nothing else about it
        // matters until it comes back.
        if !d.draft.enabled {
            if d.live.enabled {
                steps.push(Step::plain(format!("output {} disable", d.name)));
            }
            continue;
        }

        if !d.live.enabled {
            steps.push(Step::plain(format!("output {} enable", d.name)));
        }

        if d.draft.mode != d.live.mode || !d.live.enabled {
            steps.push(Step {
                cmd: format!("output {} mode {}", d.name, d.draft.mode.sway_arg()),
                // If sway won't take the exact refresh, let it pick the best
                // one it has for that resolution.
                fallback: Some(format!(
                    "output {} mode {}",
                    d.name,
                    d.draft.mode.sway_arg_loose()
                )),
            });
        }

        if (d.draft.scale - d.live.scale).abs() > 1e-4 {
            steps.push(Step::plain(format!(
                "output {} scale {}",
                d.name,
                trim_scale(d.draft.scale)
            )));
        }

        if d.draft.transform != d.live.transform {
            steps.push(Step::plain(format!(
                "output {} transform {}",
                d.name,
                transform_name(d.draft.transform)
            )));
        }

        if d.draft.x != d.live.x || d.draft.y != d.live.y || !d.live.enabled {
            // sway wants the two numbers separated by a space, not a comma.
            steps.push(Step::plain(format!(
                "output {} position {} {}",
                d.name, d.draft.x, d.draft.y
            )));
        }
    }

    steps
}

/// Push every changed output to sway.
pub fn apply(displays: &[Display]) -> Report {
    let mut report = Report::default();

    for step in plan(displays) {
        match run_sway(&step.cmd) {
            Ok(()) => report.commands.push(step.cmd),
            Err(first) => {
                let retried = step.fallback.as_ref().map(|alt| match run_sway(alt) {
                    Ok(()) => Ok(alt.clone()),
                    Err(e) => Err(e),
                });
                match retried {
                    Some(Ok(alt)) => report.commands.push(alt),
                    _ => report.errors.push(format!("{}  →  {first}", step.cmd)),
                }
            }
        }
    }

    report
}

fn trim_scale(s: f32) -> String {
    let t = format!("{s:.2}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    t.to_string()
}

// ────────────────────────────────────────────────────────────────────────────
// Desk helpers
// ────────────────────────────────────────────────────────────────────────────

/// Lay the enabled screens out in a tidy row, left to right, tops aligned.
pub fn auto_arrange(displays: &mut [Display]) {
    let mut order: Vec<usize> = (0..displays.len())
        .filter(|&i| displays[i].draft.enabled)
        .collect();
    order.sort_by_key(|&i| (displays[i].draft.x, displays[i].name.clone()));

    let mut x = 0;
    for i in order {
        let (w, _) = displays[i].logical_size();
        displays[i].draft.x = x;
        displays[i].draft.y = 0;
        x += w;
    }
}

/// Drop every edit and go back to what the compositor is actually doing.
pub fn revert(displays: &mut [Display]) {
    for d in displays.iter_mut() {
        d.draft = d.live.clone();
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(w: i32, h: i32, refresh: i32) -> Mode {
        Mode { width: w, height: h, refresh }
    }

    fn disp(m: Mode, scale: f32, transform: u8) -> Display {
        let l = Layout { enabled: true, x: 0, y: 0, scale, transform, mode: m };
        Display {
            name: "TEST-1".into(),
            make: String::new(),
            model: String::new(),
            focused: false,
            workspace: None,
            modes: vec![m],
            live: l.clone(),
            draft: l,
        }
    }

    #[test]
    fn sway_arg_is_exact() {
        // The refresh must round-trip to the very same millihertz value.
        assert_eq!(mode(1920, 1200, 60003).sway_arg(), "1920x1200@60.003Hz");
        assert_eq!(mode(3840, 2160, 30000).sway_arg(), "3840x2160@30.000Hz");
        assert_eq!(mode(1920, 1080, 59940).sway_arg(), "1920x1080@59.940Hz");
        assert_eq!(mode(1280, 720, 143981).sway_arg(), "1280x720@143.981Hz");
    }

    #[test]
    fn hz_label_trims_trailing_zeros() {
        assert_eq!(mode(1, 1, 60000).hz_label(), "60 Hz");
        assert_eq!(mode(1, 1, 60003).hz_label(), "60.003 Hz");
        assert_eq!(mode(1, 1, 59940).hz_label(), "59.94 Hz");
    }

    #[test]
    fn transform_names_round_trip() {
        for i in 0..8u8 {
            assert_eq!(transform_from_name(transform_name(i)), Some(i), "transform {i}");
        }
        assert_eq!(transform_from_name("flipped_90"), Some(5));
        assert_eq!(transform_from_name("  Normal "), Some(0));
        assert_eq!(transform_from_name("sideways"), None);
    }

    #[test]
    fn transform_accepts_strings_and_ints() {
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "de_transform")]
            transform: u8,
        }
        // sway 1.12 sends the name...
        let a: T = serde_json::from_str(r#"{"transform":"flipped-270"}"#).unwrap();
        assert_eq!(a.transform, 7);
        // ...older builds sent the raw enum.
        let b: T = serde_json::from_str(r#"{"transform":1}"#).unwrap();
        assert_eq!(b.transform, 1);
        // ...and a missing field is just "normal".
        let c: T = serde_json::from_str("{}").unwrap();
        assert_eq!(c.transform, 0);
    }

    #[test]
    fn logical_size_follows_scale_and_rotation() {
        let m = mode(3840, 2160, 60000);
        assert_eq!(disp(m, 1.0, 0).logical_size(), (3840, 2160));
        assert_eq!(disp(m, 2.0, 0).logical_size(), (1920, 1080));
        assert_eq!(disp(m, 1.0, 1).logical_size(), (2160, 3840));
        assert_eq!(disp(m, 1.0, 3).logical_size(), (2160, 3840));
        assert_eq!(disp(m, 2.0, 1).logical_size(), (1080, 1920));
        // 180° is not a quarter turn.
        assert_eq!(disp(m, 1.0, 2).logical_size(), (3840, 2160));
    }

    #[test]
    fn rotation_cycles_through_the_quarter_turns() {
        assert_eq!(next_transform(0), 1);
        assert_eq!(next_transform(3), 0);
        // Mirroring is preserved while rotating.
        assert_eq!(next_transform(4), 5);
        assert_eq!(next_transform(7), 4);
    }

    #[test]
    fn resolutions_are_deduped_and_ordered() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.modes = vec![
            mode(1920, 1080, 60000),
            mode(1920, 1080, 144000),
            mode(1280, 720, 60000),
            mode(3840, 2160, 30000),
        ];
        let sizes: Vec<(i32, i32)> = d.resolutions().iter().map(|m| (m.width, m.height)).collect();
        assert_eq!(sizes, vec![(1280, 720), (1920, 1080), (3840, 2160)]);
        // The fastest refresh survives for each size.
        assert_eq!(d.resolutions()[1].refresh, 144000);
    }

    #[test]
    fn refresh_options_filter_by_size() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.modes = vec![
            mode(1920, 1080, 60000),
            mode(1920, 1080, 50000),
            mode(1920, 1080, 60000),
            mode(1280, 720, 60000),
        ];
        let refs = d.refresh_options(1920, 1080);
        assert_eq!(refs.len(), 2, "duplicates should collapse");
        assert_eq!(refs[0].refresh, 50000);
        assert_eq!(refs[1].refresh, 60000);
    }

    #[test]
    fn pretty_name_prefers_a_real_model_name() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.name = "HDMI-A-1".into();
        d.model = "0x89C6".into();
        assert_eq!(d.pretty_name(), "HDMI-A-1", "hex model ids are useless");
        d.model = "SAMSUNG".into();
        assert_eq!(d.pretty_name(), "SAMSUNG");
    }

    #[test]
    fn auto_arrange_lays_enabled_screens_in_a_row() {
        let wide = mode(3840, 2160, 60000);
        let small = mode(1920, 1200, 60000);
        let mut a = disp(wide, 1.0, 0);
        let mut b = disp(small, 1.0, 0);
        a.draft.x = 500;
        b.draft.x = 0;
        b.draft.y = 900;
        let mut list = vec![a, b];
        auto_arrange(&mut list);
        // Sorted by current x: b (0) then a (500).
        assert_eq!((list[1].draft.x, list[1].draft.y), (0, 0));
        assert_eq!((list[0].draft.x, list[0].draft.y), (1920, 0));
    }

    #[test]
    fn dirty_tracks_edits() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        assert!(!d.dirty());
        d.draft.x += 10;
        assert!(d.dirty());
        revert(std::slice::from_mut(&mut d));
        assert!(!d.dirty());
    }

    fn cmds(displays: &[Display]) -> Vec<String> {
        plan(displays).into_iter().map(|s| s.cmd).collect()
    }

    #[test]
    fn plan_is_empty_when_nothing_changed() {
        let d = disp(mode(1920, 1080, 60000), 1.0, 0);
        assert!(plan(&[d]).is_empty());
    }

    #[test]
    fn plan_orders_mode_scale_before_position() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.draft.mode = mode(1280, 720, 60000);
        d.draft.scale = 2.0;
        d.draft.transform = 1;
        d.draft.x = 100;
        d.draft.y = 50;
        assert_eq!(
            cmds(&[d]),
            vec![
                "output TEST-1 mode 1280x720@60.000Hz",
                "output TEST-1 scale 2",
                "output TEST-1 transform 90",
                "output TEST-1 position 100 50",
            ]
        );
    }

    #[test]
    fn mode_steps_carry_a_loose_fallback() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.draft.mode = mode(1280, 720, 59940);
        let steps = plan(&[d]);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].cmd, "output TEST-1 mode 1280x720@59.940Hz");
        assert_eq!(
            steps[0].fallback.as_deref(),
            Some("output TEST-1 mode 1280x720")
        );
    }

    #[test]
    fn position_only_steps_have_no_fallback() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.draft.x = 1920;
        let steps = plan(&[d]);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].cmd, "output TEST-1 position 1920 0");
        assert!(steps[0].fallback.is_none());
    }

    #[test]
    fn disabling_only_emits_disable() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.draft.enabled = false;
        d.draft.x = 999;
        d.draft.scale = 2.0;
        assert_eq!(cmds(&[d]), vec!["output TEST-1 disable"]);
    }

    #[test]
    fn switching_a_screen_back_on_sets_everything_it_needs() {
        let mut d = disp(mode(1920, 1080, 60000), 1.0, 0);
        d.live.enabled = false;
        d.draft.enabled = true;
        assert_eq!(
            cmds(&[d]),
            vec![
                "output TEST-1 enable",
                "output TEST-1 mode 1920x1080@60.000Hz",
                "output TEST-1 position 0 0",
            ]
        );
    }

    #[test]
    fn one_bad_screen_does_not_stop_the_others() {
        let mut good = disp(mode(1920, 1080, 60000), 1.0, 0);
        good.name = "GOOD-1".into();
        good.draft.x = 1920;
        let mut bad = disp(mode(1280, 720, 60000), 1.0, 0);
        bad.name = "BAD-1".into();
        bad.draft.y = 40;
        let steps = plan(&[good, bad]);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].cmd, "output GOOD-1 position 1920 0");
        assert_eq!(steps[1].cmd, "output BAD-1 position 0 40");
    }

    /// Reads this machine's real compositor. Strictly read-only: it never
    /// issues a command, so it is safe to run at any time.
    #[test]
    fn reads_the_real_compositor_without_touching_it() {
        let (backend, note) = detect();
        if backend != Backend::Sway {
            eprintln!("skipping: no sway session here ({note:?})");
            return;
        }

        let before = query().expect("querying a live sway session should work");
        assert!(!before.is_empty(), "sway reported no outputs at all");

        for d in &before {
            assert!(!d.name.is_empty(), "an output with no name");
            assert!(d.draft.scale > 0.0, "{}: non-positive scale", d.name);
            assert!(d.draft.mode.width > 0 && d.draft.mode.height > 0, "{}: bad mode", d.name);
            assert!(
                d.modes.is_empty() || d.modes.contains(&d.draft.mode),
                "{}: current mode {} isn't in the advertised mode list",
                d.name,
                d.draft.mode.sway_arg()
            );
            // Whatever we read must be something we can also write back.
            assert!(transform_from_name(transform_name(d.draft.transform)).is_some());
        }

        // Nothing has been edited, so there is nothing to push to sway.
        assert!(
            plan(&before).is_empty(),
            "an untouched topology should need no commands, got {:?}",
            cmds(&before)
        );

        // And reading twice must not disturb anything.
        let after = query().expect("second query should work too");
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.live, b.live, "{} changed just from being read", a.name);
        }
    }
}
