//! The overlay pill: what it shows, and a software renderer shared by Windows and Linux.
//!
//! The platform creates a transparent, click-through, always-on-top window of
//! [`CANVAS_WIDTH`] × [`CANVAS_HEIGHT`] logical pixels near the bottom of the screen and copies
//! [`Renderer::render`]'s pixels into it each frame while something animates. The pill is drawn
//! centred in the canvas and sizes itself to its content, like the macOS app's.

use std::path::Path;
use std::time::Duration;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

pub const CANVAS_WIDTH: f32 = 380.0;
pub const CANVAS_HEIGHT: f32 = 44.0;
pub const PILL_HEIGHT: f32 = 26.0;

#[derive(Clone, Debug, PartialEq)]
pub enum OverlayState {
    /// Listening. `locked`: hands-free (a breathing red dot). `command`: Command mode (sparkle).
    Recording { locked: bool, command: bool },
    /// The speech model (or the language model, for commands) is working.
    Transcribing { command: bool },
    /// A short message ("Didn't catch that").
    Message(String),
    /// Words just saved to the dictionary, with an Undo button.
    Learned(Vec<String>),
}

impl OverlayState {
    /// Whether the pill takes clicks (only for Undo); otherwise clicks pass through.
    pub fn is_interactive(&self) -> bool {
        matches!(self, OverlayState::Learned(_))
    }

    /// Whether the pill changes from frame to frame (so the platform keeps redrawing).
    pub fn animates(&self) -> bool {
        matches!(
            self,
            OverlayState::Recording { .. } | OverlayState::Transcribing { .. }
        )
    }

    /// How long the state stays before the pill hides itself, if it does.
    pub fn auto_hide_after(&self) -> Option<Duration> {
        match self {
            OverlayState::Message(_) => Some(Duration::from_millis(2000)),
            OverlayState::Learned(_) => Some(Duration::from_millis(5000)),
            _ => None,
        }
    }
}

/// One rendered frame.
pub struct Frame {
    /// Premultiplied RGBA, `CANVAS_WIDTH * scale` × `CANVAS_HEIGHT * scale` pixels.
    pub pixmap: Pixmap,
    /// Where the Undo button is, in the frame's pixels.
    pub undo: Option<Rect>,
    /// The pill itself, in the frame's pixels (to limit the clickable area where the platform
    /// shapes input by region, as on Linux).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub pill: Rect,
}

pub struct Renderer {
    regular: FontArc,
    bold: Option<FontArc>,
}

