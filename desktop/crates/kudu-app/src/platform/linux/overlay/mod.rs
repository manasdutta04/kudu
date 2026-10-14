//! The overlay pill on Linux, drawn by the shared software renderer and shown by one of two
//! backends on its own thread:
//! - Wayland compositors with layer-shell (KDE, Hyprland, Sway and most wlroots ones): a
//!   layer surface on the overlay layer, the only way a Wayland client can float above others;
//! - X11, and Wayland compositors without layer-shell (GNOME) through XWayland: an
//!   override-redirect window.

mod wayland;
mod x11;

use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use super::util::Session;
use crate::overlay::OverlayState;
use crate::platform::{Overlay, PlatformEvent, OVERLAY_FRAME};

/// How far above the bottom of the screen (or panel) the pill sits, in logical pixels.
const PILL_GAP: f32 = 12.0;

pub(crate) enum Command {
    Show(OverlayState),
    Levels([f32; 5]),
    Hide,
}

/// What the pill shows and when it changes, independent of the backend.
pub(crate) struct Pill {
    pub state: Option<OverlayState>,
    pub levels: [f32; 5],
    epoch: Instant,
    hide_at: Option<Instant>,
}

impl Pill {
    pub fn new() -> Self {
        Self { state: None, levels: [0.0; 5], epoch: Instant::now(), hide_at: None }
    }

    /// Applies a command; true if the picture changed.
    pub fn apply(&mut self, command: Command, now: Instant) -> bool {
        match command {
            Command::Show(state) => {
                self.hide_at = state.auto_hide_after().map(|after| now + after);
                if !matches!(self.state, Some(OverlayState::Recording { .. })) {
                    self.levels = [0.0; 5];
                }
                self.state = Some(state);
                true
            }
            Command::Levels(levels) => {
                self.levels = levels;
                matches!(self.state, Some(OverlayState::Recording { .. }))
            }
            Command::Hide => {
                self.hide_at = None;
                self.state.take().is_some()
            }
        }
    }

    /// Hides the pill if its time is up; true if it did.
    pub fn expire(&mut self, now: Instant) -> bool {
        if self.hide_at.is_some_and(|at| now >= at) {
            self.hide_at = None;
            self.state = None;
            return true;
        }
        false
    }

    pub fn is_visible(&self) -> bool {
        self.state.is_some()
    }

    pub fn animates(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.animates())
    }

    pub fn is_interactive(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.is_interactive())
    }

    /// Seconds since start, for animations.
    pub fn time(&self, now: Instant) -> f64 {
        now.duration_since(self.epoch).as_secs_f64()
    }

    /// How long the backend may sleep before something needs doing (None: until a command).
    pub fn next_wakeup(&self, now: Instant) -> Option<Duration> {
        let mut wait: Option<Duration> = None;
        if self.animates() {
            wait = Some(OVERLAY_FRAME);
        }
        if let Some(at) = self.hide_at {
            let until = at.saturating_duration_since(now);
            wait = Some(wait.map_or(until, |w| w.min(until)));
        }
        wait
    }
}

/// Premultiplied RGBA (the renderer) to premultiplied BGRA, i.e. little-endian ARGB8888 as
/// both `wl_shm` and 32-bit X visuals expect.
pub(crate) fn rgba_to_bgra(src: &[u8], dst: &mut [u8]) {
    for (s, d) in src.as_chunks::<4>().0.iter().zip(dst.as_chunks_mut::<4>().0.iter_mut()) {
        d[0] = s[2];
        d[1] = s[1];
        d[2] = s[0];
        d[3] = s[3];
    }
}

/// Premultiplied RGBA composited over an opaque colour, as BGRX, for X servers without a
/// 32-bit visual (no transparency).
pub(crate) fn rgba_over_opaque(src: &[u8], background: (u8, u8, u8), dst: &mut [u8]) {
    for (s, d) in src.as_chunks::<4>().0.iter().zip(dst.as_chunks_mut::<4>().0.iter_mut()) {
        let rest = 255 - s[3] as u32;
        let over = |c: u8, bg: u8| (c as u32 + (bg as u32 * rest + 127) / 255).min(255) as u8;
        d[0] = over(s[2], background.2);
        d[1] = over(s[1], background.1);
        d[2] = over(s[0], background.0);
        d[3] = 255;
    }
}

