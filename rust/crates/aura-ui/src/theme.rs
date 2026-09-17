//! Palette and styling: flat, monospace, dressed the way the OS is dressed.
//!
//! Two sources, in order of preference:
//!
//! 1. **The aura theme** — `~/.config/aura/theme` names the active theme, and
//!    the palette is a flat `key=value` file at `<aura-share>/themes/<name>.conf`
//!    with seven colours: `BG`, `SURFACE`, `LINE`, `FG`, `DIM`, `ACCENT`,
//!    `ACCENT2`. That is the same file `aura-theme` itself reads, so the app
//!    tracks `violet-dusk`, `paper`, `neon-day` and friends exactly.
//! 2. **The OS light/dark preference**, via the XDG settings portal, dressed in
//!    Catppuccin Mocha or Latte.
//!
//! Either way the app ends up with one [`Palette`]. Nothing is decorative:
//! hairline borders, no shadows, no gradients, no animation. A screen is a
//! rectangle with a border, the way it would be drawn in a terminal.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};

use egui::epaint::Shadow;
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Visuals, vec2};

/// Every colour the app draws with, named by role rather than by hue — a theme
/// is free to make its accent orange, violet or white.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Palette {
    /// The panel background.
    pub base: Color32,
    /// The well the screens sit in, a shade below `base`.
    pub crust: Color32,
    /// Raised surfaces, like a screen that is switched off.
    pub mantle: Color32,
    /// A monitor tile.
    pub surface0: Color32,
    /// Hairline borders.
    pub surface1: Color32,
    /// The border of a live monitor tile.
    pub surface2: Color32,
    pub text: Color32,
    pub subtext0: Color32,
    /// Faint text: hints, units, the desk caption.
    pub overlay0: Color32,

    /// The one accent: window title, selection, the apply button, and the
    /// resolution arrow.
    pub accent: Color32,
    /// Second arrow — refresh.
    pub accent2: Color32,
    /// Third arrow — scale.
    pub accent3: Color32,
    /// The theme's own secondary colour, used for "you have unsaved edits".
    pub warn: Color32,
    /// A command that worked.
    pub ok: Color32,
    /// A command sway refused, or two screens sitting on top of each other.
    pub bad: Color32,

    /// `Visuals::dark()` or `Visuals::light()`.
    pub dark: bool,
}

/// The seven aura tokens, resolved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tokens {
    pub bg: Color32,
    pub surface: Color32,
    pub line: Color32,
    pub fg: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent2: Color32,
}

impl Tokens {
    /// Turn seven theme tokens into the roles the app draws with.
    ///
    /// The three arrows have to stay tellable apart no matter what the theme
    /// looks like, so they are the accent and the two colours 120° and 240°
    /// around the colour wheel from it. A theme with no hue to spin — `mono`,
    /// `paper` — falls back to the default triad instead.
    pub fn palette(&self) -> Palette {
        let dark = is_dark(self.bg);
        let fallback = if dark { Palette::DARK } else { Palette::LIGHT };

        let (h, s, l) = to_hsl(self.accent);
        let (a1, a2, a3) = if s < 0.15 {
            (fallback.accent, fallback.accent2, fallback.accent3)
        } else {
            (
                self.accent,
                from_hsl(h + 240.0, s, l),
                from_hsl(h + 120.0, s, l),
            )
        };

        Palette {
            base: self.bg,
            // Just enough to read as a recess, not so much that a light theme
            // turns muddy.
            crust: mix(self.bg, Color32::BLACK, if dark { 0.35 } else { 0.07 }),
            mantle: self.surface,
            surface0: self.surface,
            surface1: self.line,
            surface2: mix(self.line, self.fg, 0.35),
            text: self.fg,
            subtext0: mix(self.fg, self.dim, 0.5),
            overlay0: self.dim,
            accent: a1,
            accent2: a2,
            accent3: a3,
            warn: a3,
            ok: a2,
            bad: self.accent2,
            dark,
        }
    }
}

