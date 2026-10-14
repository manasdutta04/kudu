//! The tray icon, as a StatusNotifierItem over D-Bus (KDE, most wlroots bars, and GNOME with
//! the AppIndicator extension; the old XEmbed tray isn't supported).

use std::collections::HashMap;
use std::sync::OnceLock;

use crossbeam_channel::Sender;
use image::imageops::FilterType;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip};
use parking_lot::Mutex;

use crate::platform::{PlatformEvent, TrayAction, TrayState};

const ICON_PNG: &[u8] = include_bytes!("../../../assets/kudu-icon.png");
const ICON_SIZES: [u32; 2] = [32, 64];
const RED: (u8, u8, u8) = (255, 59, 48);
const ORANGE: (u8, u8, u8) = (255, 149, 0);

pub struct Tray {
    handle: Mutex<Option<Handle<KuduTray>>>,
    /// The state set before the tray started.
    pending: Mutex<(TrayState, String)>,
}

impl Tray {
    pub fn new() -> Self {
        Self { handle: Mutex::new(None), pending: Mutex::new((TrayState::Idle, String::new())) }
    }

    pub fn start(&self, events: Sender<PlatformEvent>) -> anyhow::Result<()> {
        let mut handle = self.handle.lock();
        if handle.is_some() {
            return Ok(());
        }
        let (state, tooltip) = self.pending.lock().clone();
        let tray = KuduTray { state, tooltip, events };
        // Starting before the desktop's tray host (at login) is fine: the item appears once
        // the host comes up.
        match tray.assume_sni_available(true).spawn() {
            Ok(h) => {
                *handle = Some(h);
                Ok(())
            }
            Err(e) => {
                log::warn!("No tray icon: {e}");
                Ok(())
            }
        }
    }

    pub fn set_state(&self, state: TrayState, tooltip: &str) {
        *self.pending.lock() = (state, tooltip.to_string());
        let handle = self.handle.lock().clone();
        if let Some(handle) = handle {
            let tooltip = tooltip.to_string();
            handle.update(move |tray| {
                tray.state = state;
                tray.tooltip = tooltip;
            });
        }
    }
}

struct KuduTray {
    state: TrayState,
    tooltip: String,
    events: Sender<PlatformEvent>,
}

impl KuduTray {
    fn send(&self, action: TrayAction) {
        let _ = self.events.send(PlatformEvent::Tray(action));
    }

    fn status_line(&self) -> String {
        if !self.tooltip.is_empty() {
            return self.tooltip.clone();
        }
        match self.state {
            TrayState::Idle => "Kudu is ready",
            TrayState::Recording => "Listening…",
            TrayState::Working => "Transcribing…",
            TrayState::NeedsSetup => "Kudu needs setting up",
        }
        .to_string()
    }
}

impl ksni::Tray for KuduTray {
    fn id(&self) -> String {
        "kudu".into()
    }

    fn title(&self) -> String {
        "Kudu".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        icons(self.state)
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip { title: "Kudu".into(), description: self.status_line(), ..Default::default() }
    }

