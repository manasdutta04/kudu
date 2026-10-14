//! The overlay as an X11 override-redirect window (X11 sessions, and GNOME on Wayland through
//! XWayland). Override-redirect keeps the window manager from decorating, focusing or moving
//! it; an empty input shape lets clicks fall through to the app below.

use std::error::Error;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    AtomEnum, ClipOrdering, ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux,
    EventMask, ImageFormat, ImageOrder, PropMode, Rectangle, StackMode, VisualClass, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use super::{rgba_over_opaque, rgba_to_bgra, Command, Pill, PILL_GAP};
use crate::overlay::{Renderer, CANVAS_HEIGHT, CANVAS_WIDTH, PILL_HEIGHT};
use crate::platform::linux::x11::{overlay_geometry, overlay_position, overlay_scale, Atoms, XRect};
use crate::platform::PlatformEvent;

/// How often to look for clicks while the pill takes them.
const CLICK_POLL: Duration = Duration::from_millis(30);
/// How often to raise the window again (other override-redirect windows may cover it).
const RAISE_EVERY: Duration = Duration::from_secs(1);
/// The colour behind the pill when the screen has no compositor (no transparency).
const OPAQUE_BACKGROUND: (u8, u8, u8) = (22, 22, 26);

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub struct Backend {
    conn: RustConnection,
    root: Window,
    screen: XRect,
    screen_num: usize,
    atoms: Atoms,
    window: Window,
    gc: u32,
    depth: u8,
    /// A 32-bit visual: transparent where a compositor runs.
    argb: bool,
    has_shape: bool,
    commands: Receiver<Command>,
    events: Sender<PlatformEvent>,
    renderer: Renderer,
    pill: Pill,
    mapped: bool,
    scale: f32,
    size: (u16, u16),
    composited: bool,
    input_shape_interactive: Option<bool>,
    undo: Option<tiny_skia::Rect>,
    pill_rect: Option<tiny_skia::Rect>,
    /// The window is cut to the pill's shape (no compositor).
    clipped: bool,
    needs_redraw: bool,
    raised_at: Instant,
    pixels: Vec<u8>,
}

impl Backend {
    pub fn new(commands: Receiver<Command>, events: Sender<PlatformEvent>) -> Result<Self> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let setup = conn.setup();
        if setup.image_byte_order != ImageOrder::LSB_FIRST {
            return Err("big-endian X servers aren't supported".into());
        }
        let screen = setup.roots.get(screen_num).ok_or("no X screen")?;
        let root = screen.root;
        let screen_rect = XRect { x: 0, y: 0, width: screen.width_in_pixels as i32, height: screen.height_in_pixels as i32 };
        let argb_visual = screen
            .allowed_depths
            .iter()
            .filter(|d| d.depth == 32)
            .flat_map(|d| d.visuals.iter())
            .find(|v| v.class == VisualClass::TRUE_COLOR)
            .map(|v| v.visual_id);
        let (root_depth, root_visual) = (screen.root_depth, screen.root_visual);
        let atoms = Atoms::new(&conn)?.reply()?;

