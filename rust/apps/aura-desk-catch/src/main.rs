//! aura-desk-catch — right-click on the bare desktop opens the Aura menu.
//!
//! sway never sees a click on the wallpaper: swaybg's layer surface takes
//! it, and `bindsym --whole-window button3` would steal right-click from
//! every app. So this puts one invisible surface on each screen, on the
//! "bottom" layer: above the wallpaper (a new wallpaper cannot cover it),
//! below every window and the bar. It is one transparent pixel stretched
//! by the compositor (wp_viewporter), so it costs next to nothing.
//!
//!   right click   aura-rclick desktop --at X,Y   (layout coordinates)
//!   left click    closes an open menu (a click here moves no focus, so
//!                 the menu would not notice it on its own)
//!
//!   aura-desk-catch              start (a second copy exits quietly)
//!   aura-desk-catch --replace    take over from a running copy (updates)

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::{Child, Command};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    reexports::{
        client::{
            delegate_noop,
            globals::registry_queue_init,
            protocol::{wl_output, wl_pointer, wl_seat, wl_surface},
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

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// One screen's invisible surface.
struct Desk {
    output: wl_output::WlOutput,
    layer: LayerSurface,
    viewport: WpViewport,
}

struct Catch {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    viewporter: WpViewporter,
    cursor_shapes: Option<CursorShapeManager>,
    _pool: SlotPool,
    pixel: Buffer,
    desks: Vec<Desk>,
    pointer: Option<(wl_pointer::WlPointer, Option<WpCursorShapeDeviceV1>)>,
    children: Vec<Child>,
    menu_bin: PathBuf,
}

impl Catch {
    fn add_desk(&mut self, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let surface = self.compositor.create_surface(qh);
        let viewport = self.viewporter.get_viewport(&surface, qh, ());
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Bottom,
            Some("aura-desk-catch"),
            Some(&output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(-1); // the whole screen, never pushed aside by a bar
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        layer.commit();
        self.desks.push(Desk { output, layer, viewport });
    }

    /// Where on the whole layout a surface-local point is.
    fn layout_point(&self, surface: &wl_surface::WlSurface, (x, y): (f64, f64)) -> Option<(i32, i32)> {
        let desk = self.desks.iter().find(|d| d.layer.wl_surface() == surface)?;
        let (ox, oy) = self.outputs.info(&desk.output)?.logical_position?;
        Some((ox + x.round() as i32, oy + y.round() as i32))
    }

    fn open_menu(&mut self, at: (i32, i32)) {
        self.children.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        match Command::new(&self.menu_bin)
            .args(["desktop", "--at", &format!("{},{}", at.0, at.1)])
            .spawn()
        {
            Ok(child) => self.children.push(child),
            Err(e) => eprintln!("aura-desk-catch: cannot start {}: {e}", self.menu_bin.display()),
        }
    }
}

/// The menu writes its pid here; whatever opened it, a click on the desktop
/// closes it. Only a process that really is aura-rclick gets the signal.
fn close_menu() {
    let Some(dir) = runtime_dir() else { return };
    let Ok(text) = fs::read_to_string(dir.join("aura-rclick.pid")) else { return };
    let Ok(pid) = text.trim().parse::<i32>() else { return };
    if fs::read_to_string(format!("/proc/{pid}/comm")).map(|c| c.trim() == "aura-rclick").unwrap_or(false) {
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
}

/// One copy per session: a lock that dies with the process. `--replace`
/// stops the running copy first (only if it really is this program).
fn single_instance(replace: bool) -> Option<File> {
    let path = runtime_dir()?.join("aura-desk-catch.pid");
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path).ok()?;
    let try_lock = |f: &File| unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    if !try_lock(&file) {
        if !replace {
            return None;
        }
        if let Ok(pid) = fs::read_to_string(&path).unwrap_or_default().trim().parse::<i32>() {
            if fs::read_to_string(format!("/proc/{pid}/comm")).map(|c| c.trim() == "aura-desk-catch").unwrap_or(false) {
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

fn menu_bin() -> PathBuf {
    if let Some(p) = std::env::var_os("AURA_RCLICK") {
        return p.into();
    }
    // built side by side in rust/target/release
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("aura-rclick");
        if sibling.exists() {
            return sibling;
        }
    }
    "aura-rclick".into()
}

fn main() {
    let replace = std::env::args().any(|a| a == "--replace");
    let Some(_lock) = single_instance(replace) else { return };

    let conn = Connection::connect_to_env().expect("aura-desk-catch: no Wayland session");
    let (globals, mut queue) = registry_queue_init(&conn).expect("aura-desk-catch: registry");
    let qh = queue.handle();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("zwlr_layer_shell_v1");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let viewporter: WpViewporter = globals.bind(&qh, 1..=1, ()).expect("wp_viewporter");
    let cursor_shapes = CursorShapeManager::bind(&globals, &qh).ok();

    // the one pixel every screen shows: fully transparent
    let mut pool = SlotPool::new(4, &shm).expect("shm pool");
    let (pixel, canvas) = pool
        .create_buffer(1, 1, 4, smithay_client_toolkit::reexports::client::protocol::wl_shm::Format::Argb8888)
        .expect("buffer");
    canvas.fill(0);

    let mut catch = Catch {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        viewporter,
        cursor_shapes,
        _pool: pool,
        pixel,
        desks: Vec::new(),
        pointer: None,
        children: Vec::new(),
        menu_bin: menu_bin(),
    };

    loop {
        if let Err(e) = queue.blocking_dispatch(&mut catch) {
            eprintln!("aura-desk-catch: {e}");
            break;
        }
    }
}

impl LayerShellHandler for Catch {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.desks.retain(|d| &d.layer != layer);
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(desk) = self.desks.iter().find(|d| &d.layer == layer) else { return };
        let (w, h) = configure.new_size;
        if w == 0 || h == 0 {
            return;
        }
        desk.viewport.set_destination(w as i32, h as i32);
        let surface = desk.layer.wl_surface();
        surface.attach(Some(self.pixel.wl_buffer()), 0, 0);
        surface.damage_buffer(0, 0, 1, 1);
        surface.commit();
    }
}

impl OutputHandler for Catch {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, qh: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_desk(qh, output);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.desks.retain(|d| d.output != output);
    }
}

impl SeatHandler for Catch {
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

impl PointerHandler for Catch {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for ev in events {
            match ev.kind {
                // nobody else sets the arrow over the desktop
                PointerEventKind::Enter { serial } => {
                    if let Some((_, Some(shape))) = &self.pointer {
                        shape.set_shape(serial, Shape::Default);
                    }
                }
                PointerEventKind::Press { button: BTN_RIGHT, .. } => {
                    close_menu();
                    if let Some(at) = self.layout_point(&ev.surface, ev.position) {
                        self.open_menu(at);
                    }
                }
                PointerEventKind::Press { button: BTN_LEFT | BTN_MIDDLE, .. } => close_menu(),
                _ => {}
            }
        }
    }
}

impl CompositorHandler for Catch {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl ShmHandler for Catch {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Catch {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Catch);
delegate_output!(Catch);
delegate_shm!(Catch);
delegate_seat!(Catch);
delegate_pointer!(Catch);
delegate_layer!(Catch);
delegate_registry!(Catch);
delegate_noop!(Catch: ignore WpViewporter);
delegate_noop!(Catch: ignore WpViewport);
