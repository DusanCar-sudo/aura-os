//! aura-calc — a calculator in one small window.
//!
//! Type an expression or click the keys; Enter gives the answer, which
//! lands in the history below (click a line to reuse its result). Knows
//! + − × ÷, %, ^, parentheses and decimals. Follows the aura theme live.

use aura_ui::{theme, widgets};
use egui::{vec2, Align, Frame, Key, Layout, Margin, RichText, ScrollArea, Sense, Stroke};

// ── the math ────────────────────────────────────────────────────────────────

/// Evaluate `src`: + - * / % ^ ( ) with the usual precedence, unary minus,
/// and × ÷ − accepted as aliases. `50%` alone means 0.5.
fn eval(src: &str) -> Result<f64, String> {
    let s: Vec<char> = src
        .chars()
        .map(|c| match c {
            '×' | 'x' => '*',
            '÷' => '/',
            '−' => '-',
            ',' => '.',
            c => c,
        })
        .filter(|c| !c.is_whitespace())
        .collect();
    if s.is_empty() {
        return Err(String::new());
    }
    let mut p = Parser { s, i: 0 };
    let v = p.sum()?;
    if p.i < p.s.len() {
        return Err(format!("unexpected '{}'", p.s[p.i]));
    }
    if !v.is_finite() {
        return Err("can't divide by zero".into());
    }
    Ok(v)
}

struct Parser {
    s: Vec<char>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn sum(&mut self) -> Result<f64, String> {
        let mut v = self.product()?;
        while let Some(c @ ('+' | '-')) = self.peek() {
            self.i += 1;
            let r = self.product()?;
            v = if c == '+' { v + r } else { v - r };
        }
        Ok(v)
    }
    fn product(&mut self) -> Result<f64, String> {
        let mut v = self.power()?;
        while let Some(c @ ('*' | '/')) = self.peek() {
            self.i += 1;
            let r = self.power()?;
            v = if c == '*' { v * r } else { v / r };
        }
        Ok(v)
    }
    fn power(&mut self) -> Result<f64, String> {
        let b = self.unary()?;
        if self.peek() == Some('^') {
            self.i += 1;
            return Ok(b.powf(self.power()?)); // right-associative
        }
        Ok(b)
    }
    fn unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some('-') => { self.i += 1; Ok(-self.unary()?) }
            Some('+') => { self.i += 1; self.unary() }
            _ => self.postfix(),
        }
    }
    fn postfix(&mut self) -> Result<f64, String> {
        let mut v = self.atom()?;
        while self.peek() == Some('%') {
            self.i += 1;
            v /= 100.0;
        }
        Ok(v)
    }
    fn atom(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some('(') => {
                self.i += 1;
                let v = self.sum()?;
                if self.peek() != Some(')') {
                    return Err("missing )".into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.i;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '.') {
                    self.i += 1;
                }
                let t: String = self.s[start..self.i].iter().collect();
                t.parse().map_err(|_| format!("bad number '{t}'"))
            }
            Some(c) => Err(format!("unexpected '{c}'")),
            None => Err("unfinished".into()),
        }
    }
}

