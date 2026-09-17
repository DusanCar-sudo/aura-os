//! The window itself: a small header, the desk, and one row of arrows.

use egui::{Align, Color32, FontId, Frame, Id, Key, Layout, Margin, RichText, Stroke, pos2};

use crate::canvas::{self, CanvasState};
use crate::slider;
use crate::displays::{self, Backend, Display};
use crate::theme;

/// The scale factors the scale arrow steps through.
const SCALE_STEPS: [f32; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5];

/// How often to ask the OS whether it has switched between light and dark.
/// The check is a single D-Bus round trip, so this is cheap.
const THEME_POLL_SECS: f64 = 2.0;

pub struct AuraOptanomai {
    displays: Vec<Display>,
    selected: usize,
    backend: Backend,
    backend_note: Option<String>,
    compositor: Option<String>,
    canvas: CanvasState,
    status: Option<Status>,
    error: Option<String>,
    /// The theme we are currently dressed in.
    theme_id: theme::ThemeId,
    next_theme_check: f64,
}

struct Status {
    text: String,
    good: bool,
    at: f64,
}

impl AuraOptanomai {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let theme_id = theme::ThemeId::detect();
        theme::apply(&cc.egui_ctx, &theme_id);

        let (backend, backend_note) = displays::detect();
        let compositor = displays::compositor_version();