const VIOLET: (u8, u8, u8) = (184, 158, 255);
const RED: (u8, u8, u8) = (255, 69, 58);

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    pub fn new() -> Self {
        let regular = load_font(REGULAR_FONTS)
            .unwrap_or_else(|| FontArc::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("bundled font"));
        let bold = load_font(BOLD_FONTS);
        Self { regular, bold }
    }

    /// Draws the pill for `state`. `levels` is the loudness history (newest first), `time` a
    /// clock in seconds for animations, `scale` the display's scale factor (1.0, 1.25, 2.0…).
    pub fn render(&self, state: &OverlayState, levels: &[f32; 5], time: f64, scale: f32) -> Frame {
        let width = (CANVAS_WIDTH * scale).round() as u32;
        let height = (CANVAS_HEIGHT * scale).round() as u32;
        let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("non-empty canvas");
        let font_size = 11.0 * scale;

        // Measure the content to size the pill.
        let mut parts: Vec<Part> = Vec::new();
        match state {
            OverlayState::Recording { locked, command } => {
                if *command {
                    parts.push(Part::Sparkle);
                    parts.push(Part::Gap(7.0));
                } else if *locked {
                    parts.push(Part::RedDot);
                    parts.push(Part::Gap(7.0));
                }
                parts.push(Part::Waveform);
            }
            OverlayState::Transcribing { command } => {
                if *command {
                    parts.push(Part::Sparkle);
                    parts.push(Part::Gap(7.0));
                }
                parts.push(Part::Dots { violet: *command });
            }
            OverlayState::Message(text) => {
                parts.push(Part::Text { text: text.clone(), bold: false, alpha: 0.92 });
            }
            OverlayState::Learned(words) => {
                parts.push(Part::Sparkle);
                parts.push(Part::Gap(6.0));
                parts.push(Part::Text { text: short(words), bold: true, alpha: 1.0 });
                parts.push(Part::Text { text: " saved to dictionary".into(), bold: false, alpha: 0.62 });
                parts.push(Part::Gap(6.0));
                parts.push(Part::Divider);
                parts.push(Part::Gap(6.0));
                parts.push(Part::Undo);
            }
        }
        let content_width: f32 = parts.iter().map(|p| self.part_width(p, font_size, scale)).sum();
        let padding = 12.0 * scale;
        let pill_height = PILL_HEIGHT * scale;
        let pill_width = (content_width + 2.0 * padding).min(width as f32 - 8.0 * scale);
        let pill = Rect::from_xywh(
            ((width as f32 - pill_width) / 2.0).round(),
            ((height as f32 - pill_height) / 2.0).round(),
            pill_width.round(),
            pill_height,
        )
        .expect("pill rect");

        draw_background(&mut pixmap, pill, scale);

        let mut x = pill.x() + padding;
        let mid = pill.y() + pill.height() / 2.0;
        let mut undo = None;
        for part in &parts {
            let w = self.part_width(part, font_size, scale);
            match part {
                Part::Gap(_) => {}
                Part::Waveform => draw_waveform(&mut pixmap, x, mid, levels, scale),
                Part::Dots { violet } => draw_dots(&mut pixmap, x, mid, time, *violet, scale),
                Part::RedDot => {
                    let pulse = 0.55 + 0.45 * (0.5 + 0.5 * (time * std::f64::consts::TAU / 1.6).sin()) as f32;
                    fill_circle(&mut pixmap, x + 3.0 * scale, mid, 3.0 * scale, RED, pulse);
                }
                Part::Sparkle => draw_sparkle(&mut pixmap, x + 5.0 * scale, mid, 5.0 * scale),
                Part::Divider => {
                    let rect = Rect::from_xywh(x, mid - 5.5 * scale, (0.75 * scale).max(1.0), 11.0 * scale).unwrap();
                    fill_rect(&mut pixmap, rect, (255, 255, 255), 0.18);
                }
                Part::Text { text, bold, alpha } => {
                    let font = if *bold { self.bold.as_ref().unwrap_or(&self.regular) } else { &self.regular };
                    draw_text(&mut pixmap, font, font_size, x, mid, text, (255, 255, 255), *alpha, *bold && self.bold.is_none());
                }
                Part::Undo => {
                    let font = self.bold.as_ref().unwrap_or(&self.regular);
                    draw_text(&mut pixmap, font, font_size, x, mid, "Undo", VIOLET, 1.0, self.bold.is_none());
                    undo = Rect::from_xywh(x - 4.0 * scale, pill.y(), w + 8.0 * scale, pill.height());
                }
            }
            x += w;
        }
        Frame { pixmap, undo, pill }
    }

    fn part_width(&self, part: &Part, font_size: f32, scale: f32) -> f32 {
        match part {
            Part::Gap(gap) => gap * scale,
            Part::Waveform => (BAR_COUNT as f32 * BAR_WIDTH + (BAR_COUNT - 1) as f32 * BAR_SPACING) * scale,
            Part::Dots { .. } => 40.0 * scale,
            Part::RedDot => 6.0 * scale,
            Part::Sparkle => 10.0 * scale,
            Part::Divider => (0.75 * scale).max(1.0),
            Part::Text { text, bold, .. } => {
                let font = if *bold { self.bold.as_ref().unwrap_or(&self.regular) } else { &self.regular };
                measure(font, font_size, text) + if *bold && self.bold.is_none() { 0.5 * scale } else { 0.0 }
            }
            Part::Undo => measure(self.bold.as_ref().unwrap_or(&self.regular), font_size, "Undo"),
        }
    }
}

enum Part {
    Gap(f32),
    Waveform,
    Dots { violet: bool },
    RedDot,
    Sparkle,
    Divider,
    Text { text: String, bold: bool, alpha: f32 },
    Undo,
}

/// The learned words, kept short enough for the pill.
fn short(words: &[String]) -> String {
    let text = words.join(", ");
    if text.chars().count() > 24 {
        format!("{}…", text.chars().take(22).collect::<String>())
    } else {
        text
    }
}

// MARK: - Fonts

#[cfg(windows)]
const REGULAR_FONTS: &[&str] = &["C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\arial.ttf"];
#[cfg(windows)]
const BOLD_FONTS: &[&str] = &["C:\\Windows\\Fonts\\seguisb.ttf", "C:\\Windows\\Fonts\\segoeuib.ttf", "C:\\Windows\\Fonts\\arialbd.ttf"];

#[cfg(not(windows))]
const REGULAR_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
    "/usr/share/fonts/inter/Inter-Regular.ttf",
    "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
    "/usr/share/fonts/cantarell/Cantarell-VF.otf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
];
#[cfg(not(windows))]
const BOLD_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/inter/Inter-SemiBold.ttf",
    "/usr/share/fonts/inter/Inter-SemiBold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/google-noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu-M.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
];

fn load_font(candidates: &[&str]) -> Option<FontArc> {
    candidates.iter().find_map(|path| {
        let data = std::fs::read(Path::new(path)).ok()?;
        FontArc::try_from_vec(data).ok()
    })
}

