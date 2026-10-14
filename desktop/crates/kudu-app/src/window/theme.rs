//! Colors, type and the egui style, ported from the Mac app's `Theme.swift`.

use eframe::egui::{
    self, epaint::text::VariationCoords, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Shadow,
    Stroke, Theme, Ui, Visuals,
};
use std::path::Path;
use std::sync::Arc;

/// The window's colors in the current appearance.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Window and sidebar background.
    pub canvas: Color32,
    /// The rounded content panel.
    pub panel: Color32,
    /// Cards inside the panel.
    pub card: Color32,
    /// Selected sidebar row, control fills.
    pub selection: Color32,
    pub hairline: Color32,
    /// A soft wash for hovered rows.
    pub hover: Color32,
    pub text: Color32,
    pub secondary: Color32,
    pub tertiary: Color32,
    pub accent: Color32,
    pub green: Color32,
    pub orange: Color32,
    pub red: Color32,
}

pub const ACCENT: Color32 = Color32::from_rgb(107, 84, 242);
/// The midnight gradient, top-leading to bottom-trailing.
pub const MIDNIGHT: (Color32, Color32) = (Color32::from_rgb(46, 36, 102), Color32::from_rgb(15, 13, 33));

const LIGHT: Palette = Palette {
    dark: false,
    canvas: Color32::from_rgb(0xF2, 0xF1, 0xEE),
    panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    card: Color32::from_rgb(0xF8, 0xF7, 0xF4),
    selection: Color32::from_rgb(0xE4, 0xE2, 0xDD),
    hairline: Color32::from_rgba_premultiplied(0, 0, 0, 20),
    hover: Color32::from_rgba_premultiplied(0, 0, 0, 6),
    text: Color32::from_rgb(0x1F, 0x1E, 0x1C),
    secondary: Color32::from_rgb(0x79, 0x77, 0x72),
    tertiary: Color32::from_rgb(0xB4, 0xB2, 0xAC),
    accent: ACCENT,
    green: Color32::from_rgb(0x2F, 0xA8, 0x4F),
    orange: Color32::from_rgb(0xE8, 0x8A, 0x1A),
    red: Color32::from_rgb(0xD9, 0x3B, 0x30),
};

const DARK: Palette = Palette {
    dark: true,
    canvas: Color32::from_rgb(0x14, 0x14, 0x16),
    panel: Color32::from_rgb(0x1D, 0x1D, 0x20),
    card: Color32::from_rgb(0x25, 0x25, 0x29),
    selection: Color32::from_rgb(0x2A, 0x2A, 0x2F),
    hairline: Color32::from_rgba_premultiplied(20, 20, 20, 20),
    hover: Color32::from_rgba_premultiplied(6, 6, 6, 6),
    text: Color32::from_rgb(0xEC, 0xEB, 0xE8),
    secondary: Color32::from_rgb(0x9A, 0x99, 0x9F),
    tertiary: Color32::from_rgb(0x5E, 0x5D, 0x63),
    accent: Color32::from_rgb(0x86, 0x72, 0xF7),
    green: Color32::from_rgb(0x3C, 0xC2, 0x62),
    orange: Color32::from_rgb(0xF0, 0x9A, 0x30),
    red: Color32::from_rgb(0xF0, 0x55, 0x4A),
};

impl Palette {
    pub fn of(ui: &Ui) -> Palette {
        if ui.visuals().dark_mode {
            DARK
        } else {
            LIGHT
        }
    }

    /// A color at an opacity (0…1), premultiplied.
    pub fn alpha(color: Color32, opacity: f32) -> Color32 {
        color.gamma_multiply(opacity)
    }

    /// Fill for the selected state of a control on a card.
    pub fn raised(&self) -> Color32 {
        if self.dark {
            Color32::from_rgb(0x3A, 0x3A, 0x40)
        } else {
            self.panel
        }
    }
}

/// Weights of the interface font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
}

const MEDIUM: &str = "kudu-medium";
const SEMIBOLD: &str = "kudu-semibold";
const SERIF: &str = "kudu-serif";