    fn status(&self) -> ksni::Status {
        match self.state {
            TrayState::NeedsSetup => ksni::Status::NeedsAttention,
            _ => ksni::Status::Active,
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayAction::OpenWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem { label: self.status_line().replace('_', "__"), enabled: false, ..Default::default() }.into(),
            StandardItem {
                label: "Open Kudu".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::OpenWindow)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Paste last transcript".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::PasteLast)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit Kudu".into(),
                activate: Box::new(|tray: &mut Self| tray.send(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// The icon for a state, at each size: the app icon with a small red (recording) or orange
/// (needs setup) dot.
fn icons(state: TrayState) -> Vec<Icon> {
    static CACHE: OnceLock<Mutex<HashMap<u8, Vec<Icon>>>> = OnceLock::new();
    let dot = match state {
        TrayState::Recording => Some(RED),
        TrayState::NeedsSetup => Some(ORANGE),
        TrayState::Idle | TrayState::Working => None,
    };
    let key = match dot {
        None => 0,
        Some(RED) => 1,
        Some(_) => 2,
    };
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(icons) = cache.lock().get(&key) {
        return icons.iter().map(|i| Icon { width: i.width, height: i.height, data: i.data.clone() }).collect();
    }
    let Ok(source) = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png) else {
        log::warn!("The bundled tray icon couldn't be decoded");
        return Vec::new();
    };
    let source = source.to_rgba8();
    let icons: Vec<Icon> = ICON_SIZES
        .iter()
        .map(|&size| {
            let mut rgba = image::imageops::resize(&source, size, size, FilterType::Lanczos3).into_raw();
            add_outline(&mut rgba, size);
            if let Some(color) = dot {
                draw_dot(&mut rgba, size, color);
            }
            Icon { width: size as i32, height: size as i32, data: rgba_to_argb32(&rgba) }
        })
        .collect();
    let copy = icons.iter().map(|i| Icon { width: i.width, height: i.height, data: i.data.clone() }).collect();
    cache.lock().insert(key, copy);
    icons
}

/// Straight RGBA to the StatusNotifierItem's ARGB32 in network byte order (A, R, G, B).
fn rgba_to_argb32(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, a]| [a, r, g, b]).collect()
}

/// The glyph is white on transparent, which vanishes on light panels: put a soft dark edge
/// under it so it reads on light and dark panels alike.
fn add_outline(rgba: &mut [u8], size: u32) {
    let radius = (size as i32 / 32).max(1);
    let n = size as i32;
    let alpha = |x: i32, y: i32| -> f32 {
        if x < 0 || y < 0 || x >= n || y >= n {
            0.0
        } else {
            rgba[((y * n + x) * 4 + 3) as usize] as f32 / 255.0
        }
    };
    let mut halo = vec![0f32; (n * n) as usize];
    for y in 0..n {
        for x in 0..n {
            let mut most = 0f32;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy <= radius * radius {
                        most = most.max(alpha(x + dx, y + dy));
                    }
                }
            }
            halo[(y * n + x) as usize] = most * 0.45;
        }
    }
    for (i, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let a = pixel[3] as f32 / 255.0;
        let under = halo[i] * (1.0 - a);
        let out = a + under;
        if out <= 0.0 {
            continue;
        }
        // Glyph over a black halo (straight alpha).
        for c in pixel.iter_mut().take(3) {
            *c = ((*c as f32 * a) / out).round().clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
    }
}

/// A filled, anti-aliased dot with a white ring in the bottom-right corner (straight RGBA).
fn draw_dot(rgba: &mut [u8], size: u32, color: (u8, u8, u8)) {
    let s = size as f32;
    let radius = s * 0.2;
    let ring = (s / 32.0).max(1.0);
    let (cx, cy) = (s - radius - ring, s - radius - ring);
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let outer = (radius + ring + 0.5 - d).clamp(0.0, 1.0);
            if outer <= 0.0 {
                continue;
            }
            let inner = (radius + 0.5 - d).clamp(0.0, 1.0);
            // Dot colour inside, white ring around it.
            let fill = [
                color.0 as f32 * inner + 255.0 * (1.0 - inner),
                color.1 as f32 * inner + 255.0 * (1.0 - inner),
                color.2 as f32 * inner + 255.0 * (1.0 - inner),
            ];
            let i = ((y * size + x) * 4) as usize;
            let dst_a = rgba[i + 3] as f32 / 255.0;
            let out_a = outer + dst_a * (1.0 - outer);
            for c in 0..3 {
                let dst = rgba[i + c] as f32;
                let value = if out_a > 0.0 { (fill[c] * outer + dst * dst_a * (1.0 - outer)) / out_a } else { 0.0 };
                rgba[i + c] = value.round().clamp(0.0, 255.0) as u8;
            }
            rgba[i + 3] = (out_a * 255.0).round() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_network_order_argb() {
        assert_eq!(rgba_to_argb32(&[1, 2, 3, 4, 5, 6, 7, 8]), vec![4, 1, 2, 3, 8, 5, 6, 7]);
    }

    #[test]
    fn makes_icons_with_state_dots() {
        let idle = icons(TrayState::Idle);
        assert_eq!(idle.len(), 2);
        assert_eq!((idle[0].width, idle[0].height, idle[0].data.len()), (32, 32, 32 * 32 * 4));
        assert_eq!(idle[1].data.len(), 64 * 64 * 4);
        let recording = icons(TrayState::Recording);
        // The centre of the dot (bottom right) is red and opaque.
        let size = 64usize;
        let r = 64.0 * 0.2;
        let c = (64.0 - r - 2.0) as usize;
        let i = (c * size + c) * 4;
        assert_eq!(&recording[1].data[i..i + 4], &[255, RED.0, RED.1, RED.2]);
        assert_ne!(recording[1].data, idle[1].data);
    }

    #[test]
    fn outlines_the_white_glyph() {
        // A white 2 × 2 square in a transparent 8 × 8 icon gets a dark, translucent edge.
        let size = 8u32;
        let mut rgba = vec![0u8; (size * size * 4) as usize];
        for (x, y) in [(3, 3), (4, 3), (3, 4), (4, 4)] {
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
        add_outline(&mut rgba, size);
        let px = |x: u32, y: u32| &rgba[((y * size + x) * 4) as usize..((y * size + x) * 4 + 4) as usize];
        assert_eq!(px(3, 3), &[255, 255, 255, 255]);
        assert_eq!(px(2, 3), &[0, 0, 0, 115]);
        assert_eq!(px(0, 0)[3], 0);
    }
}