impl Palette {
    /// Catppuccin Mocha, for when there is no aura theme to follow.
    pub const DARK: Self = Self {
        base: Color32::from_rgb(0x1e, 0x1e, 0x2e),
        crust: Color32::from_rgb(0x11, 0x11, 0x1b),
        mantle: Color32::from_rgb(0x18, 0x18, 0x25),
        surface0: Color32::from_rgb(0x31, 0x32, 0x44),
        surface1: Color32::from_rgb(0x45, 0x47, 0x5a),
        surface2: Color32::from_rgb(0x58, 0x5b, 0x70),
        text: Color32::from_rgb(0xcd, 0xd6, 0xf4),
        subtext0: Color32::from_rgb(0xa6, 0xad, 0xc8),
        overlay0: Color32::from_rgb(0x6c, 0x70, 0x86),
        accent: Color32::from_rgb(0x89, 0xb4, 0xfa),  // blue
        accent2: Color32::from_rgb(0xa6, 0xe3, 0xa1), // green
        accent3: Color32::from_rgb(0xf5, 0xc2, 0xe7), // pink
        warn: Color32::from_rgb(0xf9, 0xe2, 0xaf),
        ok: Color32::from_rgb(0xa6, 0xe3, 0xa1),
        bad: Color32::from_rgb(0xf3, 0x8b, 0xa8),
        dark: true,
    };

    /// Catppuccin Latte.
    pub const LIGHT: Self = Self {
        base: Color32::from_rgb(0xef, 0xf1, 0xf5),
        crust: Color32::from_rgb(0xdc, 0xe0, 0xe8),
        mantle: Color32::from_rgb(0xe6, 0xe9, 0xef),
        surface0: Color32::from_rgb(0xcc, 0xd0, 0xda),
        surface1: Color32::from_rgb(0xbc, 0xc0, 0xcc),
        surface2: Color32::from_rgb(0xac, 0xb0, 0xbe),
        text: Color32::from_rgb(0x4c, 0x4f, 0x69),
        subtext0: Color32::from_rgb(0x6c, 0x6f, 0x85),
        overlay0: Color32::from_rgb(0x9c, 0xa0, 0xb0),
        accent: Color32::from_rgb(0x1e, 0x66, 0xf5),
        accent2: Color32::from_rgb(0x40, 0xa0, 0x2b),
        accent3: Color32::from_rgb(0xea, 0x76, 0xcb), // pink
        warn: Color32::from_rgb(0xdf, 0x8e, 0x1d),
        ok: Color32::from_rgb(0x40, 0xa0, 0x2b),
        bad: Color32::from_rgb(0xd2, 0x0f, 0x39),
        dark: false,
    };
}

// ────────────────────────────────────────────────────────────────────────────
// Colour maths
// ────────────────────────────────────────────────────────────────────────────

/// `#rrggbb`, with or without the hash and the surrounding quotes.
fn parse_hex(s: &str) -> Option<Color32> {
    let s = s.trim().trim_matches('"').trim_start_matches('#');
    if s.len() != 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color32::from_rgb(
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    ))
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// Hue in degrees, saturation and lightness in 0..=1.
fn to_hsl(c: Color32) -> (f32, f32, f32) {
    let r = c.r() as f32 / 255.0;
    let g = c.g() as f32 / 255.0;
    let b = c.b() as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / d) + 2.0)
    } else {
        60.0 * (((r - g) / d) + 4.0)
    };
    (h.rem_euclid(360.0), s.clamp(0.0, 1.0), l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Color32 {
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(to(r), to(g), to(b))
}

/// Perceived lightness, so `paper` reads as light and `violet-dusk` as dark.
fn is_dark(c: Color32) -> bool {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let luma = 0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b());
    luma < 0.5
}

// ────────────────────────────────────────────────────────────────────────────
// The aura theme system
// ────────────────────────────────────────────────────────────────────────────

/// One aura theme: seven colours, a radius, and a name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AuraTheme {
    /// The file stem, e.g. `violet-dusk`.
    pub name: String,
    /// The pretty name from `NAME=`, e.g. `Violet Dusk`.
    pub title: String,
    pub tokens: Tokens,
    /// Corner radius in points, from `RADIUS="4px"`.
    pub radius: u8,
}