pub fn font(size: f32, weight: Weight) -> FontId {
    match weight {
        Weight::Regular => FontId::proportional(size),
        Weight::Medium => FontId::new(size, FontFamily::Name(MEDIUM.into())),
        Weight::Semibold => FontId::new(size, FontFamily::Name(SEMIBOLD.into())),
    }
}

/// Large serif display type, for page titles and headline numbers.
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SERIF.into()))
}

/// A font file on this system, with the face index and the weight for variable fonts.
struct SystemFont {
    path: String,
    index: u32,
}

fn load(font: &SystemFont, weight: Option<f32>) -> Option<FontData> {
    let lower = font.path.to_ascii_lowercase();
    if !(lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc")) {
        return None;
    }
    let bytes = std::fs::read(&font.path).ok()?;
    let mut data = FontData::from_owned(bytes);
    data.index = font.index;
    if let Some(weight) = weight {
        data.tweak.coords = VariationCoords::new([(b"wght", weight)]);
    }
    Some(data)
}

fn existing(paths: &[&str]) -> Option<SystemFont> {
    paths.iter().find(|p| Path::new(p).is_file()).map(|p| SystemFont { path: (*p).to_string(), index: 0 })
}

/// Asks fontconfig for the desktop's font for a pattern ("sans-serif:weight=medium").
#[cfg(target_os = "linux")]
fn fc_match(pattern: &str) -> Option<SystemFont> {
    let output = std::process::Command::new("fc-match").args(["-f", "%{file}|%{index}", pattern]).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let (path, index) = text.trim().split_once('|')?;
    let index: u32 = index.trim().parse().unwrap_or(0);
    // Named instances of variable fonts come back as `instance << 16 | face`.
    Some(SystemFont { path: path.to_string(), index: index & 0xFFFF }).filter(|f| Path::new(&f.path).is_file())
}

#[cfg(not(target_os = "linux"))]
fn file(paths: &[&str], weight: Option<f32>) -> Option<FontData> {
    existing(paths).and_then(|font| load(&font, weight))
}

/// The fonts to use: (regular, medium, semibold, serif). Each may be missing.
#[cfg(windows)]
fn system_fonts() -> [Option<FontData>; 4] {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let at = |name: &str| format!("{dir}\\Fonts\\{name}");
    let serif = file(&[&at("georgia.ttf")], None);
    // Windows 11's variable Segoe UI has every weight; older systems have the static faces.
    if let Some(variable) = existing(&[&at("SegUIVar.ttf")]) {
        return [load(&variable, Some(400.0)), load(&variable, Some(500.0)), load(&variable, Some(600.0)), serif];
    }
    let semibold = file(&[&at("seguisb.ttf")], None);
    [file(&[&at("segoeui.ttf")], None), semibold.clone(), semibold, serif]
}

