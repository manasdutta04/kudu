//! The overlay pill: a layered, click-through, always-on-top popup that never takes focus.
//!
//! The shared software renderer draws each frame; it's copied into a DIB section and handed to
//! `UpdateLayeredWindow`, which composites it with per-pixel alpha. The window lives on the UI
//! thread; the `Overlay` methods, called from any thread, forward to it.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ::windows::core::w;
use ::windows::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use ::windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow,
    ScreenToClient, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL,
};
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use ::windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, KillTimer,
    LoadCursorW, RegisterClassExW, SetCursor, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    UpdateLayeredWindow, GWL_EXSTYLE, HWND_TOPMOST, IDC_ARROW, IDC_HAND, MA_NOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_SETCURSOR, WM_SETTINGCHANGE, WM_TIMER,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use crossbeam_channel::Sender;
use parking_lot::Mutex;

use super::worker::Worker;
use crate::overlay::{OverlayState, Renderer};
use crate::platform::{Overlay, PlatformEvent, OVERLAY_FRAME};

const FRAME_TIMER: usize = 1;
const HIDE_TIMER: usize = 2;
/// How often the position is checked while the pill shows (the mouse may move to another
/// screen, the taskbar may move, an app may go full screen).
const PLACEMENT_CHECK: Duration = Duration::from_millis(250);

/// The `Overlay` handed to the app: forwards to the UI thread.
pub struct OverlayHandle {
    ui: Worker,
    levels: Arc<Mutex<[f32; 5]>>,
}

impl OverlayHandle {
    /// Creates the window on the UI thread (call from there).
    pub fn create(events: Sender<PlatformEvent>) -> anyhow::Result<Arc<Mutex<[f32; 5]>>> {
        let levels = Arc::new(Mutex::new([0.0; 5]));
        let window = OverlayWindow::create(events, levels.clone())?;
        OVERLAY.with(|cell| *cell.borrow_mut() = Some(window));
        Ok(levels)
    }

    pub fn new(ui: Worker, levels: Arc<Mutex<[f32; 5]>>) -> Self {
        Self { ui, levels }
    }
}

impl Overlay for OverlayHandle {
    fn show(&self, state: OverlayState) {
        self.ui.post(move || {
            with_overlay(|overlay| overlay.show(state));
        });
    }

    fn set_levels(&self, levels: [f32; 5]) {
        // Read by the next animation frame; no need to wake the UI thread.
        *self.levels.lock() = levels;
    }

    fn hide(&self) {
        self.ui.post(|| {
            with_overlay(OverlayWindow::hide);
        });
    }
}

thread_local! {
    static OVERLAY: RefCell<Option<OverlayWindow>> = const { RefCell::new(None) };
}

fn with_overlay<R>(f: impl FnOnce(&mut OverlayWindow) -> R) -> Option<R> {
    OVERLAY.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Where the pill goes: bottom centre of `area`, `gap` pixels above its bottom edge.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    area: RECT,
    gap: i32,
    scale: f32,
}

impl Placement {
    /// On the screen under the mouse: above the taskbar, or near the bottom edge when an app
    /// is full screen there.
    fn current() -> Self {
        // SAFETY: plain queries with valid out pointers.
        unsafe {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
                dpi_x = 96;
            }
            let scale = dpi_x.max(48) as f32 / 96.0;
            let full_screen = foreground_covers(monitor, &info.rcMonitor);
            let (area, gap) = if full_screen { (info.rcMonitor, 10.0) } else { (info.rcWork, 8.0) };
            Self { area, gap: (gap * scale).round() as i32, scale }
        }
    }

    fn origin(&self, width: i32, height: i32) -> POINT {
        POINT {
            x: self.area.left + (self.area.right - self.area.left - width) / 2,
            y: self.area.bottom - height - self.gap,
        }
    }
}

/// Whether the foreground window covers the whole monitor (a full-screen video, game, slides).
fn foreground_covers(monitor: ::windows::Win32::Graphics::Gdi::HMONITOR, bounds: &RECT) -> bool {
    // SAFETY: plain queries with valid out pointers.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() || MonitorFromWindow(window, MONITOR_DEFAULTTONULL) != monitor {
            return false;
        }
        // The desktop itself covers the monitor but isn't full screen.
        if matches!(super::app::class_name(window).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(window, &mut rect).is_err() {
            return false;
        }
        rect.left <= bounds.left && rect.top <= bounds.top && rect.right >= bounds.right && rect.bottom >= bounds.bottom
    }
}

