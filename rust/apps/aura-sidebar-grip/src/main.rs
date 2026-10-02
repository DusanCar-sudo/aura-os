//! aura-sidebar-grip — drag the sidebar's edge to make it wider or narrower.
//!
//! waybar can show and hide its left bar (Super+B) but cannot be resized by
//! hand. This puts a thin invisible strip on every screen, on the "top"
//! layer, anchored left with exclusive zone 0. sway places such a surface
//! where the bars' reserved space ends: on the sidebar's edge while it is
//! shown, on the screen's edge while it is hidden. Nothing has to watch the
//! bar to keep the strip in place.
//!
//!   drag the edge        a line follows the pointer; on release the width
//!                        goes to ~/.config/aura/sidebar.json (waybar's left
//!                        bar includes it) and waybar reloads
//!   drag it almost shut  hides the sidebar (Super+B brings it back)
//!   drag from the edge   while the sidebar is hidden: shows it again
//!
//!   aura-sidebar-grip              start (a second copy exits quietly)
//!   aura-sidebar-grip --replace    take over from a running copy (updates)

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::Command;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Region},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::{
        client::{
            delegate_noop,
            globals::registry_queue_init,
            protocol::{wl_output, wl_pointer, wl_seat, wl_shm::Format, wl_surface},
            Connection, QueueHandle,
        },
        protocols::wp::{
            cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1},
            viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{cursor_shape::CursorShapeManager, PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{
        slot::{Buffer, SlotPool},
        Shm, ShmHandler,
    },
};

const NAME: &str = "aura-sidebar-grip";
const BTN_LEFT: u32 = 0x110;
/// the strip you grab, in pixels
const GRIP: u32 = 6;
/// the line that shows where the edge will go
const LINE: u32 = 3;
/// let go narrower than this and the sidebar hides instead
const HIDE_BELOW: i32 = 100;
/// the narrowest width that is saved (waybar keeps the bar wide enough for
/// its content in any case)
const MIN_WIDTH: i32 = 160;

/// One screen's strip.
struct Grip {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    viewport: WpViewport,
}

/// A drag in progress. Fields drop in order: the line's surface goes before
/// the buffer it shows.
struct Drag {
    line: LayerSurface,
    line_view: WpViewport,
    paint: Buffer,
    output: wl_output::WlOutput,
    /// where in the strip the pointer was pressed
    grab_x: f64,
    /// the sidebar's width when the drag began, 0 when hidden
    start: i32,
    /// where the edge would go now
    width: i32,
    max: i32,
}

impl Drag {
    fn follow(&mut self, x: f64) {
        self.width = ((self.start as f64 + x - self.grab_x).round() as i32).clamp(0, self.max);
        self.line.set_margin(0, 0, 0, line_x(self.width));
        self.line.commit();
    }
}

fn line_x(width: i32) -> i32 {
    (width - LINE as i32 / 2).max(0)
}

struct App {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    viewporter: WpViewporter,
    cursor_shapes: Option<CursorShapeManager>,
    pool: SlotPool,
    clear: Buffer,
    grips: Vec<Grip>,
    pointer: Option<(wl_pointer::WlPointer, Option<WpCursorShapeDeviceV1>)>,
    drag: Option<Drag>,
}

impl App {
    fn add_grip(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let surface = self.compositor.create_surface(qh);
        let viewport = self.viewporter.get_viewport(&surface, qh, ());
        let layer = self.layer_shell.create_layer_surface(qh, surface, Layer::Top, Some(NAME), Some(&output));
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT);
        layer.set_exclusive_zone(0); // placed after the bars' space: right on the sidebar's edge
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(GRIP, 0);
        layer.commit();
        self.grips.push(Grip { output, layer, viewport });
    }

    fn begin_drag(&mut self, qh: &QueueHandle<Self>, surface: &wl_surface::WlSurface, x: f64) {
        let Some(grip) = self.grips.iter().find(|g| g.layer.wl_surface() == surface) else { return };
        let output = grip.output.clone();
        let Some(info) = self.outputs.info(&output) else { return };
        let (Some(name), Some((ox, _)), Some((ow, _))) = (info.name, info.logical_position, info.logical_size) else {
            return;
        };
        let Some(start) = sidebar_width(&name, ox) else { return };

        // the line: overlay layer, laid out over the whole screen whatever
        // the bars reserve, moved by its left margin; clicks pass through it
        let surface = self.compositor.create_surface(qh);
        let line_view = self.viewporter.get_viewport(&surface, qh, ());
        if let Ok(region) = Region::new(&self.compositor) {
            surface.set_input_region(Some(region.wl_region()));
        }
        let line = self.layer_shell.create_layer_surface(qh, surface, Layer::Overlay, Some(NAME), Some(&output));
        line.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT);
        line.set_exclusive_zone(-1);
        line.set_keyboard_interactivity(KeyboardInteractivity::None);
        line.set_size(LINE, 0);
        line.set_margin(0, 0, 0, line_x(start));
        line.commit();

        let Ok((paint, canvas)) = self.pool.create_buffer(1, 1, 4, Format::Argb8888) else { return };
        canvas[..4].copy_from_slice(&accent()); // the slot can be larger than the one pixel
        self.drag = Some(Drag { line, line_view, paint, output, grab_x: x, start, width: start, max: ow / 2 });
    }

    fn end_drag(&mut self) {
        let Some(drag) = self.drag.take() else { return };
        let (start, width) = (drag.start, drag.width);
        drop(drag); // the line goes away
        if (width - start).abs() < 4 {
            return; // a click, not a drag
        }
        if width < HIDE_BELOW {
            if start > 0 {
                signal_waybar(libc::SIGUSR1); // the left bar's toggle: hide it
            }
            return;
        }
        match save_width(width.max(MIN_WIDTH)) {
            Ok(()) => signal_waybar(libc::SIGUSR2), // reload: every bar re-reads its config
            Err(e) => eprintln!("{NAME}: cannot save the width: {e}"),
        }
    }
}

