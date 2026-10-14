//! The overlay as a wlr-layer-shell surface (KDE, Hyprland, Sway and most wlroots compositors).
//! A regular Wayland window can't stay on top, take no focus or pick its position; a layer
//! surface on the overlay layer can do all three.

use std::error::Error;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::ping::{make_ping, Ping};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use super::{rgba_to_bgra, Command, Pill, PILL_GAP};
use crate::overlay::{Renderer, CANVAS_HEIGHT, CANVAS_WIDTH, PILL_HEIGHT};
use crate::platform::PlatformEvent;

/// linux/input-event-codes.h BTN_LEFT.
const BTN_LEFT: u32 = 0x110;

pub struct Backend {
    event_loop: EventLoop<'static, State>,
    state: State,
    ping: Ping,
}

struct State {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    qh: QueueHandle<State>,
    pool: Option<SlotPool>,
    layer: Option<LayerSurface>,
    /// The buffer on screen, kept until the next one replaces it.
    buffer: Option<Buffer>,
    configured: bool,
    needs_redraw: bool,
    /// The surface's integer scale, from the output it's on.
    scale: i32,
    input_interactive: Option<bool>,
    pointer: Option<wl_pointer::WlPointer>,
    undo: Option<tiny_skia::Rect>,
    drawn_scale: i32,
    pill: Pill,
    renderer: Renderer,
    commands: Receiver<Command>,
    events: Sender<PlatformEvent>,
}

impl Backend {
    /// The backend, or `None` if the compositor has no layer-shell (GNOME).
    pub fn new(commands: Receiver<Command>, events: Sender<PlatformEvent>) -> Result<Option<Self>, Box<dyn Error>> {
        let conn = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();
        let Ok(layer_shell) = LayerShell::bind(&globals, &qh) else { return Ok(None) };
        let compositor = CompositorState::bind(&globals, &qh)?;
        let shm = Shm::bind(&globals, &qh)?;
        let state = State {
            registry_state: RegistryState::new(&globals),
            output_state: OutputState::new(&globals, &qh),
            seat_state: SeatState::new(&globals, &qh),
            compositor,
            layer_shell,
            shm,
            qh,
            pool: None,
            layer: None,
            buffer: None,
            configured: false,
            needs_redraw: false,
            scale: 1,
            input_interactive: None,
            pointer: None,
            undo: None,
            drawn_scale: 1,
            pill: Pill::new(),
            renderer: Renderer::new(),
            commands,
            events,
        };
        let event_loop: EventLoop<'static, State> = EventLoop::try_new()?;
        WaylandSource::new(conn, queue).insert(event_loop.handle()).map_err(|e| e.error.to_string())?;
        let (ping, ping_source) = make_ping()?;
        event_loop.handle().insert_source(ping_source, |_, _, _| {}).map_err(|e| e.error.to_string())?;
        log::info!("Wayland layer-shell overlay ready");
        Ok(Some(Self { event_loop, state, ping }))
    }

    /// Wakes the event loop when a command is queued.
    pub fn waker(&self) -> Box<dyn Fn() + Send + Sync> {
        let ping = self.ping.clone();
        Box::new(move || ping.ping())
    }

    pub fn run(mut self) {
        loop {
            let timeout = self.state.pill.next_wakeup(Instant::now());
            if let Err(e) = self.event_loop.dispatch(timeout, &mut self.state) {
                log::warn!("The Wayland overlay stopped: {e}");
                break;
            }
            self.state.process();
        }
        for _ in self.state.commands.iter() {}
    }
}

impl State {
    fn process(&mut self) {
        let now = Instant::now();
        let queued: Vec<Command> = self.commands.try_iter().collect();
        let mut dirty = false;
        for command in queued {
            dirty |= self.pill.apply(command, now);
        }
        dirty |= self.pill.expire(now);
        if !self.pill.is_visible() {
            if self.layer.is_some() {
                // Destroying the surface unmaps it; a new one is made (on the focused output)
                // next time.
                self.layer = None;
                self.buffer = None;
                self.configured = false;
                self.input_interactive = None;
            }
            return;
        }
        if self.layer.is_none() {
            self.create_layer();
            return; // drawn once configured
        }
        if self.configured && (dirty || self.needs_redraw || self.pill.animates()) {
            self.draw();
        }
    }