/// 0.1+0.2 shows as 0.3, big and tiny numbers stay readable.
fn fmt(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.abs() >= 1e15 || v.abs() < 1e-9 {
        return format!("{v:e}");
    }
    let s = format!("{:.10}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

// ── speech → expression ─────────────────────────────────────────────────────

fn small_number(w: &str) -> Option<f64> {
    const ONES: [&str; 20] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
    ];
    const TENS: [&str; 8] = ["twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];
    if let Some(i) = ONES.iter().position(|o| *o == w) {
        return Some(i as f64);
    }
    TENS.iter().position(|t| *t == w).map(|i| (i as f64 + 2.0) * 10.0)
}

/// "twelve times seven" → "12×7", "15 percent of 200" → "15%×200".
/// Digits and symbols Whisper already wrote pass straight through.
fn spoken_to_expr(said: &str) -> String {
    let lower = said.to_lowercase().replace('-', " ");
    let mut clean = String::new();
    for c in lower.chars() {
        // keep digits/operators glued, split everything else into words
        if c.is_alphanumeric() || c == '.' || c == ',' || c == ' ' {
            clean.push(c);
        } else if "+*/×÷−^%()=x".contains(c) {
            clean.push(' ');
            clean.push(c);
            clean.push(' ');
        } else {
            clean.push(' ');
        }
    }
    let words: Vec<&str> = clean.split_whitespace().collect();

    let mut out = String::new();
    // a number being spelled out: "two hundred thirty four" → 234
    let (mut total, mut cur, mut in_num) = (0.0_f64, 0.0_f64, false);
    let mut decimals: Option<String> = None;
    let flush = |out: &mut String, total: &mut f64, cur: &mut f64, in_num: &mut bool, dec: &mut Option<String>| {
        if *in_num {
            out.push_str(&fmt(*total + *cur));
            if let Some(d) = dec.take() {
                out.push('.');
                out.push_str(&d);
            }
        }
        *total = 0.0;
        *cur = 0.0;
        *in_num = false;
    };

    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let next = words.get(i + 1).copied().unwrap_or("");
        if let Some(d) = decimals.as_mut() {
            if let Some(n) = small_number(w).filter(|n| *n < 10.0) {
                d.push_str(&fmt(n));
                i += 1;
                continue;
            }
        }
        if let Some(n) = small_number(w) {
            cur += n;
            in_num = true;
        } else if w == "hundred" && in_num {
            cur = cur.max(1.0) * 100.0;
        } else if (w == "thousand" || w == "million") && in_num {
            total += cur.max(1.0) * if w == "thousand" { 1e3 } else { 1e6 };
            cur = 0.0;
        } else if (w == "point" || w == "dot") && in_num {
            decimals = Some(String::new());
        } else if w == "and" && in_num && small_number(next).is_some() {
            // "one hundred and five"
        } else {
            flush(&mut out, &mut total, &mut cur, &mut in_num, &mut decimals);
            let op = match (w, next) {
                ("plus" | "add" | "+", _) => "+",
                ("minus" | "subtract" | "less" | "negative", _) => "−",
                ("times" | "x" | "×" | "*", _) => "×",
                ("multiplied" | "multiply", _) => { if next == "by" { i += 1; } "×" }
                ("divided" | "divide", _) => { if next == "by" { i += 1; } "÷" }
                ("over" | "/" | "÷", _) => "÷",
                ("percent" | "%", "of") => { i += 1; "%×" }
                ("percent" | "%", _) => "%",
                ("squared", _) => "^2",
                ("cubed", _) => "^3",
                ("to", "the") if words.get(i + 2) == Some(&"power") => { i += 3; if words.get(i) == Some(&"of") { i += 1; } out.push('^'); continue; }
                ("power" | "^", _) => { if next == "of" { i += 1; } "^" }
                ("open" | "(", _) => { if matches!(next, "bracket" | "brackets" | "parenthesis") { i += 1; } "(" }
                ("close" | ")", _) => { if matches!(next, "bracket" | "brackets" | "parenthesis") { i += 1; } ")" }
                (t, _) if t.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.') => {
                    out.push_str(&t.replace(',', ""));
                    i += 1;
                    continue;
                }
                _ => "", // "what's", "is", "equals", "of"… ignored
            };
            out.push_str(op);
        }
        i += 1;
    }
    flush(&mut out, &mut total, &mut cur, &mut in_num, &mut decimals);
    out
}

/// Runs aura-calc-listen with `arg`, returns its stdout (or stderr as Err).
fn listen(arg: &str) -> Result<String, String> {
    let o = std::process::Command::new("aura-calc-listen")
        .arg(arg)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|_| "aura-calc-listen is not installed".to_string())?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mic {
    Idle,
    Recording,
    Thinking,
}

// ── the window ──────────────────────────────────────────────────────────────

const KEYS: [[&str; 4]; 5] = [
    ["C", "(", ")", "÷"],
    ["7", "8", "9", "×"],
    ["4", "5", "6", "−"],
    ["1", "2", "3", "+"],
    ["0", ".", "⌫", "="],
];