        let window = conn.generate_id()?;
        let mut aux = CreateWindowAux::new()
            .override_redirect(1)
            .border_pixel(0)
            .event_mask(EventMask::BUTTON_PRESS | EventMask::EXPOSURE);
        let (depth, visual) = match argb_visual {
            Some(visual) => {
                let colormap = conn.generate_id()?;
                conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual)?;
                aux = aux.background_pixel(0).colormap(colormap);
                (32, visual)
            }
            None => {
                aux = aux.background_pixel(0x16161a);
                (root_depth, root_visual)
            }
        };
        let (width, height) = (CANVAS_WIDTH as u16, CANVAS_HEIGHT as u16);
        conn.create_window(depth, window, root, 0, 0, width, height, 0, WindowClass::INPUT_OUTPUT, visual, &aux)?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_WINDOW_TYPE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_WINDOW_TYPE_NOTIFICATION],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            window,
            atoms._NET_WM_STATE,
            AtomEnum::ATOM,
            &[atoms._NET_WM_STATE_ABOVE, atoms._NET_WM_STATE_SKIP_TASKBAR, atoms._NET_WM_STATE_SKIP_PAGER],
        )?;
        conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_NAME, AtomEnum::STRING, b"Kudu")?;
        conn.change_property8(PropMode::REPLACE, window, AtomEnum::WM_CLASS, AtomEnum::STRING, b"kudu\0Kudu\0")?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, window, &CreateGCAux::new())?;
        let has_shape = conn.extension_information(shape::X11_EXTENSION_NAME)?.is_some();
        if !has_shape {
            log::warn!("The X server has no Shape extension; the overlay will block clicks under it");
        }
        conn.flush()?;
        log::info!("X11 overlay ready ({}-bit visual)", depth);
        Ok(Self {
            conn,
            root,
            screen: screen_rect,
            screen_num,
            atoms,
            window,
            gc,
            depth,
            argb: argb_visual.is_some(),
            has_shape,
            commands,
            events,
            renderer: Renderer::new(),
            pill: Pill::new(),
            mapped: false,
            scale: 1.0,
            size: (width, height),
            composited: false,
            input_shape_interactive: None,
            undo: None,
            pill_rect: None,
            clipped: false,
            needs_redraw: false,
            raised_at: Instant::now(),
            pixels: Vec::new(),
        })
    }

    pub fn run(mut self) {
        if let Err(e) = self.event_loop() {
            log::warn!("The X11 overlay stopped: {e}");
        }
        // Keep accepting commands so the app never blocks on the overlay.
        for _ in self.commands.iter() {}
    }

    fn event_loop(&mut self) -> Result<()> {
        loop {
            let now = Instant::now();
            let mut timeout = self.pill.next_wakeup(now);
            if self.pill.is_interactive() {
                timeout = Some(timeout.map_or(CLICK_POLL, |t| t.min(CLICK_POLL)));
            }
            if self.mapped {
                timeout = Some(timeout.map_or(RAISE_EVERY, |t| t.min(RAISE_EVERY)));
            }
            let first = match timeout {
                Some(timeout) => match self.commands.recv_timeout(timeout) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return Ok(()),
                },
                None => match self.commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return Ok(()),
                },
            };
            let mut queued: Vec<Command> = first.into_iter().collect();
            queued.extend(self.commands.try_iter());
            let now = Instant::now();
            let mut dirty = false;
            for command in queued {
                dirty |= self.pill.apply(command, now);
            }
            dirty |= self.pill.expire(now);
            dirty |= self.handle_events()?;
            self.update(dirty)?;
        }
    }

    /// Handles pending X events; true if a redraw is needed.
    fn handle_events(&mut self) -> Result<bool> {
        let mut redraw = false;
        while let Some(event) = self.conn.poll_for_event()? {
            match event {
                Event::Expose(_) => redraw = true,
                Event::ButtonPress(press) if press.event == self.window && press.detail == 1 => {
                    let (x, y) = (press.event_x as f32, press.event_y as f32);
                    let on_undo = self.undo.is_some_and(|r| x >= r.left() && x <= r.right() && y >= r.top() && y <= r.bottom());
                    if self.pill.is_interactive() && on_undo {
                        let _ = self.events.send(PlatformEvent::UndoLearned);
                        redraw |= self.pill.apply(Command::Hide, Instant::now());
                    }
                }
                _ => {}
            }
        }
        Ok(redraw)
    }

    fn update(&mut self, dirty: bool) -> Result<()> {
        if !self.pill.is_visible() {
            if self.mapped {
                self.conn.unmap_window(self.window)?;
                self.conn.flush()?;
                self.mapped = false;
            }
            return Ok(());
        }
        let mut dirty = dirty || self.needs_redraw;
        if !self.mapped {
            self.place()?;
            self.conn.map_window(self.window)?;
            self.raise()?;
            self.mapped = true;
            dirty = true;
        } else if self.raised_at.elapsed() >= RAISE_EVERY {
            self.raise()?;
        }
        if dirty || self.pill.animates() {
            self.draw()?;
        }
        self.conn.flush()?;
        Ok(())
    }

    fn raise(&mut self) -> Result<()> {
        self.conn.configure_window(self.window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        self.raised_at = Instant::now();
        Ok(())
    }

    /// Sizes and moves the window to the bottom centre of the monitor under the pointer.
    fn place(&mut self) -> Result<()> {
        self.scale = overlay_scale(&self.conn, self.root);
        self.composited = self.is_composited();
        let width = (CANVAS_WIDTH * self.scale).round() as i32;
        let height = (CANVAS_HEIGHT * self.scale).round() as i32;
        let (monitor, workarea) = overlay_geometry(&self.conn, self.root, &self.atoms, self.screen);
        // The pill is centred in the canvas; keep its bottom PILL_GAP above the floor.
        let gap = ((PILL_GAP - (CANVAS_HEIGHT - PILL_HEIGHT) / 2.0) * self.scale).round() as i32;
        let (x, y) = overlay_position(monitor, workarea, width, height, gap);
        self.conn.configure_window(
            self.window,
            &ConfigureWindowAux::new().x(x).y(y).width(width as u32).height(height as u32),
        )?;
        self.size = (width as u16, height as u16);
        self.input_shape_interactive = None;
        Ok(())
    }

    /// Whether a compositing manager runs (otherwise transparent pixels show as black).
    fn is_composited(&self) -> bool {
        let name = format!("_NET_WM_CM_S{}", self.screen_num);
        let Ok(Ok(atom)) = self.conn.intern_atom(false, name.as_bytes()).map(|c| c.reply()) else { return false };
        self.conn
            .get_selection_owner(atom.atom)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|r| r.owner != 0)
    }

    fn draw(&mut self) -> Result<()> {
        self.needs_redraw = false;
        let state = self.pill.state.clone().ok_or("nothing to draw")?;
        let frame = self.renderer.render(&state, &self.pill.levels, self.pill.time(Instant::now()), self.scale);
        let (width, height) = (frame.pixmap.width() as usize, frame.pixmap.height() as usize);
        if (width as u16, height as u16) != self.size {
            return Ok(()); // sized for another scale; the next show re-places it
        }
        self.pixels.resize(width * height * 4, 0);
        let transparent = self.argb && self.composited;
        if transparent {
            rgba_to_bgra(frame.pixmap.data(), &mut self.pixels);
            if self.clipped && self.has_shape {
                // A compositor started: back to the full (transparent) window.
                self.conn.shape_mask(shape::SO::SET, shape::SK::BOUNDING, self.window, 0, 0, x11rb::NONE)?;
                self.clipped = false;
            }
        } else {
            rgba_over_opaque(frame.pixmap.data(), OPAQUE_BACKGROUND, &mut self.pixels);
            if !self.clipped || self.pill_rect != Some(frame.pill) {
                self.clip_to_pill(frame.pixmap.data(), width, height)?;
                self.clipped = true;
            }
        }
        self.pill_rect = Some(frame.pill);
        self.undo = frame.undo;
        self.update_input_shape(frame.pill)?;

        // Split the image so no request exceeds the server's maximum size.
        let row_bytes = width * 4;
        let max_rows = ((self.conn.maximum_request_bytes().saturating_sub(64)) / row_bytes).max(1);
        let mut row = 0;
        while row < height {
            let rows = max_rows.min(height - row);
            let data = &self.pixels[row * row_bytes..(row + rows) * row_bytes];
            self.conn.put_image(
                ImageFormat::Z_PIXMAP,
                self.window,
                self.gc,
                width as u16,
                rows as u16,
                0,
                row as i16,
                0,
                self.depth,
                data,
            )?;
            row += rows;
        }
        Ok(())
    }

    /// Without transparency, cuts the window to the pill's shape (row by row from its alpha).
    fn clip_to_pill(&self, rgba: &[u8], width: usize, height: usize) -> Result<()> {
        if !self.has_shape {
            return Ok(());
        }
        let rects = opaque_rows(rgba, width, height);
        self.conn.shape_rectangles(shape::SO::SET, shape::SK::BOUNDING, ClipOrdering::UNSORTED, self.window, 0, 0, &rects)?;
        Ok(())
    }

    /// Click-through everywhere, except on the pill while it offers Undo.
    fn update_input_shape(&mut self, pill: tiny_skia::Rect) -> Result<()> {
        let interactive = self.pill.is_interactive();
        if !self.has_shape || self.input_shape_interactive == Some(interactive) {
            return Ok(());
        }
        let rects = if interactive {
            vec![Rectangle {
                x: pill.x() as i16,
                y: pill.y() as i16,
                width: pill.width().ceil() as u16,
                height: pill.height().ceil() as u16,
            }]
        } else {
            Vec::new()
        };
        self.conn.shape_rectangles(shape::SO::SET, shape::SK::INPUT, ClipOrdering::UNSORTED, self.window, 0, 0, &rects)?;
        self.input_shape_interactive = Some(interactive);
        Ok(())
    }
}

/// One rectangle per pixel row covering the mostly opaque pixels (the pill, not its shadow).
fn opaque_rows(rgba: &[u8], width: usize, height: usize) -> Vec<Rectangle> {
    let mut rects = Vec::new();
    for y in 0..height {
        let row = &rgba[y * width * 4..(y + 1) * width * 4];
        let opaque = |x: &usize| row[x * 4 + 3] >= 128;
        let (Some(first), Some(last)) = ((0..width).find(opaque), (0..width).rev().find(opaque)) else { continue };
        rects.push(Rectangle { x: first as i16, y: y as i16, width: (last - first + 1) as u16, height: 1 });
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_rows_to_opaque_pixels() {
        // 4 × 2: row 0 opaque at x = 1…2, row 1 transparent.
        let mut rgba = vec![0u8; 4 * 2 * 4];
        rgba[4 + 3] = 255;
        rgba[2 * 4 + 3] = 200;
        rgba[3 * 4 + 3] = 20;
        let rects = opaque_rows(&rgba, 4, 2);
        assert_eq!(rects.len(), 1);
        assert_eq!((rects[0].x, rects[0].y, rects[0].width, rects[0].height), (1, 0, 2, 1));
    }
}