/// The overlay handle the app holds; commands go to the backend thread.
pub struct LinuxOverlay {
    commands: Sender<Command>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl LinuxOverlay {
    pub fn start(session: &Session, events: Sender<PlatformEvent>) -> Self {
        let (commands, receiver) = crossbeam_channel::unbounded();
        let (wake_tx, wake_rx) = crossbeam_channel::bounded::<Box<dyn Fn() + Send + Sync>>(1);
        let session = session.clone();
        let spawned = std::thread::Builder::new().name("kudu-overlay".into()).spawn(move || {
            run(session, receiver, events, wake_tx);
        });
        if let Err(e) = spawned {
            log::warn!("Couldn't start the overlay thread: {e}");
        }
        // The Wayland backend wakes its event loop through a calloop ping; wait briefly for it.
        let wake = wake_rx.recv_timeout(Duration::from_secs(2)).unwrap_or_else(|_| Box::new(|| {}));
        Self { commands, wake }
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_ok() {
            (self.wake)();
        }
    }
}

impl Overlay for LinuxOverlay {
    fn show(&self, state: OverlayState) {
        self.send(Command::Show(state));
    }

    fn set_levels(&self, levels: [f32; 5]) {
        self.send(Command::Levels(levels));
    }

    fn hide(&self) {
        self.send(Command::Hide);
    }
}

fn run(
    session: Session,
    commands: Receiver<Command>,
    events: Sender<PlatformEvent>,
    wake: Sender<Box<dyn Fn() + Send + Sync>>,
) {
    if session.wayland {
        match wayland::Backend::new(commands.clone(), events.clone()) {
            Ok(Some(backend)) => {
                let _ = wake.send(backend.waker());
                backend.run();
                return;
            }
            Ok(None) => log::info!("The compositor has no layer-shell; using an XWayland window for the overlay"),
            Err(e) => log::info!("Wayland overlay unavailable ({e}); trying X11"),
        }
    }
    let _ = wake.send(Box::new(|| {}));
    if session.x11 {
        match x11::Backend::new(commands.clone(), events) {
            Ok(backend) => {
                backend.run();
                return;
            }
            Err(e) => log::warn!("X11 overlay unavailable: {e}"),
        }
    }
    log::warn!("No way to show the overlay on this desktop; continuing without it");
    // Drain commands so senders never block or pile up.
    for _ in commands.iter() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_rgba_to_bgra() {
        let src = [10, 20, 30, 40, 255, 0, 0, 255];
        let mut dst = [0u8; 8];
        rgba_to_bgra(&src, &mut dst);
        assert_eq!(dst, [30, 20, 10, 40, 0, 0, 255, 255]);
    }

    #[test]
    fn composites_over_an_opaque_background() {
        // Transparent → background; opaque → itself; half-transparent premultiplied white.
        let src = [0, 0, 0, 0, 200, 100, 50, 255, 128, 128, 128, 128];
        let mut dst = [0u8; 12];
        rgba_over_opaque(&src, (20, 20, 24), &mut dst);
        assert_eq!(&dst[0..4], &[24, 20, 20, 255]);
        assert_eq!(&dst[4..8], &[50, 100, 200, 255]);
        assert_eq!(&dst[8..12], &[128 + 12, 128 + 10, 128 + 10, 255]);
    }

    #[test]
    fn the_pill_animates_and_hides_itself() {
        let t = Instant::now();
        let mut pill = Pill::new();
        assert_eq!(pill.next_wakeup(t), None);
        assert!(pill.apply(Command::Show(OverlayState::Recording { locked: false, command: false }), t));
        assert_eq!(pill.next_wakeup(t), Some(OVERLAY_FRAME));
        assert!(pill.apply(Command::Levels([0.5; 5]), t));
        assert!(pill.apply(Command::Show(OverlayState::Message("Didn't catch that".into())), t));
        assert!(!pill.apply(Command::Levels([0.5; 5]), t), "levels don't redraw a message");
        assert_eq!(pill.next_wakeup(t), Some(Duration::from_millis(2000)));
        assert!(!pill.expire(t + Duration::from_millis(1999)));
        assert!(pill.expire(t + Duration::from_millis(2000)));
        assert!(!pill.is_visible());
        assert!(!pill.apply(Command::Hide, t));
    }
}