fn measure(font: &FontArc, size: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut previous = None;
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(previous) = previous {
            width += scaled.kern(previous, id);
        }
        width += scaled.h_advance(id);
        previous = Some(id);
    }
    width
}

/// Draws text vertically centred on `mid`. `fake_bold` thickens a regular font when no bold
/// face is installed.
#[allow(clippy::too_many_arguments)]
fn draw_text(
    pixmap: &mut Pixmap,
    font: &FontArc,
    size: f32,
    x: f32,
    mid: f32,
    text: &str,
    color: (u8, u8, u8),
    alpha: f32,
    fake_bold: bool,
) {
    let scaled = font.as_scaled(PxScale::from(size));
    let baseline = mid + (scaled.ascent() + scaled.descent()) / 2.0;
    let offsets: &[f32] = if fake_bold { &[0.0, 0.5] } else { &[0.0] };
    for &dx in offsets {
        let mut pen = x + dx;
        let mut previous = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(previous) = previous {
                pen += scaled.kern(previous, id);
            }
            let glyph = id.with_scale_and_position(PxScale::from(size), ab_glyph::point(pen, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = bounds.min.x as i32 + gx as i32;
                    let py = bounds.min.y as i32 + gy as i32;
                    blend_pixel(pixmap, px, py, color, (coverage * alpha).clamp(0.0, 1.0));
                });
            }
            pen += scaled.h_advance(id);
            previous = Some(id);
        }
    }
}

fn blend_pixel(pixmap: &mut Pixmap, x: i32, y: i32, color: (u8, u8, u8), alpha: f32) {
    if x < 0 || y < 0 || x >= pixmap.width() as i32 || y >= pixmap.height() as i32 || alpha <= 0.0 {
        return;
    }
    let index = (y as usize * pixmap.width() as usize + x as usize) * 4;
    let data = pixmap.data_mut();
    // Premultiplied "source over".
    let a = alpha;
    let src = [color.0 as f32 * a, color.1 as f32 * a, color.2 as f32 * a, 255.0 * a];
    for channel in 0..4 {
        let dst = data[index + channel] as f32;
        data[index + channel] = (src[channel] + dst * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
    }
}

// MARK: - Shapes

fn paint(color: (u8, u8, u8), alpha: f32) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.0, color.1, color.2, (alpha.clamp(0.0, 1.0) * 255.0) as u8));
    paint.anti_alias = true;
    paint
}

fn rounded_rect(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let (l, t, rr, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    // Cubic approximation of a quarter circle.
    let k = 0.552_284_8 * r;
    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(rr - r, t);
    pb.cubic_to(rr - r + k, t, rr, t + r - k, rr, t + r);
    pb.line_to(rr, b - r);
    pb.cubic_to(rr, b - r + k, rr - r + k, b, rr - r, b);
    pb.line_to(l + r, b);
    pb.cubic_to(l + r - k, b, l, b - r + k, l, b - r);
    pb.line_to(l, t + r);
    pb.cubic_to(l, t + r - k, l + r - k, t, l + r, t);
    pb.close();
    pb.finish()
}

fn fill_rect(pixmap: &mut Pixmap, rect: Rect, color: (u8, u8, u8), alpha: f32) {
    pixmap.fill_rect(rect, &paint(color, alpha), Transform::identity(), None);
}

fn fill_capsule(pixmap: &mut Pixmap, rect: Rect, color: (u8, u8, u8), alpha: f32) {
    if let Some(path) = rounded_rect(rect, rect.width().min(rect.height()) / 2.0) {
        pixmap.fill_path(&path, &paint(color, alpha), FillRule::Winding, Transform::identity(), None);
    }
}

fn fill_circle(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32, color: (u8, u8, u8), alpha: f32) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, radius) {
        pixmap.fill_path(&path, &paint(color, alpha), FillRule::Winding, Transform::identity(), None);
    }
}

/// Dark glass with a hairline border and a soft shadow.
fn draw_background(pixmap: &mut Pixmap, pill: Rect, scale: f32) {
    // Shadow: a few widening, fading capsules below the pill.
    for step in 1..=5 {
        let grow = step as f32 * 1.2 * scale;
        if let Some(rect) = Rect::from_xywh(pill.x() - grow, pill.y() - grow + 2.0 * scale, pill.width() + 2.0 * grow, pill.height() + 2.0 * grow) {
            fill_capsule(pixmap, rect, (0, 0, 0), 0.05);
        }
    }
    fill_capsule(pixmap, pill, (22, 22, 26), 0.88);
    if let Some(path) = rounded_rect(pill, pill.height() / 2.0) {
        let stroke = tiny_skia::Stroke { width: (0.5 * scale).max(0.75), ..Default::default() };
        pixmap.stroke_path(&path, &paint((255, 255, 255), 0.16), &stroke, Transform::identity(), None);
    }
}