        let mut app = Self {
            displays: Vec::new(),
            selected: 0,
            backend,
            backend_note,
            compositor,
            canvas: CanvasState::default(),
            status: None,
            error: None,
            theme_id,
            next_theme_check: 0.0,
        };
        app.reload();
        app
    }

    /// Re-read the OS theme, and restyle if it changed.
    fn follow_os_theme(&mut self, ctx: &egui::Context, now: f64) {
        if now < self.next_theme_check {
            return;
        }
        self.next_theme_check = now + THEME_POLL_SECS;

        let theme_id = theme::ThemeId::detect();
        if theme_id != self.theme_id {
            self.theme_id = theme_id;
            theme::apply(ctx, &self.theme_id);
            self.say(now, format!("theme: {}", self.theme_id.label()), true);
        }
    }

    /// Re-read the topology from the compositor, discarding pending edits.
    fn reload(&mut self) {
        match displays::query() {
            Ok(list) => {
                self.displays = list;
                self.error = None;
                if self.selected >= self.displays.len() {
                    self.selected = 0;
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn say(&mut self, now: f64, text: String, good: bool) {
        self.status = Some(Status { text, good, at: now });
    }

    // ── header ──────────────────────────────────────────────────────────────
    fn header(&mut self, ctx: &egui::Context) {
        let mut reload = false;
        let mut line_up = false;

        egui::TopBottomPanel::top("header")
            .resizable(false)
            .frame(
                Frame::NONE
                    .fill(theme::base())
                    .inner_margin(Margin::symmetric(12, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("displays")
                            .font(FontId::new(14.0, theme::bold()))
                            .color(theme::accent()),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("drag your screens to match your desk")
                            .font(FontId::monospace(12.0))
                            .color(theme::overlay0()),
                    );

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Reload").clicked() {
                            reload = true;
                        }
                        if ui.button("Line up").clicked() {
                            line_up = true;
                        }
                        if let Some(v) = &self.compositor {
                            ui.add_space(2.0);
                            ui.label(
                                RichText::new(format!("sway {v}"))
                                    .font(FontId::monospace(11.0))
                                    .color(theme::overlay0()),
                            );
                        }
                    });
                });

                // Hairline under the title bar.
                let r = ui.max_rect();
                ui.painter().line_segment(
                    [pos2(r.left(), r.bottom() + 4.0), pos2(r.right(), r.bottom() + 4.0)],
                    Stroke::new(1.0_f32, theme::surface1()),
                );
            });

        if reload {
            self.reload();
        }
        if line_up {
            displays::auto_arrange(&mut self.displays);
        }
    }

    // ── bottom control bar ──────────────────────────────────────────────────
    fn controls(&mut self, ctx: &egui::Context, now: f64) {
        let mut apply = false;
        let mut revert = false;
        let mut toggle = false;
        let mut rotate = false;

        egui::TopBottomPanel::bottom("controls")
            .resizable(false)
            .frame(
                Frame::NONE
                    .fill(theme::base())
                    .inner_margin(Margin::symmetric(12, 7)),
            )
            .show(ctx, |ui| {
                // Hairline over the control bar.
                let top = ui.max_rect().top();
                ui.painter().line_segment(
                    [
                        pos2(ui.max_rect().left(), top - 4.0),
                        pos2(ui.max_rect().right(), top - 4.0),
                    ],
                    Stroke::new(1.0_f32, theme::surface1()),
                );

                // ── the status line ─────────────────────────────────────────
                if let Some(s) = &self.status {
                    let ink = if s.good { theme::ok() } else { theme::bad() };
                    ui.label(
                        RichText::new(&s.text)
                            .font(FontId::monospace(11.0))
                            .color(ink),
                    );
                }

                // ── which screen are we editing, and the four actions ───────
                ui.horizontal(|ui| {
                    for i in 0..self.displays.len() {
                        let d = &self.displays[i];
                        let on = d.draft.enabled;
                        let label = if on {
                            d.pretty_name()
                        } else {
                            format!("{} [off]", d.pretty_name())
                        };
                        let here = i == self.selected;
                        let (fill, stroke, ink) = if here {
                            (
                                theme::surface1(),
                                Stroke::new(1.0_f32, theme::accent()),
                                theme::accent(),
                            )
                        } else {
                            (
                                Color32::TRANSPARENT,
                                Stroke::new(1.0_f32, theme::surface1()),
                                if on { theme::subtext0() } else { theme::overlay0() },
                            )
                        };
                        let chip = egui::Button::new(
                            RichText::new(label)
                                .font(FontId::monospace(11.5))
                                .color(ink),
                        )
                        .fill(fill)
                        .stroke(stroke)
                        .min_size(egui::vec2(0.0, 21.0));
                        if ui.add(chip).on_hover_text(chip_tooltip(d)).clicked() {
                            self.selected = i;
                        }
                    }

                    let dirty = self.displays.iter().any(|d| d.dirty());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let apply_btn = egui::Button::new(
                            RichText::new("apply")
                                .font(FontId::monospace(11.5))
                                .color(if dirty { theme::accent() } else { theme::overlay0() }),
                        )
                        .fill(theme::surface0())
                        .stroke(Stroke::new(
                            1.0_f32,
                            if dirty { theme::accent() } else { theme::surface1() },
                        ))
                        .min_size(egui::vec2(74.0, 21.0));
                        if ui.add_enabled(dirty, apply_btn).clicked() {
                            apply = true;
                        }

                        let revert_btn = egui::Button::new(
                            RichText::new("revert").font(FontId::monospace(11.5)),
                        )
                        .min_size(egui::vec2(66.0, 21.0));
                        if ui.add_enabled(dirty, revert_btn).clicked() {
                            revert = true;
                        }

                        if let Some(d) = self.displays.get(self.selected) {
                            if ui
                                .button(
                                    RichText::new(format!(
                                        "rotate {}",
                                        displays::transform_label(d.draft.transform)
                                    ))
                                    .font(FontId::monospace(11.5)),
                                )
                                .clicked()
                            {
                                rotate = true;
                            }
                            let on = d.draft.enabled;
                            if ui
                                .button(
                                    RichText::new(if on { "turn off" } else { "turn on" })
                                        .font(FontId::monospace(11.5)),
                                )
                                .clicked()
                            {
                                toggle = true;
                            }
                        }
                    });
                });

                ui.add_space(4.0);

                // ── one track, three arrows ─────────────────────────────────
                if self.displays.is_empty() {
                    return;
                }
                let idx = self.selected.min(self.displays.len() - 1);

                // Pull everything we need out of the display so the borrow ends
                // before we start writing edits back into it.
                let (res_opts, ref_opts, scale_opts, res_i, ref_i, scale_i, enabled, current) = {
                    let d = &self.displays[idx];
                    let cur = d.draft.mode;
                    let res_opts = d.resolutions();
                    let ref_opts = d.refresh_options(cur.width, cur.height);
                    let scale_opts = scale_options(d.draft.scale);
                    let res_i = res_opts
                        .iter()
                        .position(|m| m.width == cur.width && m.height == cur.height)
                        .unwrap_or(0);
                    let ref_i = ref_opts
                        .iter()
                        .position(|m| m.refresh == cur.refresh)
                        .unwrap_or(0);
                    let scale_i = scale_opts
                        .iter()
                        .position(|s| (s - d.draft.scale).abs() < 1e-3)
                        .unwrap_or(0);
                    (
                        res_opts,
                        ref_opts,
                        scale_opts,
                        res_i,
                        ref_i,
                        scale_i,
                        d.draft.enabled,
                        cur,
                    )
                };

                let mut marks = vec![
                    slider::Mark {
                        id: Id::new("track-resolution"),
                        title: "resolution",
                        label: res_opts[res_i].pixel_label(),
                        color: theme::accent(),
                        index: res_i,
                        count: res_opts.len(),
                        enabled,
                    },
                    slider::Mark {
                        id: Id::new("track-refresh"),
                        title: "refresh",
                        label: ref_opts[ref_i].hz_label(),
                        color: theme::accent2(),
                        index: ref_i,
                        count: ref_opts.len(),
                        enabled,
                    },
                    slider::Mark {
                        id: Id::new("track-scale"),
                        title: "scale",
                        label: format!("{}x", trim_scale(scale_opts[scale_i])),
                        color: theme::accent3(),
                        index: scale_i,
                        count: scale_opts.len(),
                        enabled,
                    },
                ];

                if slider::track(ui, &mut marks) {
                    if marks[0].index != res_i {
                        let target = res_opts[marks[0].index];
                        let d = &mut self.displays[idx];
                        // Hang on to the refresh rate closest to the one that
                        // was already chosen for this screen.
                        let refs = d.refresh_options(target.width, target.height);
                        d.draft.mode = refs
                            .iter()
                            .min_by_key(|m| (m.refresh - current.refresh).abs())
                            .copied()
                            .unwrap_or(target);
                    }
                    if marks[1].index != ref_i {
                        self.displays[idx].draft.mode = ref_opts[marks[1].index];
                    }
                    if marks[2].index != scale_i {
                        self.displays[idx].draft.scale = scale_opts[marks[2].index];
                    }
                }
            });

        // ── actions that need the whole list ────────────────────────────────
        if !self.displays.is_empty() {
            let idx = self.selected.min(self.displays.len() - 1);
            if toggle {
                let d = &mut self.displays[idx];
                d.draft.enabled = !d.draft.enabled;
            }
            if rotate {
                let d = &mut self.displays[idx];
                d.draft.transform = displays::next_transform(d.draft.transform);
            }
        }
        if revert {
            displays::revert(&mut self.displays);
            self.say(now, "reverted".into(), true);
        }
        if apply {
            let report = displays::apply(&self.displays);
            if report.ok() {
                let n = report.commands.len();
                self.say(now, format!("applied {n} command(s)"), true);
            } else {
                self.say(
                    now,
                    format!("sway rejected: {}", report.errors.join("  ")),
                    false,
                );
            }
            self.reload();
        }
    }

    // ── the desk ────────────────────────────────────────────────────────────
    fn desk(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(theme::base()).inner_margin(Margin {
                left: 12,
                right: 12,
                top: 8,
                bottom: 8,
            }))
            .show(ctx, |ui| {
                if self.displays.is_empty() {
                    ui.centered_and_justified(|ui| {
                        let msg = self
                            .error
                            .clone()
                            .unwrap_or_else(|| "no displays reported".into());
                        ui.label(
                            RichText::new(msg)
                                .font(FontId::monospace(12.0))
                                .color(theme::overlay0()),
                        );
                    });
                    return;
                }
                canvas::show(ui, &mut self.canvas, &mut self.displays, &mut self.selected);
            });
    }

    // ── no compositor to talk to ────────────────────────────────────────────
    fn unsupported(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(theme::base()).inner_margin(Margin::same(16)))
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    Frame::NONE
                        .fill(theme::mantle())
                        .stroke(Stroke::new(1.0_f32, theme::surface1()))
                        .inner_margin(Margin::same(20))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("no compositor to talk to")
                                        .font(FontId::monospace(14.0))
                                        .color(theme::bad()),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(
                                        self.backend_note
                                            .clone()
                                            .unwrap_or_else(|| "unsupported session".into()),
                                    )
                                    .font(FontId::monospace(12.0))
                                    .color(theme::subtext0()),
                                );
                            });
                        });
                });
            });
    }
}