    fn create_layer(&mut self) {
        let surface = self.compositor.create_surface(&self.qh);
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("kudu"), None);
        layer.set_anchor(Anchor::BOTTOM);
        layer.set_size(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32);
        // The pill is centred in the canvas; keep its bottom PILL_GAP above the panel or edge.
        let margin = (PILL_GAP - (CANVAS_HEIGHT - PILL_HEIGHT) / 2.0).round() as i32;
        layer.set_margin(0, 0, margin, 0);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_exclusive_zone(0);
        // Start with the largest output scale so the first frame is sharp; corrected when the
        // surface reports the output it's on.
        self.scale = self
            .output_state
            .outputs()
            .filter_map(|o| self.output_state.info(&o))
            .map(|info| info.scale_factor)
            .max()
            .unwrap_or(1)
            .max(1);
        self.layer = Some(layer);
        self.input_interactive = None;
        self.set_input_region(None);
        if let Some(layer) = &self.layer {
            layer.commit();
        }
    }

    /// Click-through (an empty input region) except on the pill while it offers Undo.
    fn set_input_region(&mut self, pill: Option<tiny_skia::Rect>) {
        let interactive = self.pill.is_interactive() && pill.is_some();
        if self.input_interactive == Some(interactive) {
            return;
        }
        let Some(layer) = &self.layer else { return };
        let Ok(region) = Region::new(&self.compositor) else { return };
        if let (true, Some(pill)) = (interactive, pill) {
            let scale = self.scale.max(1) as f32;
            region.add(
                (pill.x() / scale) as i32,
                (pill.y() / scale) as i32,
                (pill.width() / scale).ceil() as i32,
                (pill.height() / scale).ceil() as i32,
            );
        }
        layer.wl_surface().set_input_region(Some(region.wl_region()));
        self.input_interactive = Some(interactive);
    }

    fn draw(&mut self) {
        self.needs_redraw = false;
        let Some(state) = self.pill.state.clone() else { return };
        if self.layer.is_none() {
            return;
        }
        let scale = self.scale.max(1);
        let frame = self.renderer.render(&state, &self.pill.levels, self.pill.time(Instant::now()), scale as f32);
        let (width, height) = (frame.pixmap.width() as i32, frame.pixmap.height() as i32);
        if self.pool.is_none() {
            match SlotPool::new((width * height * 4) as usize * 2, &self.shm) {
                Ok(pool) => self.pool = Some(pool),
                Err(e) => {
                    log::warn!("Can't allocate overlay buffers: {e}");
                    return;
                }
            }
        }
        let Some(pool) = self.pool.as_mut() else { return };
        let buffer = match pool.create_buffer(width, height, width * 4, wl_shm::Format::Argb8888) {
            Ok((buffer, canvas)) => {
                rgba_to_bgra(frame.pixmap.data(), canvas);
                buffer
            }
            Err(e) => {
                log::warn!("Can't create an overlay buffer: {e}");
                return;
            }
        };
        self.set_input_region(Some(frame.pill));
        let Some(layer) = &self.layer else { return };
        let surface = layer.wl_surface();
        surface.set_buffer_scale(scale);
        if let Err(e) = buffer.attach_to(surface) {
            log::warn!("Can't attach the overlay buffer: {e}");
            return;
        }
        surface.damage_buffer(0, 0, width, height);
        layer.commit();
        self.buffer = Some(buffer);
        self.undo = frame.undo;
        self.drawn_scale = scale;
    }

    fn is_our_surface(&self, surface: &wl_surface::WlSurface) -> bool {
        self.layer.as_ref().is_some_and(|l| l.wl_surface() == surface)
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, factor: i32) {
        if self.is_our_surface(surface) && factor != self.scale && factor > 0 {
            self.scale = factor;
            self.input_interactive = None;
            self.needs_redraw = true;
        }
    }

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for State {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        // The compositor removed the surface (e.g. its output went away).
        self.layer = None;
        self.buffer = None;
        self.configured = false;
        self.pill.apply(Command::Hide, Instant::now());
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface, _: LayerSurfaceConfigure, _: u32) {
        self.configured = true;
        self.draw();
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(e) => log::debug!("No pointer for the overlay: {e}"),
            }
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for State {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            if !self.is_our_surface(&event.surface) || !self.pill.is_interactive() {
                continue;
            }
            if let PointerEventKind::Press { button: BTN_LEFT, .. } = event.kind {
                // Surface coordinates are logical; the frame is in buffer pixels.
                let scale = self.drawn_scale.max(1) as f32;
                let (x, y) = (event.position.0 as f32 * scale, event.position.1 as f32 * scale);
                if self.undo.is_some_and(|r| x >= r.left() && x <= r.right() && y >= r.top() && y <= r.bottom()) {
                    let _ = self.events.send(PlatformEvent::UndoLearned);
                    self.pill.apply(Command::Hide, Instant::now());
                }
            }
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_registry!(State);
smithay_client_toolkit::delegate_dispatch2!(State);