impl AuraTheme {
    /// Parse the flat `key=value` format `aura-theme` writes.
    ///
    /// Unknown keys (`GLOW`, `WALLPAPER`, `FONT`, …) are ignored on purpose: the
    /// font is embedded in the binary, and the rest is not ours to draw.
    fn parse(text: &str, name: &str) -> Option<Self> {
        let get = |key: &str| -> Option<String> {
            text.lines()
                .filter_map(|line| line.split_once('='))
                .find(|(k, _)| k.trim() == key)
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
        };
        let colour = |key: &str| get(key).as_deref().and_then(parse_hex);

        let tokens = Tokens {
            bg: colour("BG")?,
            surface: colour("SURFACE")?,
            line: colour("LINE")?,
            fg: colour("FG")?,
            dim: colour("DIM")?,
            accent: colour("ACCENT")?,
            accent2: colour("ACCENT2")?,
        };

        let radius = get("RADIUS")
            .and_then(|r| {
                r.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u8>()
                    .ok()
            })
            .unwrap_or(0)
            .min(16);

        Some(Self {
            name: name.to_string(),
            title: get("NAME").unwrap_or_else(|| name.to_string()),
            tokens,
            radius,
        })
    }

    pub fn palette(&self) -> Palette {
        self.tokens.palette()
    }
}

fn aura_config_dir() -> Option<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("aura"))
}

/// Where the aura scripts live. Resolved the same way `lib/common.sh` does it:
/// `$AURA_SHARE`, else the directory two levels up from the `aura-theme` on
/// `PATH` (which is normally a symlink into `<share>/bin/`).
fn aura_share() -> Option<PathBuf> {
    if let Ok(s) = std::env::var("AURA_SHARE") {
        let p = PathBuf::from(s);
        if p.is_dir() {
            return Some(p);
        }
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        let candidate = dir.join("aura-theme");
        if !candidate.exists() {
            continue;
        }
        let real = std::fs::canonicalize(&candidate).ok()?;
        return real
            .parent()
            .and_then(|bin| bin.parent())
            .map(PathBuf::from);
    }
    None
}

/// The active aura theme, if this machine has one.
///
/// Follows `aura-theme`'s own search order: the user's override directory
/// first, then the shipped themes.
pub fn aura_theme() -> Option<AuraTheme> {
    let config = aura_config_dir()?;
    let name = std::fs::read_to_string(config.join("theme"))
        .ok()?
        .trim()
        .to_string();
    if name.is_empty() {
        return None;
    }

    let user = config.join("themes").join(format!("{name}.conf"));
    let path = if user.is_file() {
        user
    } else {
        aura_share()?.join("themes").join(format!("{name}.conf"))
    };

    AuraTheme::parse(&std::fs::read_to_string(path).ok()?, &name)
}

// ────────────────────────────────────────────────────────────────────────────
// Which theme are we wearing
// ────────────────────────────────────────────────────────────────────────────

/// What the OS says it wants, when there is no aura theme to ask.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scheme {
    Dark,
    Light,
}

impl Scheme {
    pub fn palette(self) -> Palette {
        match self {
            Scheme::Dark => Palette::DARK,
            Scheme::Light => Palette::LIGHT,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Scheme::Dark => "dark",
            Scheme::Light => "light",
        }
    }
}

/// The resolved theme: an aura theme if there is one, otherwise the OS
/// light/dark preference with a Catppuccin palette.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ThemeId {
    Aura(AuraTheme),
    Scheme(Scheme),
}

impl ThemeId {
    /// Work out what to wear.
    ///
    /// `AURA_OPTANOMAI_THEME=dark|light` pins the fallback scheme, which is
    /// handy for testing on a machine that does have an aura theme.
    pub fn detect() -> Self {
        if let Ok(v) = std::env::var("AURA_OPTANOMAI_THEME") {
            if let Some(s) = parse_scheme_override(&v) {
                return ThemeId::Scheme(s);
            }
        }
        match aura_theme() {
            Some(t) => ThemeId::Aura(t),
            None => ThemeId::Scheme(detect_scheme()),
        }
    }

    pub fn palette(&self) -> Palette {
        match self {
            ThemeId::Aura(t) => t.palette(),
            ThemeId::Scheme(s) => s.palette(),
        }
    }

    /// Corner radius, in points. The aura themes ask for 4–6px; the fallback is
    /// square, like a terminal.
    pub fn radius(&self) -> u8 {
        match self {
            ThemeId::Aura(t) => t.radius,
            ThemeId::Scheme(_) => 0,
        }
    }