impl eframe::App for AuraOptanomai {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        // Stay dressed the way the OS is dressed.
        self.follow_os_theme(ctx, now);

        // Let the toast fade out on its own.
        if let Some(s) = &self.status {
            if now - s.at > 7.0 {
                self.status = None;
            }
        }

        if self.backend == Backend::Unsupported {
            self.unsupported(ctx);
            return;
        }

        self.header(ctx);
        self.controls(ctx, now);
        self.desk(ctx);

        if ctx.input(|i| i.modifiers.command && i.key_pressed(Key::R)) {
            self.reload();
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Small helpers
// ────────────────────────────────────────────────────────────────────────────

/// The little "what is this screen, really" card behind each chip.
fn chip_tooltip(d: &Display) -> String {
    let mut s = format!("{}  {}", d.name, d.pretty_name());
    s.push('\n');
    s.push_str(&format!(
        "{} @ {}  scale {}",
        d.draft.mode.size_label(),
        d.draft.mode.hz_label(),
        trim_scale(d.draft.scale)
    ));
    if !d.make.trim().is_empty() {
        s.push('\n');
        s.push_str(d.make.trim());
    }
    if let Some(ws) = &d.workspace {
        s.push('\n');
        s.push_str(&format!("workspace {ws}"));
    }
    if d.focused {
        s.push_str("\nfocused");
    }
    s
}

fn scale_options(current: f32) -> Vec<f32> {
    let mut v = SCALE_STEPS.to_vec();
    if !v.iter().any(|s| (s - current).abs() < 1e-3) {
        v.push(current);
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    }
    v
}

fn trim_scale(s: f32) -> String {
    let t = format!("{s:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}