const BAR_COUNT: usize = 9;
const BAR_WIDTH: f32 = 2.5;
const BAR_SPACING: f32 = 2.5;

/// Nine bars mirrored around the middle: the middle shows the newest loudness, and each step
/// outward an older one with a cosine-squared falloff, so speech ripples outward.
fn draw_waveform(pixmap: &mut Pixmap, x: f32, mid: f32, levels: &[f32; 5], scale: f32) {
    let (min_h, max_h) = (3.0, 14.0);
    let center = BAR_COUNT / 2;
    for index in 0..BAR_COUNT {
        let distance = index.abs_diff(center);
        let level = levels[distance.min(levels.len() - 1)].clamp(0.0, 1.0);
        let t = distance as f32 / (center + 1) as f32;
        let falloff = 0.25 + 0.75 * (t * std::f32::consts::FRAC_PI_2).cos().powi(2);
        let h = (min_h + (max_h - min_h) * level * falloff) * scale;
        let bx = x + index as f32 * (BAR_WIDTH + BAR_SPACING) * scale;
        if let Some(rect) = Rect::from_xywh(bx, mid - h / 2.0, BAR_WIDTH * scale, h) {
            fill_capsule(pixmap, rect, (255, 255, 255), 0.95);
        }
    }
}

/// Three dots that rise and fall in turn.
fn draw_dots(pixmap: &mut Pixmap, x: f32, mid: f32, time: f64, violet: bool, scale: f32) {
    let color = if violet { VIOLET } else { (255, 255, 255) };
    let start = x + (40.0 - (3.0 * 4.0 + 2.0 * 4.0)) / 2.0 * scale;
    for index in 0..3 {
        let phase = ((time * std::f64::consts::TAU / 0.9) - index as f64 * 0.7).sin() as f32;
        let cx = start + (2.0 + index as f32 * 8.0) * scale;
        fill_circle(pixmap, cx, mid - 1.5 * phase * scale, 2.0 * scale, color, 0.45 + 0.55 * (0.5 + 0.5 * phase));
    }
}

/// A four-pointed violet sparkle with a faint glow.
fn draw_sparkle(pixmap: &mut Pixmap, cx: f32, cy: f32, radius: f32) {
    fill_circle(pixmap, cx, cy, radius * 0.9, VIOLET, 0.12);
    let inner = radius * 0.28;
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - radius);
    pb.quad_to(cx + inner * 0.3, cy - inner * 0.3, cx + radius, cy);
    pb.quad_to(cx + inner * 0.3, cy + inner * 0.3, cx, cy + radius);
    pb.quad_to(cx - inner * 0.3, cy + inner * 0.3, cx - radius, cy);
    pb.quad_to(cx - inner * 0.3, cy - inner * 0.3, cx, cy - radius);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &paint(VIOLET, 1.0), FillRule::Winding, Transform::identity(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_every_state_inside_the_canvas() {
        let renderer = Renderer::new();
        let levels = [0.8, 0.6, 0.4, 0.2, 0.1];
        for state in [
            OverlayState::Recording { locked: false, command: false },
            OverlayState::Recording { locked: true, command: false },
            OverlayState::Recording { locked: false, command: true },
            OverlayState::Transcribing { command: false },
            OverlayState::Message("Didn't catch that".into()),
            OverlayState::Learned(vec!["heatmap".into()]),
        ] {
            for scale in [1.0, 1.5, 2.0] {
                let frame = renderer.render(&state, &levels, 0.4, scale);
                assert_eq!(frame.pixmap.width(), (CANVAS_WIDTH * scale) as u32);
                assert!(frame.pill.right() <= frame.pixmap.width() as f32);
                assert!(frame.pixmap.data().iter().any(|&b| b > 0), "something was drawn");
            }
        }
        let frame = renderer.render(&OverlayState::Learned(vec!["heatmap".into()]), &levels, 0.0, 1.0);
        let undo = frame.undo.expect("undo button");
        assert!(undo.left() >= frame.pill.left() && undo.right() <= frame.pill.right() + 1.0);
        if std::env::var_os("KUDU_OVERLAY_PNG").is_some() {
            let dir = std::env::var("KUDU_OVERLAY_PNG").unwrap();
            for (i, state) in [
                OverlayState::Recording { locked: false, command: false },
                OverlayState::Recording { locked: true, command: false },
                OverlayState::Transcribing { command: true },
                OverlayState::Learned(vec!["heat map".into()]),
            ]
            .iter()
            .enumerate()
            {
                renderer.render(state, &levels, 0.4, 2.0).pixmap.save_png(format!("{dir}/pill{i}.png")).unwrap();
            }
        }
    }

    #[test]
    fn keeps_learned_words_short() {
        assert_eq!(short(&["heatmap".into()]), "heatmap");
        assert!(short(&["a very long learned phrase".into(), "and another one".into()]).ends_with('…'));
    }
}