    /// Shown in the status line when the theme changes.
    pub fn label(&self) -> String {
        match self {
            ThemeId::Aura(t) => t.title.clone(),
            ThemeId::Scheme(s) => s.label().to_string(),
        }
    }
}

/// Ask the OS which colour scheme it is using.
///
/// The XDG desktop portal is the canonical answer — it is what GTK, Qt and
/// every other toolkit reads, so it reflects whatever the user picked. The
/// other two are fallbacks for sessions with no portal running.
pub fn detect_scheme() -> Scheme {
    portal_scheme()
        .or_else(gsettings_scheme)
        .or_else(gtk_settings_scheme)
        .unwrap_or(Scheme::Dark)
}

fn parse_scheme_override(v: &str) -> Option<Scheme> {
    match v.trim().to_ascii_lowercase().as_str() {
        "dark" => Some(Scheme::Dark),
        "light" => Some(Scheme::Light),
        _ => None,
    }
}

/// `org.freedesktop.appearance color-scheme`, via the settings portal.
fn portal_scheme() -> Option<Scheme> {
    let out = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_portal_reply(&String::from_utf8_lossy(&out.stdout))
}

/// The settings portal answers with a variant, e.g. `(<<uint32 1>>,)`:
/// 0 = no preference, 1 = prefer dark, 2 = prefer light.
fn parse_portal_reply(reply: &str) -> Option<Scheme> {
    let digits: String = reply
        .split("uint32")
        .nth(1)?
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    match digits.parse::<u32>().ok()? {
        1 => Some(Scheme::Dark),
        2 => Some(Scheme::Light),
        _ => None,
    }
}

/// The GNOME/GTK setting most desktops write the same preference into.
fn gsettings_scheme() -> Option<Scheme> {
    let out = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).to_lowercase();
    if v.contains("prefer-dark") {
        Some(Scheme::Dark)
    } else if v.contains("prefer-light") {
        Some(Scheme::Light)
    } else {
        None
    }
}

/// Last resort: read the GTK settings file directly.
fn gtk_settings_scheme() -> Option<Scheme> {
    let config = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })?;

    for dir in ["gtk-4.0", "gtk-3.0"] {
        let Ok(text) = std::fs::read_to_string(config.join(dir).join("settings.ini")) else {
            continue;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() != "gtk-application-prefer-dark-theme" {
                continue;
            }
            let v = value.trim();
            if v == "1" || v.eq_ignore_ascii_case("true") {
                return Some(Scheme::Dark);
            }
            if v == "0" || v.eq_ignore_ascii_case("false") {
                return Some(Scheme::Light);
            }
        }
    }
    None
}

// ────────────────────────────────────────────────────────────────────────────
// Live palette
// ────────────────────────────────────────────────────────────────────────────

static PALETTE: RwLock<Palette> = RwLock::new(Palette::DARK);
static RADIUS: AtomicU8 = AtomicU8::new(0);

fn p() -> Palette {
    *PALETTE.read().unwrap_or_else(|e| e.into_inner())
}

/// Corner radius for rounded rectangles, from the theme.
pub fn radius() -> CornerRadius {
    CornerRadius::same(RADIUS.load(Ordering::Relaxed))
}

// Token accessors, so a call site can just say `theme::accent()` and get the
// colour for whatever the OS is wearing.

pub fn base() -> Color32 {
    p().base
}
pub fn crust() -> Color32 {
    p().crust
}
pub fn mantle() -> Color32 {
    p().mantle
}
pub fn surface0() -> Color32 {
    p().surface0
}
pub fn surface1() -> Color32 {
    p().surface1
}
pub fn surface2() -> Color32 {
    p().surface2
}
pub fn text() -> Color32 {
    p().text
}
pub fn subtext0() -> Color32 {
    p().subtext0
}
pub fn overlay0() -> Color32 {
    p().overlay0
}
pub fn accent() -> Color32 {
    p().accent
}
pub fn accent2() -> Color32 {
    p().accent2
}
pub fn accent3() -> Color32 {
    p().accent3
}
/// Success: an apply that worked.
pub fn ok() -> Color32 {
    p().ok
}
/// Failure: a rejected command, or two screens overlapping.
pub fn bad() -> Color32 {
    p().bad
}
pub fn warn() -> Color32 {
    p().warn
}

