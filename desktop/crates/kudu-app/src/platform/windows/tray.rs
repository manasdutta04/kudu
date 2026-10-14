//! The notification-area icon and its menu, on the UI thread.

use std::cell::RefCell;

use anyhow::Context;
use crossbeam_channel::Sender;
use image::imageops::{self, FilterType};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::platform::{PlatformEvent, TrayAction, TrayState};

const ICON_PNG: &[u8] = include_bytes!("../../../assets/kudu-icon.png");
const SIZE: u32 = 32;
const RED: [u8; 3] = [255, 69, 58];
const ORANGE: [u8; 3] = [255, 159, 10];

const OPEN_ID: &str = "kudu.open";
const PASTE_ID: &str = "kudu.paste-last";
const QUIT_ID: &str = "kudu.quit";

struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    /// The glyph's coverage, `SIZE` × `SIZE`.
    glyph: Vec<u8>,
    state: TrayState,
    light_taskbar: bool,
}

thread_local! {
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

/// Creates the icon (on the UI thread). Calling it again only updates where events go.
pub fn start(events: Sender<PlatformEvent>) -> anyhow::Result<()> {
    let menu_events = events.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = match event.id().0.as_str() {
            OPEN_ID => TrayAction::OpenWindow,
            PASTE_ID => TrayAction::PasteLast,
            QUIT_ID => TrayAction::Quit,
            _ => return,
        };
        let _ = menu_events.send(PlatformEvent::Tray(action));
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            let _ = events.send(PlatformEvent::Tray(TrayAction::OpenWindow));
        }
    }));
    if TRAY.with(|cell| cell.borrow().is_some()) {
        return Ok(());
    }

    let glyph = load_glyph()?;
    let light_taskbar = taskbar_is_light();
    let status = MenuItem::new("Kudu", false, None);
    let menu = Menu::new();
    menu.append_items(&[
        &status,
        &MenuItem::with_id(OPEN_ID, "Open Kudu", true, None),
        &MenuItem::with_id(PASTE_ID, "Paste last transcript", true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(QUIT_ID, "Quit Kudu", true, None),
    ])
    .context("building the tray menu")?;
    let icon = TrayIconBuilder::new()
        .with_id("kudu")
        .with_icon(compose(&glyph, TrayState::Idle, light_taskbar)?)
        .with_tooltip("Kudu")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .context("creating the tray icon")?;
    TRAY.with(|cell| {
        *cell.borrow_mut() = Some(Tray { icon, status, glyph, state: TrayState::Idle, light_taskbar });
    });
    Ok(())
}

pub fn set_state(state: TrayState, tooltip: &str) {
    let tooltip = truncate_utf16(tooltip, 127); // the notification area's limit
    with_tray(|tray| {
        if tray.state != state {
            tray.state = state;
            match compose(&tray.glyph, state, tray.light_taskbar) {
                Ok(icon) => {
                    if let Err(error) = tray.icon.set_icon(Some(icon)) {
                        log::warn!("Couldn't update the tray icon: {error}");
                    }
                }
                Err(error) => log::warn!("Couldn't draw the tray icon: {error:#}"),
            }
        }
        if let Err(error) = tray.icon.set_tooltip(Some(&tooltip)) {
            log::warn!("Couldn't update the tray tooltip: {error}");
        }
        tray.status.set_text(&tooltip);
    });
}

/// Redraws the icon if the taskbar switched between light and dark.
pub fn theme_changed() {
    with_tray(|tray| {
        let light = taskbar_is_light();
        if light != tray.light_taskbar {
            tray.light_taskbar = light;
            if let Ok(icon) = compose(&tray.glyph, tray.state, light) {
                let _ = tray.icon.set_icon(Some(icon));
            }
        }
    });
}

fn with_tray(f: impl FnOnce(&mut Tray)) {
    TRAY.with(|cell| {
        if let Ok(mut tray) = cell.try_borrow_mut() {
            if let Some(tray) = tray.as_mut() {
                f(tray);
            }
        }
    });
}

fn truncate_utf16(text: &str, max_units: usize) -> String {
    let mut units = 0;
    let mut out = String::new();
    for c in text.chars() {
        units += c.len_utf16();
        if units > max_units {
            break;
        }
        out.push(c);
    }
    out
}

/// Whether the taskbar uses the light theme (where a white glyph would vanish).
fn taskbar_is_light() -> bool {
    windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_u32("SystemUsesLightTheme"))
        .is_ok_and(|value| value == 1)
}

/// The app icon's glyph (white on transparent), cropped to its bounds and scaled to `SIZE`,
/// as coverage values.
fn load_glyph() -> anyhow::Result<Vec<u8>> {
    let image = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)
        .context("decoding the tray icon")?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 16 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right <= left || bottom <= top {
        anyhow::bail!("the tray icon is empty");
    }
    // A square around the glyph with a small margin, so it fills the notification area's slot.
    let side = (right - left).max(bottom - top);
    let side = side + side / 16;
    let cx = (left + right) / 2;
    let cy = (top + bottom) / 2;
    let x0 = cx.saturating_sub(side / 2);
    let y0 = cy.saturating_sub(side / 2);
    let mut square = image::RgbaImage::new(side, side);
    imageops::overlay(
        &mut square,
        &imageops::crop_imm(&image, x0, y0, side.min(width - x0), side.min(height - y0)).to_image(),
        0,
        0,
    );
    let scaled = imageops::resize(&square, SIZE, SIZE, FilterType::Lanczos3);
    Ok(scaled.pixels().map(|pixel| pixel[3]).collect())
}

/// The icon for a state: the glyph in the taskbar's text colour, with a coloured dot while
/// recording (red) or when setup needs attention (orange).
fn compose(glyph: &[u8], state: TrayState, light_taskbar: bool) -> anyhow::Result<Icon> {
    let ink: [u8; 3] = if light_taskbar { [28, 28, 30] } else { [255, 255, 255] };
    let dot = match state {
        TrayState::Recording => Some(RED),
        TrayState::NeedsSetup => Some(ORANGE),
        TrayState::Idle | TrayState::Working => None,
    };
    let (cx, cy, radius) = (25.0f32, 25.0f32, 5.5f32);
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut alpha = glyph[(y * SIZE + x) as usize] as f32 / 255.0;
            let mut color = ink;
            if let Some(dot_color) = dot {
                let distance = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                // Clear a thin ring around the dot so it reads as a badge on the glyph.
                let ring = (radius + 1.5 - distance + 0.5).clamp(0.0, 1.0);
                alpha *= 1.0 - ring;
                let coverage = (radius - distance + 0.5).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let blended = coverage + alpha * (1.0 - coverage);
                    for channel in 0..3 {
                        let over =
                            dot_color[channel] as f32 * coverage + color[channel] as f32 * alpha * (1.0 - coverage);
                        color[channel] = (over / blended.max(f32::EPSILON)).round().clamp(0.0, 255.0) as u8;
                    }
                    alpha = blended;
                }
            }
            rgba.extend_from_slice(&[color[0], color[1], color[2], (alpha * 255.0).round() as u8]);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).context("creating the tray icon image")
}