#[cfg(target_os = "linux")]
fn system_fonts() -> [Option<FontData>; 4] {
    let pick = |pattern: &str, fallbacks: &[&str]| fc_match(pattern).or_else(|| existing(fallbacks));
    let regular = pick(
        "sans-serif:weight=regular",
        &[
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ],
    );
    let medium = pick(
        "sans-serif:weight=medium",
        &["/usr/share/fonts/truetype/noto/NotoSans-Medium.ttf", "/usr/share/fonts/noto/NotoSans-Medium.ttf"],
    );
    let semibold = pick(
        "sans-serif:weight=semibold",
        &[
            "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
            "/usr/share/fonts/noto/NotoSans-SemiBold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        ],
    );
    let serif = pick(
        "serif",
        &[
            "/usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf",
            "/usr/share/fonts/noto/NotoSerif-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
            "/usr/share/fonts/TTF/DejaVuSerif.ttf",
        ],
    );
    let regular_data = regular.as_ref().and_then(|f| load(f, Some(400.0)));
    // Without a medium face, medium text uses the regular one (bold reads too heavy).
    let medium_data = medium.as_ref().and_then(|f| load(f, Some(500.0))).or_else(|| regular_data.clone());
    [regular_data, medium_data, semibold.as_ref().and_then(|f| load(f, Some(600.0))), serif.as_ref().and_then(|f| load(f, None))]
}

#[cfg(not(any(windows, target_os = "linux")))]
fn system_fonts() -> [Option<FontData>; 4] {
    // Development on a Mac: the system face, and Georgia as on Windows.
    let sans = "/System/Library/Fonts/SFNS.ttf";
    [
        file(&[sans], Some(400.0)),
        file(&[sans], Some(510.0)),
        file(&[sans], Some(590.0)),
        file(&["/System/Library/Fonts/Supplemental/Georgia.ttf"], None),
    ]
}

/// Installs the system fonts (when present) over egui's defaults.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let [regular, medium, semibold, serif] = system_fonts();

    let mut family = |name: &str, data: Option<FontData>, fallback: Option<&str>| -> Vec<String> {
        let mut list = Vec::new();
        if let Some(data) = data {
            fonts.font_data.insert(name.to_string(), Arc::new(data));
            list.push(name.to_string());
        } else if let Some(fallback) = fallback {
            list.push(fallback.to_string());
        }
        list
    };
    let has_regular = regular.is_some();
    let mut regular_list = family("kudu-regular", regular, None);
    let mut medium_list = family("kudu-medium-face", medium, has_regular.then_some("kudu-regular"));
    let mut semibold_list = family("kudu-semibold-face", semibold, has_regular.then_some("kudu-regular"));
    let mut serif_list = family("kudu-serif-face", serif, has_regular.then_some("kudu-regular"));
    for list in [&mut regular_list, &mut medium_list, &mut semibold_list, &mut serif_list] {
        list.extend(defaults.iter().cloned());
    }
    fonts.families.insert(FontFamily::Proportional, regular_list);
    fonts.families.insert(FontFamily::Name(MEDIUM.into()), medium_list);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semibold_list);
    fonts.families.insert(FontFamily::Name(SERIF.into()), serif_list);
    ctx.set_fonts(fonts);
}

/// Quiet egui visuals in the app's colors, for both appearances.
pub fn install_style(ctx: &egui::Context) {
    for (theme, palette) in [(Theme::Light, LIGHT), (Theme::Dark, DARK)] {
        let mut visuals = if palette.dark { Visuals::dark() } else { Visuals::light() };
        visuals.panel_fill = palette.canvas;
        visuals.window_fill = palette.panel;
        visuals.faint_bg_color = palette.card;
        visuals.extreme_bg_color = palette.card;
        visuals.code_bg_color = palette.selection;
        visuals.hyperlink_color = palette.accent;
        visuals.selection.bg_fill = palette.accent.gamma_multiply(0.32);
        visuals.selection.stroke = Stroke::new(1.0, palette.accent);
        visuals.text_cursor.stroke = Stroke::new(1.5, palette.accent);
        visuals.window_stroke = Stroke::new(1.0, palette.hairline);
        visuals.window_corner_radius = CornerRadius::same(12);
        visuals.menu_corner_radius = CornerRadius::same(10);
        visuals.window_shadow = Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(40) };
        visuals.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(30) };
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.fg_stroke.color = palette.text;
            widget.bg_stroke = Stroke::NONE;
            widget.corner_radius = CornerRadius::same(7);
            widget.expansion = 0.0;
        }
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.hairline);
        visuals.widgets.inactive.weak_bg_fill = palette.selection;
        visuals.widgets.inactive.bg_fill = palette.selection;
        visuals.widgets.hovered.weak_bg_fill = palette.selection;
        visuals.widgets.hovered.bg_fill = palette.selection;
        visuals.widgets.active.weak_bg_fill = palette.selection;
        visuals.widgets.active.bg_fill = palette.selection;
        visuals.widgets.noninteractive.fg_stroke.color = palette.text;
        visuals.weak_text_color = Some(palette.secondary);
        ctx.set_visuals_of(theme, visuals);
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(12.0, 5.0);
            style.spacing.interact_size.y = 22.0;
            style.spacing.menu_margin = egui::Margin::same(6);
            style.interaction.selectable_labels = false;
            style.interaction.tooltip_delay = 0.4;
            style.url_in_tooltip = false;
            style.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.5));
            style.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.0));
            style.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.5));
            style.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(12.5));
        });
    }
    ctx.set_theme(egui::ThemePreference::System);
}