/// Everything is monospace, like a terminal.
pub const MONO: FontFamily = FontFamily::Monospace;

/// The same colour at a lower alpha, for hairlines that shouldn't shout.
pub fn dim(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

// ────────────────────────────────────────────────────────────────────────────
// Fonts
// ────────────────────────────────────────────────────────────────────────────

/// The official JetBrains Mono, embedded so the app renders identically on
/// every machine. Patched "Nerd Font" builds and whatever `fc-match monospace`
/// happens to resolve to both drift from these metrics.
///
/// SIL Open Font License 1.1 — see `assets/OFL.txt`.
static REGULAR: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../assets/JetBrainsMono-Bold.ttf");

/// egui has no notion of a font weight, so the heavier face is just another
/// family you can ask for by name.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Swap in JetBrains Mono, keeping egui's bundled fonts behind it so any glyph
/// it doesn't cover degrades to something rather than a tofu box.
///
/// The aura themes name a font stack too, but `paper` asks for IBM Plex Sans —
/// a proportional face, which would wreck a layout built on a grid. The
/// embedded monospace wins.
///
/// `AURA_OPTANOMAI_FONT=/path/to/font.ttf` replaces the regular face. Call this
/// once at startup.
pub fn install_fonts(ctx: &egui::Context) {
    let regular = match std::env::var("AURA_OPTANOMAI_FONT") {
        Ok(path) => match std::fs::read(&path) {
            Ok(bytes) => egui::FontData::from_owned(bytes),
            Err(e) => {
                eprintln!("aura-optanomai: cannot read {path}: {e}; using the bundled font");
                egui::FontData::from_static(REGULAR)
            }
        },
        Err(_) => egui::FontData::from_static(REGULAR),
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("jetbrains".to_owned(), Arc::new(regular));
    fonts.font_data.insert(
        "jetbrains-bold".to_owned(),
        Arc::new(egui::FontData::from_static(BOLD)),
    );

    for family in [FontFamily::Monospace, FontFamily::Proportional] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "jetbrains".to_owned());
    }

    // Bold first, then the whole normal chain so the emoji fallbacks survive.
    let mut heavy = vec!["jetbrains-bold".to_owned()];
    heavy.extend(
        fonts
            .families
            .get(&FontFamily::Monospace)
            .cloned()
            .unwrap_or_default(),
    );
    fonts.families.insert(bold(), heavy);

    ctx.set_fonts(fonts);
}

// ────────────────────────────────────────────────────────────────────────────
// Styling
// ────────────────────────────────────────────────────────────────────────────

/// Install the flat look for `theme` on a context, and remember the palette
/// for the token accessors above.
pub fn apply(ctx: &egui::Context, theme: &ThemeId) {
    let pal = theme.palette();
    *PALETTE.write().unwrap_or_else(|e| e.into_inner()) = pal;
    RADIUS.store(theme.radius(), Ordering::Relaxed);

    let mut style = (*ctx.style()).clone();

    style.text_styles = [
        (TextStyle::Heading, FontId::new(15.0, MONO)),
        (TextStyle::Body, FontId::new(13.0, MONO)),
        (TextStyle::Button, FontId::new(13.0, MONO)),
        (TextStyle::Small, FontId::new(11.0, MONO)),
        (TextStyle::Monospace, FontId::new(13.0, MONO)),
    ]
    .into();

    let mut v = if pal.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = pal.base;
    v.window_fill = pal.mantle;
    v.faint_bg_color = pal.surface0;
    v.extreme_bg_color = pal.crust;
    v.override_text_color = Some(pal.text);
    v.selection.bg_fill = pal.surface1;
    v.selection.stroke = Stroke::new(1.0_f32, pal.accent);
    v.window_corner_radius = radius();
    v.window_stroke = Stroke::new(1.0_f32, pal.surface1);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius();
        w.bg_fill = pal.surface0;
        w.weak_bg_fill = pal.surface0;
        w.bg_stroke = Stroke::new(1.0_f32, pal.surface1);
        w.fg_stroke = Stroke::new(1.0_f32, pal.text);
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.subtext0);
    v.widgets.hovered.bg_fill = pal.surface1;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.overlay0);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, pal.text);
    v.widgets.active.bg_fill = pal.surface1;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, pal.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, pal.accent);
    v.widgets.open.bg_fill = pal.surface1;
    v.widgets.open.fg_stroke = Stroke::new(1.0_f32, pal.text);

    style.visuals = v;
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(10.0, 4.0);
    style.spacing.window_margin = Margin::same(0);
    style.spacing.interact_size.y = 24.0;
    // Terminals don't ease.
    style.animation_time = 0.0;

    ctx.set_style(style);
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte the shape aura-theme writes.
    const VIOLET_DUSK: &str = r##"NAME="Violet Dusk"