/// How far the work area on `output` starts from its left edge: the
/// sidebar's real width (0 while it is hidden). The bars' reserved space is
/// not in sway's IPC, but the visible workspace's rect is what is left.
fn sidebar_width(output: &str, output_x: i32) -> Option<i32> {
    let out = Command::new("swaymsg").args(["-t", "get_workspaces", "-r"]).output().ok()?;
    let list: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let ws = list.as_array()?.iter().find(|w| w["output"] == output && w["visible"] == true)?;
    Some(ws["rect"]["x"].as_i64()? as i32 - output_x)
}

/// waybar's left bar includes this file; AURA_SIDEBAR_FILE points
/// elsewhere (tests).
fn width_file() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("AURA_SIDEBAR_FILE") {
        return Some(p.into());
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/aura/sidebar.json"))
}

fn save_width(width: i32) -> io::Result<()> {
    let path = width_file().ok_or_else(|| io::Error::other("no HOME"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, format!("{{ \"width\": {width} }}\n"))?;
    fs::rename(&tmp, &path)
}

/// The waybar of this session only — same Wayland display and runtime dir
/// as ours — so a test session's bar, or another user's, is left alone.
fn signal_waybar(sig: i32) {
    let var = |k: &str| std::env::var(k).ok().map(|v| format!("{k}={v}"));
    let (Some(display), Some(runtime)) = (var("WAYLAND_DISPLAY"), var("XDG_RUNTIME_DIR")) else { return };
    let Ok(procs) = fs::read_dir("/proc") else { return };
    for p in procs.flatten() {
        let Ok(pid) = p.file_name().to_string_lossy().parse::<i32>() else { continue };
        if fs::read_to_string(p.path().join("comm")).map(|c| c.trim() != "waybar").unwrap_or(true) {
            continue;
        }
        let Ok(env) = fs::read(p.path().join("environ")) else { continue };
        let vars: Vec<&[u8]> = env.split(|b| *b == 0).collect();
        if vars.contains(&display.as_bytes()) && vars.contains(&runtime.as_bytes()) {
            unsafe { libc::kill(pid, sig) };
        }
    }
}

/// The theme's accent (waybar's theme.css, written by aura-os-theme) as one
/// ARGB8888 pixel; orange when there is none.
fn accent() -> [u8; 4] {
    let css = std::env::var_os("HOME")
        .and_then(|h| fs::read_to_string(PathBuf::from(h).join(".config/waybar/theme.css")).ok())
        .unwrap_or_default();
    let hex = css.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        if w.next()? == "@define-color" && w.next()? == "accent" { w.next() } else { None }
    });
    let rgb = hex
        .map(|h| h.trim_end_matches(';').trim_start_matches('#'))
        .filter(|h| h.len() == 6)
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or(0xfe8019);
    [(rgb & 0xff) as u8, (rgb >> 8 & 0xff) as u8, (rgb >> 16 & 0xff) as u8, 0xff]
}

fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
}

/// One copy per session: a lock that dies with the process. `--replace`
/// stops the running copy first (only if it really is this program; the
/// kernel keeps 15 characters of a process name).
fn single_instance(replace: bool) -> Option<File> {
    let path = runtime_dir()?.join(format!("{NAME}.pid"));
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).ok()?;
    let try_lock = |f: &File| unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if !try_lock(&file) {
        if !replace {
            return None;
        }
        if let Ok(pid) = fs::read_to_string(&path).unwrap_or_default().trim().parse::<i32>() {
            if fs::read_to_string(format!("/proc/{pid}/comm")).map(|c| c.trim() == &NAME[..15]).unwrap_or(false) {
                unsafe { libc::kill(pid, libc::SIGTERM) };
            }
        }
        let mut tries = 0;
        while !try_lock(&file) {
            tries += 1;
            if tries > 40 {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    let _ = file.set_len(0);
    let _ = writeln!(&file, "{}", std::process::id());
    Some(file)
}

fn main() {
    let replace = std::env::args().any(|a| a == "--replace");
    let Some(_lock) = single_instance(replace) else { return };

    let conn = Connection::connect_to_env().expect("aura-sidebar-grip: no Wayland session");
    let (globals, mut queue) = registry_queue_init(&conn).expect("aura-sidebar-grip: registry");
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("zwlr_layer_shell_v1");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let viewporter: WpViewporter = globals.bind(&qh, 1..=1, ()).expect("wp_viewporter");
    let cursor_shapes = CursorShapeManager::bind(&globals, &qh).ok();

    // the strip itself is one transparent pixel, stretched
    let mut pool = SlotPool::new(64, &shm).expect("shm pool");
    let (clear, canvas) = pool.create_buffer(1, 1, 4, Format::Argb8888).expect("buffer");
    canvas.fill(0);

    let mut app = App {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        viewporter,
        cursor_shapes,
        pool,
        clear,
        grips: Vec::new(),
        pointer: None,
        drag: None,
    };

    loop {
        if let Err(e) = queue.blocking_dispatch(&mut app) {
            eprintln!("aura-sidebar-grip: {e}");
            break;
        }
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.grips.retain(|g| &g.layer != layer);
        if self.drag.as_ref().is_some_and(|d| &d.line == layer) {
            self.drag = None;
        }
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let (w, h) = configure.new_size;
        if w == 0 || h == 0 {
            return;
        }
        let (viewport, buffer) = if let Some(g) = self.grips.iter().find(|g| &g.layer == layer) {
            (&g.viewport, &self.clear)
        } else if let Some(d) = self.drag.as_ref().filter(|d| &d.line == layer) {
            (&d.line_view, &d.paint)
        } else {
            return;
        };
        viewport.set_destination(w as i32, h as i32);
        let surface = layer.wl_surface();
        surface.attach(Some(buffer.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, 1, 1);
        surface.commit();
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_grip(qh, output);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.grips.retain(|g| g.output != output);
        if self.drag.as_ref().is_some_and(|d| d.output == output) {
            self.drag = None;
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer && self.pointer.is_none() {
            if let Ok(pointer) = self.seats.get_pointer(qh, &seat) {
                let shape = self.cursor_shapes.as_ref().map(|m| m.get_shape_device(&pointer, qh));
                self.pointer = Some((pointer, shape));
            }
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            if let Some((pointer, shape)) = self.pointer.take() {
                if let Some(s) = shape {
                    s.destroy();
                }
                pointer.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for ev in events {
            match ev.kind {
                // the strip is the only thing of ours that takes the pointer
                PointerEventKind::Enter { serial } => {
                    if let Some((_, Some(shape))) = &self.pointer {
                        shape.set_shape(serial, Shape::EwResize);
                    }
                }
                PointerEventKind::Press { button: BTN_LEFT, .. } if self.drag.is_none() => {
                    self.begin_drag(qh, &ev.surface, ev.position.0);
                }
                // while the button is held, sway keeps sending motion to the
                // strip, in the strip's coordinates, wherever the pointer is
                PointerEventKind::Motion { .. } => {
                    if let Some(drag) = &mut self.drag {
                        drag.follow(ev.position.0);
                    }
                }
                PointerEventKind::Release { button: BTN_LEFT, .. } => self.end_drag(),
                _ => {}
            }
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_registry!(App);
delegate_noop!(App: ignore WpViewporter);
delegate_noop!(App: ignore WpViewport);