struct App {
    input: String,
    history: Vec<(String, String)>,
    error: String,
    theme_id: theme::ThemeId,
    next_theme_check: f64,
    mic: Mic,
    /// what the transcriber heard, handed back from its thread
    heard: std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>,
    said: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        Self {
            input: String::new(),
            history: Vec::new(),
            error: String::new(),
            theme_id,
            next_theme_check: 0.0,
            mic: Mic::Idle,
            heard: Default::default(),
            said: String::new(),
        }
    }

    fn equals(&mut self) {
        match eval(&self.input) {
            Ok(v) => {
                let r = fmt(v);
                self.history.insert(0, (self.input.clone(), r.clone()));
                self.history.truncate(50);
                self.input = r;
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }

    fn press(&mut self, k: &str) {
        self.error.clear();
        match k {
            "C" => self.input.clear(),
            "⌫" => { self.input.pop(); }
            "=" => self.equals(),
            k => self.input.push_str(k),
        }
    }

    /// Mic: first click records, second click transcribes and solves.
    fn toggle_mic(&mut self, ctx: &egui::Context) {
        match self.mic {
            Mic::Idle => match listen("start") {
                Ok(_) => { self.mic = Mic::Recording; self.error.clear(); self.said.clear(); }
                Err(e) => self.error = e,
            },
            Mic::Recording => {
                self.mic = Mic::Thinking;
                let (slot, ctx) = (self.heard.clone(), ctx.clone());
                std::thread::spawn(move || {
                    *slot.lock().unwrap() = Some(listen("stop"));
                    ctx.request_repaint();
                });
            }
            Mic::Thinking => {}
        }
    }

    fn take_heard(&mut self) {
        let Some(res) = self.heard.lock().unwrap().take() else { return };
        self.mic = Mic::Idle;
        match res {
            Ok(text) if !text.is_empty() => {
                self.said = text.clone();
                let expr = spoken_to_expr(&text);
                if expr.is_empty() {
                    self.error = "didn't hear a calculation".into();
                } else {
                    self.input = expr;
                    self.equals();
                }
            }
            Ok(_) => self.error = "didn't hear anything".into(),
            Err(e) => self.error = if e.is_empty() { "microphone failed".into() } else { e },
        }
    }

    /// One keypad key: a flat tile, accent for operators, filled for "=".
    fn key(&mut self, ui: &mut egui::Ui, k: &str, size: egui::Vec2) {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let op = matches!(k, "÷" | "×" | "−" | "+" | "(" | ")" | "C" | "⌫");
        let (fill, fg) = if k == "=" {
            (theme::accent(), theme::base())
        } else if resp.hovered() {
            (theme::surface1(), theme::text())
        } else {
            (theme::surface0(), if op { theme::accent() } else { theme::text() })
        };
        ui.painter().rect(rect, theme::radius(), fill, Stroke::NONE, egui::StrokeKind::Inside);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, k, egui::FontId::new(20.0, theme::MONO), fg);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            self.press(k);
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
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }

        self.take_heard();
        if self.mic == Mic::Recording {
            ctx.request_repaint_after(std::time::Duration::from_millis(250)); // blink
        }

        // Keyboard: typing goes straight into the expression.
        let (text, enter, back, esc, del) = ctx.input(|i| {
            let mut t = String::new();
            for e in &i.events {
                if let egui::Event::Text(s) = e {
                    t.push_str(s);
                }
            }
            (t, i.key_pressed(Key::Enter), i.key_pressed(Key::Backspace), i.key_pressed(Key::Escape), i.key_pressed(Key::Delete))
        });
        for c in text.chars().filter(|c| "0123456789.,+-*/x%^()".contains(*c)) {
            self.error.clear();
            self.input.push(match c { '*' => '×', '/' => '÷', '-' => '−', ',' => '.', c => c });
        }
        if back { self.press("⌫"); }
        if del { self.press("C"); }
        if enter || text.contains('=') { self.equals(); }
        if text.contains(' ') { self.toggle_mic(ctx); }
        if esc {
            if self.input.is_empty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.press("C");
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::base()).inner_margin(Margin::same(12)))
            .show(ctx, |ui| {
                widgets::header(ui, "calc", "type, click or speak · space = mic");
                widgets::hairline(ui);

                // Display: the expression, and a live preview of the answer.
                let preview = match eval(&self.input) {
                    Ok(v) if !self.input.is_empty() && fmt(v) != self.input => format!("= {}", fmt(v)),
                    _ => String::new(),
                };
                Frame::new().fill(theme::crust()).inner_margin(Margin::symmetric(12, 10)).corner_radius(theme::radius()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.with_layout(Layout::top_down(Align::Max), |ui| {
                        let shown = if self.input.is_empty() { "0" } else { &self.input };
                        ui.label(RichText::new(shown).size(28.0).color(theme::text()));
                        let sub = if self.error.is_empty() { preview } else { self.error.clone() };
                        let col = if self.error.is_empty() { theme::subtext0() } else { theme::bad() };
                        ui.label(RichText::new(if sub.is_empty() { " ".into() } else { sub }).size(14.0).color(col));
                    });
                });
                ui.add_space(10.0);

                // Mic bar: full width, above the keys.
                let (label, fill, fg) = match self.mic {
                    Mic::Idle => ("🎤  speak a calculation", theme::surface0(), theme::accent()),
                    Mic::Recording => {
                        let on = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
                        ("●  listening… click to solve", if on { theme::bad() } else { theme::dim(theme::bad(), 160) }, theme::base())
                    }
                    Mic::Thinking => ("…  working it out", theme::surface1(), theme::subtext0()),
                };
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                let fill = if resp.hovered() && self.mic == Mic::Idle { theme::surface1() } else { fill };
                ui.painter().rect(rect, theme::radius(), fill, Stroke::NONE, egui::StrokeKind::Inside);
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::new(15.0, theme::MONO), fg);
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() {
                    self.toggle_mic(ctx);
                }
                if !self.said.is_empty() {
                    ui.label(RichText::new(format!("heard: “{}”", self.said)).size(11.0).color(theme::overlay0()));
                }
                ui.add_space(6.0);

                // Keypad fills the width; 4 columns, 6 px gaps.
                let gap = 6.0;
                let w = (ui.available_width() - gap * 3.0) / 4.0;
                let size = vec2(w, 46.0);
                ui.spacing_mut().item_spacing = vec2(gap, gap);
                for row in KEYS {
                    ui.horizontal(|ui| {
                        for k in row {
                            self.key(ui, k, size);
                        }
                    });
                }

                ui.add_space(8.0);
                widgets::hairline(ui);
                widgets::caption(ui, if self.history.is_empty() { "history  ·  empty" } else { "history  ·  click to reuse" });
                let mut pick = None;
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (i, (expr, res)) in self.history.iter().enumerate() {
                        if widgets::row(ui, "=", expr, res).clicked() {
                            pick = Some(i);
                        }
                    }
                });
                if let Some(i) = pick {
                    self.input = self.history[i].1.clone();
                    self.error.clear();
                }
            });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Aura calc")
            .with_app_id("aura-calc")
            .with_inner_size([340.0, 560.0])
            .with_min_inner_size([280.0, 460.0]),
        ..Default::default()
    };
    eframe::run_native("Aura calc", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(s: &str) -> String {
        eval(s).map(fmt).unwrap_or_else(|x| format!("ERR {x}"))
    }

    #[test]
    fn precedence_and_parens() {
        assert_eq!(e("2+3×4"), "14");
        assert_eq!(e("(2+3)*4"), "20");
        assert_eq!(e("2^3^2"), "512");
        assert_eq!(e("−5+2"), "-3");
        assert_eq!(e("10÷4"), "2.5");
    }

    #[test]
    fn decimals_and_percent() {
        assert_eq!(e("0.1+0.2"), "0.3");
        assert_eq!(e("50%"), "0.5");
        assert_eq!(e("200×15%"), "30");
        assert_eq!(e("1,5+1"), "2.5");
    }

    #[test]
    fn speech() {
        let s = |t: &str| e(&spoken_to_expr(t));
        assert_eq!(s("twelve times seven"), "84");
        assert_eq!(s("What's 100 divided by 8?"), "12.5");
        assert_eq!(s("two hundred and thirty four plus six"), "240");
        assert_eq!(s("15 percent of 200"), "30");
        assert_eq!(s("three point five minus one"), "2.5");
        assert_eq!(s("2 to the power of 10"), "1024");
        assert_eq!(s("open bracket two plus three close bracket times four"), "20");
        assert_eq!(s("five squared"), "25");
        assert_eq!(s("one thousand five hundred multiplied by 2"), "3000");
    }

    #[test]
    fn errors() {
        assert_eq!(e("1/0"), "ERR can't divide by zero");
        assert_eq!(e("(1+2"), "ERR missing )");
        assert_eq!(e("2+"), "ERR unfinished");
    }
}
