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
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);
        Self { input: String::new(), history: Vec::new(), error: String::new(), theme_id, next_theme_check: 0.0 }
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
                widgets::header(ui, "calc", "type or click · enter = · esc clears");
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
    fn errors() {
        assert_eq!(e("1/0"), "ERR can't divide by zero");
        assert_eq!(e("(1+2"), "ERR missing )");
        assert_eq!(e("2+"), "ERR unfinished");
    }
}