BG="#0b0a14"
SURFACE="#131126"
LINE="#262247"
FG="#ded9f2"
DIM="#7c76a6"
ACCENT="#a78bfa"
ACCENT2="#f0abfc"
GLOW="rgba(167,139,250,0.32)"
FONT="JetBrains Mono, DejaVu Sans Mono, monospace"
RADIUS="4px"
WALLPAPER="ados-horizon-dark.png"
"##;

    const PAPER: &str = r##"NAME="Paper"
BG="#faf9f5"
SURFACE="#ffffff"
LINE="#e2ded2"
FG="#141413"
DIM="#76736b"
ACCENT="#1a1a1a"
ACCENT2="#8a8681"
RADIUS="6px"
"##;

    const MONO: &str = r##"NAME="Mono"
BG="#050505"
SURFACE="#101010"
LINE="#242424"
FG="#f0f0f0"
DIM="#8a8a8a"
ACCENT="#ffffff"
ACCENT2="#bdbdbd"
RADIUS="4px"
"##;

    fn hue_gap(a: Color32, b: Color32) -> f32 {
        let (ha, _, _) = to_hsl(a);
        let (hb, _, _) = to_hsl(b);
        let d = (ha - hb).abs() % 360.0;
        d.min(360.0 - d)
    }

    #[test]
    fn parses_the_aura_theme_format() {
        let t = AuraTheme::parse(VIOLET_DUSK, "violet-dusk").expect("should parse");
        assert_eq!(t.name, "violet-dusk");
        assert_eq!(t.title, "Violet Dusk");
        assert_eq!(t.radius, 4);
        assert_eq!(t.tokens.bg, Color32::from_rgb(0x0b, 0x0a, 0x14));
        assert_eq!(t.tokens.accent, Color32::from_rgb(0xa7, 0x8b, 0xfa));
        assert_eq!(t.tokens.accent2, Color32::from_rgb(0xf0, 0xab, 0xfc));
    }

    #[test]
    fn a_theme_missing_a_token_is_rejected() {
        // No ACCENT2 — better to fall back than to paint half a theme.
        let broken = VIOLET_DUSK.replace("ACCENT2=\"#f0abfc\"\n", "");
        assert!(AuraTheme::parse(&broken, "broken").is_none());
    }

    #[test]
    fn hex_parsing_is_strict() {
        assert_eq!(parse_hex("#0b0a14"), Some(Color32::from_rgb(11, 10, 20)));
        assert_eq!(parse_hex("\"#FFFFFF\""), Some(Color32::WHITE));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex("rgb(1,2,3)"), None);
    }

    #[test]
    fn dark_and_light_are_told_apart() {
        let dusk = AuraTheme::parse(VIOLET_DUSK, "violet-dusk").unwrap();
        let paper = AuraTheme::parse(PAPER, "paper").unwrap();
        assert!(dusk.palette().dark, "violet-dusk should read as dark");
        assert!(!paper.palette().dark, "paper should read as light");
    }

    #[test]
    fn theme_colours_reach_the_palette() {
        let pal = AuraTheme::parse(VIOLET_DUSK, "violet-dusk")
            .unwrap()
            .palette();
        assert_eq!(pal.base, Color32::from_rgb(0x0b, 0x0a, 0x14));
        assert_eq!(pal.text, Color32::from_rgb(0xde, 0xd9, 0xf2));
        assert_eq!(pal.overlay0, Color32::from_rgb(0x7c, 0x76, 0xa6));
        assert_eq!(pal.surface1, Color32::from_rgb(0x26, 0x22, 0x47));
        // The accent is the theme's own, so the app is actually violet.
        assert_eq!(pal.accent, Color32::from_rgb(0xa7, 0x8b, 0xfa));
        // The well is a recess below the background.
        assert!(is_dark(pal.crust) && pal.crust != pal.base);
    }

    #[test]
    fn the_three_arrows_are_always_tellable_apart() {
        // One sample per shipped aura theme, chromatic or not.
        let sources = [
            VIOLET_DUSK,
            PAPER,
            MONO,
            r##"NAME="Ember"
BG="#0c0705"
SURFACE="#1a0f08"
LINE="#2e1c10"
FG="#f5e6d8"
DIM="#9a7f68"
ACCENT="#ff7a3c"
ACCENT2="#ffd34d"
RADIUS="4px"
"##,
            r##"NAME="Ocean"
BG="#030812"
SURFACE="#08182e"
LINE="#123055"
FG="#d6ecff"
DIM="#6f93b8"
ACCENT="#2f9bff"
ACCENT2="#3be8c8"
RADIUS="4px"
"##,
            r##"NAME="Sunset"
BG="#150a10"
SURFACE="#241019"
LINE="#3d1c2a"
FG="#ffe9e0"
DIM="#b08a8a"
ACCENT="#ff7e5f"
ACCENT2="#ffc46b"
RADIUS="4px"
"##,
        ];
        for src in sources {
            let t = AuraTheme::parse(src, "t").unwrap();
            let pal = t.palette();
            let name = &t.title;
            for (a, b) in [
                (pal.accent, pal.accent2),
                (pal.accent, pal.accent3),
                (pal.accent2, pal.accent3),
            ] {
                assert_ne!(a, b, "{name}: two arrows are the same colour");
                assert!(
                    hue_gap(a, b) > 60.0,
                    "{name}: arrows {a:?} and {b:?} are only {}° apart",
                    hue_gap(a, b)
                );
            }
        }
    }

    #[test]
    fn the_fallback_triads_are_well_spread_too() {
        // This is what caught blue and mauve being only ~50 deg apart: on a
        // single line, two arrows that close are impossible to tell apart.
        for pal in [Palette::DARK, Palette::LIGHT] {
            for (a, b) in [
                (pal.accent, pal.accent2),
                (pal.accent, pal.accent3),
                (pal.accent2, pal.accent3),
            ] {
                assert!(
                    hue_gap(a, b) > 60.0,
                    "fallback arrows {a:?} and {b:?} are only {} deg apart",
                    hue_gap(a, b)
                );
            }
        }
    }

    #[test]
    fn a_monochrome_theme_falls_back_to_the_default_triad() {
        let pal = AuraTheme::parse(MONO, "mono").unwrap().palette();
        // A white accent has no hue to spin, so nothing should be invented:
        // the surfaces come from the theme, the triad from the fallback.
        assert_eq!(pal.base, Color32::from_rgb(0x05, 0x05, 0x05));
        assert_eq!(pal.accent, Palette::DARK.accent);
        assert_eq!(pal.accent2, Palette::DARK.accent2);
        assert_eq!(pal.accent3, Palette::DARK.accent3);
    }

    #[test]
    fn a_light_theme_gets_the_light_triad() {
        let pal = AuraTheme::parse(PAPER, "paper").unwrap().palette();
        // Paper is light, so the light fallback triad applies...
        assert_eq!(pal.accent, Palette::LIGHT.accent);
        // ...but the surfaces are paper's, not Latte's.
        assert_eq!(pal.base, Color32::from_rgb(0xfa, 0xf9, 0xf5));
        assert_eq!(pal.surface0, Color32::WHITE);
        assert_eq!(pal.text, Color32::from_rgb(0x14, 0x14, 0x13));
    }

    #[test]
    fn hsl_round_trips() {
        for c in [
            Color32::from_rgb(0xa7, 0x8b, 0xfa),
            Color32::from_rgb(0x2f, 0x9b, 0xff),
            Color32::from_rgb(0xff, 0x7a, 0x3c),
            Color32::from_rgb(0xde, 0xd9, 0xf2),
        ] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            let close = |a: u8, b: u8| (a as i32 - b as i32).abs() <= 1;
            assert!(
                close(c.r(), back.r()) && close(c.g(), back.g()) && close(c.b(), back.b()),
                "{c:?} -> {back:?}"
            );
        }
    }

    #[test]
    fn rotates_hue_without_touching_saturation() {
        let base = Color32::from_rgb(0xa7, 0x8b, 0xfa);
        let (h0, s0, l0) = to_hsl(base);
        let spun = from_hsl(h0 + 120.0, s0, l0);
        let (_, s1, l1) = to_hsl(spun);
        assert!((s1 - s0).abs() < 0.02, "saturation moved: {s0} -> {s1}");
        assert!((l1 - l0).abs() < 0.02, "lightness moved: {l0} -> {l1}");
        assert!((hue_gap(base, spun) - 120.0).abs() < 2.0);
    }

    #[test]
    fn parses_the_portal_reply() {
        assert_eq!(parse_portal_reply("(<<uint32 1>>,)"), Some(Scheme::Dark));
        assert_eq!(parse_portal_reply("(<<uint32 2>>,)"), Some(Scheme::Light));
        // 0 means "no preference", so we fall through to the other sources.
        assert_eq!(parse_portal_reply("(<<uint32 0>>,)"), None);
        assert_eq!(parse_portal_reply("()"), None);
        assert_eq!(parse_portal_reply("Error: something went wrong"), None);
        assert_eq!(parse_portal_reply(""), None);
    }

    #[test]
    fn parses_the_env_override() {
        assert_eq!(parse_scheme_override("dark"), Some(Scheme::Dark));
        assert_eq!(parse_scheme_override("LIGHT"), Some(Scheme::Light));
        assert_eq!(parse_scheme_override(" light "), Some(Scheme::Light));
        assert_eq!(parse_scheme_override("violet-dusk"), None);
    }

    #[test]
    fn the_fallback_palettes_are_actually_opposite() {
        assert!(Scheme::Dark.palette().dark);
        assert!(!Scheme::Light.palette().dark);

        let luma = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        assert!(luma(Palette::LIGHT.base) > luma(Palette::DARK.base));
        for pal in [Palette::DARK, Palette::LIGHT] {
            let gap = (luma(pal.text) as i32 - luma(pal.base) as i32).abs();
            assert!(gap > 200, "text/base contrast is too low: {pal:?}");
            let gap = (luma(pal.accent) as i32 - luma(pal.base) as i32).abs();
            assert!(gap > 100, "accent/base contrast is too low: {pal:?}");
            assert_ne!(pal.surface0, pal.crust);
        }
    }

    #[test]
    fn the_embedded_fonts_are_real_truetype_files() {
        for (name, bytes) in [("Regular", REGULAR), ("Bold", BOLD)] {
            assert!(bytes.len() > 50_000, "{name} is suspiciously small");
            assert_eq!(
                &bytes[..4],
                &[0x00, 0x01, 0x00, 0x00],
                "{name} is not a TrueType file"
            );
        }
        assert_ne!(REGULAR.len(), BOLD.len());
    }

    #[test]
    fn bold_is_a_separate_family() {
        assert_ne!(bold(), FontFamily::Monospace);
        assert_ne!(bold(), FontFamily::Proportional);
    }

    /// Reads this machine's real aura theme. Strictly read-only.
    #[test]
    fn reads_the_real_aura_theme() {
        match aura_theme() {
            Some(t) => {
                eprintln!("aura theme: {} ({}) radius {}", t.title, t.name, t.radius);
                let pal = t.palette();
                assert_ne!(pal.base, pal.text, "{}: no contrast", t.title);
                assert_ne!(
                    pal.accent, pal.accent2,
                    "{}: the first two arrows are the same colour",
                    t.title
                );
            }
            None => eprintln!("skipping: no aura theme on this machine"),
        }
    }

    #[test]
    fn detects_a_theme_one_way_or_the_other() {
        let id = ThemeId::detect();
        eprintln!("wearing: {} (radius {})", id.label(), id.radius());
        let pal = id.palette();
        assert_ne!(pal.base, pal.text);
        assert_ne!(pal.accent, pal.accent2);
    }
}