/// A 32-bit top-down DIB section selected into a memory DC.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Surface {
    fn new(width: i32, height: i32) -> anyhow::Result<Self> {
        // SAFETY: GDI objects are created here and released in `Drop`; `bits` points to the
        // DIB's `width * height * 4` bytes for as long as the bitmap lives.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                anyhow::bail!("CreateCompatibleDC failed");
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top-down, like the renderer's rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(bitmap) if !bits.is_null() => bitmap,
                result => {
                    if let Ok(bitmap) = result {
                        let _ = DeleteObject(bitmap.into());
                    }
                    let _ = DeleteDC(dc);
                    anyhow::bail!("CreateDIBSection failed");
                }
            };
            let previous = SelectObject(dc, bitmap.into());
            Ok(Self { dc, bitmap, previous, bits: bits.cast(), width, height })
        }
    }

    /// Copies premultiplied RGBA into the DIB's premultiplied BGRA.
    fn copy_from(&mut self, rgba: &[u8]) {
        let len = (self.width * self.height * 4) as usize;
        // SAFETY: `bits` is valid for `len` bytes (see `new`) and only accessed through `self`.
        let bgra = unsafe { std::slice::from_raw_parts_mut(self.bits, len) };
        for (dst, &[r, g, b, a]) in bgra.as_chunks_mut::<4>().0.iter_mut().zip(rgba.as_chunks::<4>().0) {
            *dst = [b, g, r, a];
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: undoing `new`, in reverse order.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

struct OverlayWindow {
    hwnd: HWND,
    renderer: Renderer,
    events: Sender<PlatformEvent>,
    levels: Arc<Mutex<[f32; 5]>>,
    epoch: Instant,
    /// What's showing; `None` while hidden.
    state: Option<OverlayState>,
    placement: Placement,
    placement_checked: Instant,
    surface: Option<Surface>,
    /// The Undo button, in window pixels.
    undo: Option<RECT>,
    undo_pressed: bool,
    interactive: bool,
}

impl OverlayWindow {
    fn create(events: Sender<PlatformEvent>, levels: Arc<Mutex<[f32; 5]>>) -> anyhow::Result<Self> {
        // SAFETY: plain Win32 calls with 'static class and window names.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(overlay_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("KuduOverlay"),
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                anyhow::bail!("RegisterClassExW failed: {:?}", GetLastError());
            }
            // Layered (per-pixel alpha), click-through, above other windows, out of the
            // taskbar and Alt + Tab, and never activated by clicks.
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("KuduOverlay"),
                w!("Kudu"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        Ok(Self {
            hwnd,
            renderer: Renderer::new(),
            events,
            levels,
            epoch: Instant::now(),
            state: None,
            placement: Placement::current(),
            placement_checked: Instant::now(),
            surface: None,
            undo: None,
            undo_pressed: false,
            interactive: false,
        })
    }

    fn show(&mut self, state: OverlayState) {
        let was_visible = self.state.is_some();
        if !was_visible {
            self.placement = Placement::current();
            self.placement_checked = Instant::now();
        }
        self.set_interactive(state.is_interactive());
        let animates = state.animates();
        let hide_after = state.auto_hide_after();
        self.state = Some(state);
        self.undo_pressed = false;
        self.render();
        // SAFETY: plain window calls on our own window.
        unsafe {
            if !was_visible {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            // Stay above windows that became topmost since.
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            let interval = if animates { OVERLAY_FRAME } else { PLACEMENT_CHECK };
            SetTimer(Some(self.hwnd), FRAME_TIMER, interval.as_millis() as u32, None);
            match hide_after {
                Some(after) => {
                    SetTimer(Some(self.hwnd), HIDE_TIMER, after.as_millis() as u32, None);
                }
                None => {
                    let _ = KillTimer(Some(self.hwnd), HIDE_TIMER);
                }
            }
        }
    }

    fn hide(&mut self) {
        if self.state.take().is_none() {
            return;
        }
        // SAFETY: plain window calls on our own window.
        unsafe {
            let _ = KillTimer(Some(self.hwnd), FRAME_TIMER);
            let _ = KillTimer(Some(self.hwnd), HIDE_TIMER);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.set_interactive(false);
        self.undo = None;
        self.undo_pressed = false;
    }

    /// Takes clicks (for Undo) or lets them through to the windows below.
    fn set_interactive(&mut self, interactive: bool) {
        if interactive == self.interactive {
            return;
        }
        self.interactive = interactive;
        // SAFETY: plain window calls on our own window.
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let transparent = WS_EX_TRANSPARENT.0 as isize;
            let style = if interactive { style & !transparent } else { style | transparent };
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, style);
            let _ = SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }

    fn render(&mut self) {
        let Some(state) = &self.state else { return };
        let levels = *self.levels.lock();
        let time = self.epoch.elapsed().as_secs_f64();
        let frame = self.renderer.render(state, &levels, time, self.placement.scale);
        let (width, height) = (frame.pixmap.width() as i32, frame.pixmap.height() as i32);
        if self.surface.as_ref().is_none_or(|s| s.width != width || s.height != height) {
            self.surface = None; // free the old one first
            match Surface::new(width, height) {
                Ok(surface) => self.surface = Some(surface),
                Err(error) => {
                    log::error!("Couldn't create the overlay's bitmap: {error:#}");
                    return;
                }
            }
        }
        let Some(surface) = self.surface.as_mut() else { return };
        surface.copy_from(frame.pixmap.data());
        self.undo = frame.undo.map(|r| RECT {
            left: r.left().floor() as i32,
            top: r.top().floor() as i32,
            right: r.right().ceil() as i32,
            bottom: r.bottom().ceil() as i32,
        });
        let origin = self.placement.origin(width, height);
        let size = SIZE { cx: width, cy: height };
        let source = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: all pointers are to locals valid for the call; the DC holds the DIB.
        let result = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&origin),
                Some(&size),
                Some(surface.dc),
                Some(&source),
                Default::default(),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        if let Err(error) = result {
            log::warn!("UpdateLayeredWindow failed: {error}");
        }
    }

    fn on_frame_timer(&mut self) {
        let Some(state) = &self.state else { return };
        let animates = state.animates();
        let mut moved = false;
        if self.placement_checked.elapsed() >= PLACEMENT_CHECK {
            self.placement_checked = Instant::now();
            let placement = Placement::current();
            if placement != self.placement {
                self.placement = placement;
                moved = true;
            }
        }
        if animates || moved {
            self.render();
        }
    }

    fn over_undo(&self, x: i32, y: i32) -> bool {
        self.undo.is_some_and(|r| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
    }

    fn cursor_over_undo(&self) -> bool {
        let mut point = POINT::default();
        // SAFETY: plain queries with valid out pointers.
        unsafe {
            if GetCursorPos(&mut point).is_err() || !ScreenToClient(self.hwnd, &mut point).as_bool() {
                return false;
            }
        }
        self.over_undo(point.x, point.y)
    }

    fn on_mouse(&mut self, x: i32, y: i32, up: bool) {
        if !self.interactive {
            return;
        }
        if !up {
            self.undo_pressed = self.over_undo(x, y);
        } else if std::mem::take(&mut self.undo_pressed) && self.over_undo(x, y) {
            let _ = self.events.send(PlatformEvent::UndoLearned);
            self.hide();
        }
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = std::panic::catch_unwind(|| handle(msg, wparam, lparam)).unwrap_or_else(|_| {
        log::error!("The overlay's window procedure panicked");
        None
    });
    match handled {
        Some(result) => result,
        // SAFETY: forwarding the unhandled message unchanged.
        None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn handle(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    match msg {
        // Clicking the pill must not take focus from the app the text goes to.
        WM_MOUSEACTIVATE => Some(LRESULT(MA_NOACTIVATE as isize)),
        WM_TIMER => {
            match wparam.0 {
                FRAME_TIMER => {
                    with_overlay(OverlayWindow::on_frame_timer);
                }
                HIDE_TIMER => {
                    with_overlay(OverlayWindow::hide);
                }
                _ => return None,
            }
            Some(LRESULT(0))
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP => {
            with_overlay(|overlay| overlay.on_mouse(x, y, msg == WM_LBUTTONUP));
            Some(LRESULT(0))
        }
        WM_SETCURSOR => {
            if with_overlay(|overlay| overlay.interactive && overlay.cursor_over_undo()) == Some(true) {
                // SAFETY: plain call with a system cursor.
                unsafe {
                    if let Ok(hand) = LoadCursorW(None, IDC_HAND) {
                        SetCursor(Some(hand));
                    }
                }
                return Some(LRESULT(1));
            }
            None
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with_overlay(|overlay| {
                overlay.placement = Placement::current();
                overlay.render();
            });
            Some(LRESULT(0))
        }
        WM_SETTINGCHANGE => {
            // The taskbar may have switched between light and dark.
            super::tray::theme_changed();
            None
        }
        _ => None,
    }
}
