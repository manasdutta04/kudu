//! Line icons, the Kudu mark and the midnight gradient, drawn with egui's painter so they stay
//! crisp at any scale and need no icon font.

use eframe::egui::{
    epaint::{Mesh, PathShape, PathStroke, Vertex, WHITE_UV},
    pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2,
};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Insights,
    Dictionary,
    Snippets,
    Style,
    Settings,
    Copy,
    Ellipsis,
    Trash,
    Pencil,
    Search,
    Close,
    Plus,
    Chevrons,
    Check,
    Sparkle,
    ArrowRight,
    Waveform,
    Message,
    Bubbles,
    Envelope,
    Document,
    Code,
    Clock,
    Warning,
    Download,
    Meetings,
    ChevronLeft,
    ChevronRight,
    Circle,
    Dot,
}

/// Draws `icon` centered in `rect` (designed on a 16 × 16 grid).
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let o = rect.center() - vec2(8.0, 8.0) * s;
    let p = |x: f32, y: f32| o + vec2(x, y) * s;
    let stroke = Stroke::new(1.35 * s.max(0.75), color);
    let line = |points: Vec<Pos2>| {
        painter.add(PathShape::line(points, PathStroke::new(stroke.width, color)));
    };
    let closed = |points: Vec<Pos2>| {
        painter.add(PathShape::closed_line(points, PathStroke::new(stroke.width, color)));
    };
    let rounded = |r: Rect, radius: f32| {
        painter.rect_stroke(r, radius * s, stroke, eframe::egui::StrokeKind::Middle);
    };
    match icon {
        Icon::Home => {
            closed(vec![p(2.5, 7.5), p(8.0, 2.8), p(13.5, 7.5), p(13.5, 13.5), p(2.5, 13.5)]);
            line(vec![p(6.5, 13.5), p(6.5, 9.8), p(9.5, 9.8), p(9.5, 13.5)]);
        }
        Icon::Insights => {
            line(vec![p(2.0, 13.8), p(14.0, 13.8)]);
            for (x, top) in [(4.0, 8.5), (8.0, 3.0), (12.0, 6.0)] {
                rounded(Rect::from_min_max(p(x - 1.3, top), p(x + 1.3, 11.6)), 0.8);
            }
        }
        Icon::Dictionary => {
            rounded(Rect::from_min_max(p(3.0, 2.2), p(13.0, 13.8)), 1.6);
            line(vec![p(5.6, 2.2), p(5.6, 13.8)]);
            line(vec![p(7.8, 5.6), p(10.8, 5.6)]);
        }
        Icon::Snippets => {
            line(vec![p(2.5, 4.0), p(13.5, 4.0)]);
            line(vec![p(2.5, 7.5), p(9.0, 7.5)]);
            line(vec![p(2.5, 11.0), p(7.0, 11.0)]);
            line(vec![p(12.0, 8.8), p(12.0, 14.2)]);
            line(vec![p(9.3, 11.5), p(14.7, 11.5)]);
        }
        Icon::Style => {
            line(vec![p(2.6, 13.4), p(10.0, 6.0)]);
            sparkle(painter, p(11.6, 4.4), 3.0 * s, color);
            sparkle(painter, p(5.0, 3.8), 1.6 * s, color);
            sparkle(painter, p(13.0, 10.5), 1.5 * s, color);
        }
        Icon::Settings => {
            let c = p(8.0, 8.0);
            painter.circle_stroke(c, 2.1 * s, stroke);
            let teeth = 8;
            let mut outline = Vec::new();
            for i in 0..teeth {
                let a = i as f32 / teeth as f32 * TAU;
                let half = PI / teeth as f32;
                for (angle, radius) in
                    [(a - half * 0.95, 4.6), (a - half * 0.45, 6.2), (a + half * 0.45, 6.2), (a + half * 0.95, 4.6)]
                {
                    outline.push(c + Vec2::angled(angle) * radius * s);
                }
            }
            closed(outline);
        }
        Icon::Copy => {
            rounded(Rect::from_min_max(p(5.5, 5.5), p(13.5, 13.5)), 1.8);
            line(vec![p(10.5, 3.6), p(10.5, 3.2), p(9.8, 2.5), p(3.2, 2.5), p(2.5, 3.2), p(2.5, 9.8), p(3.2, 10.5), p(3.6, 10.5)]);
        }
        Icon::Ellipsis => {
            for x in [3.5, 8.0, 12.5] {
                painter.circle_filled(p(x, 8.0), 1.25 * s, color);
            }
        }
        Icon::Trash => {
            line(vec![p(2.5, 4.2), p(13.5, 4.2)]);
            line(vec![p(6.0, 4.2), p(6.0, 2.5), p(10.0, 2.5), p(10.0, 4.2)]);
            line(vec![p(3.9, 4.2), p(4.7, 13.5), p(11.3, 13.5), p(12.1, 4.2)]);
            line(vec![p(6.8, 6.8), p(7.0, 11.0)]);
            line(vec![p(9.2, 6.8), p(9.0, 11.0)]);
        }
        Icon::Pencil => {
            closed(vec![p(10.6, 2.6), p(13.4, 5.4), p(5.6, 13.2), p(2.4, 13.6), p(2.8, 10.4)]);
            line(vec![p(9.2, 4.0), p(12.0, 6.8)]);
        }
        Icon::Search => {
            painter.circle_stroke(p(7.0, 7.0), 4.4 * s, stroke);
            line(vec![p(10.3, 10.3), p(13.8, 13.8)]);
        }
        Icon::Close => {
            line(vec![p(4.0, 4.0), p(12.0, 12.0)]);
            line(vec![p(12.0, 4.0), p(4.0, 12.0)]);
        }
        Icon::Plus => {
            line(vec![p(8.0, 3.0), p(8.0, 13.0)]);
            line(vec![p(3.0, 8.0), p(13.0, 8.0)]);
        }
        Icon::Chevrons => {
            line(vec![p(4.8, 6.4), p(8.0, 3.4), p(11.2, 6.4)]);
            line(vec![p(4.8, 9.6), p(8.0, 12.6), p(11.2, 9.6)]);
        }
        Icon::Check => line(vec![p(3.2, 8.4), p(6.6, 11.6), p(12.8, 4.6)]),
        Icon::Meetings => {
            // Two people side by side.
            painter.circle_stroke(p(5.6, 5.4), 2.2 * s, stroke);
            line(vec![p(1.6, 13.2), p(1.9, 11.4), p(3.2, 9.8), p(5.6, 9.2), p(8.0, 9.8), p(9.3, 11.4), p(9.6, 13.2)]);
            painter.circle_stroke(p(11.0, 4.6), 1.9 * s, stroke);
            line(vec![p(10.6, 8.3), p(12.8, 8.6), p(14.1, 9.9), p(14.4, 11.8)]);
        }
        Icon::ChevronLeft => line(vec![p(10.0, 3.0), p(5.0, 8.0), p(10.0, 13.0)]),
        Icon::ChevronRight => line(vec![p(6.0, 3.0), p(11.0, 8.0), p(6.0, 13.0)]),
        Icon::Circle => {
            painter.circle_stroke(p(8.0, 8.0), 5.0 * s, stroke);
        }
        Icon::Dot => {
            painter.circle_filled(p(8.0, 8.0), 2.6 * s, color);
        }
        Icon::Sparkle => sparkle(painter, p(8.0, 8.0), 6.5 * s, color),
        Icon::ArrowRight => {
            line(vec![p(2.5, 8.0), p(13.0, 8.0)]);
            line(vec![p(9.0, 4.0), p(13.0, 8.0), p(9.0, 12.0)]);
        }
        Icon::Waveform => {
            for (x, h) in [(2.5, 3.0), (5.25, 7.0), (8.0, 11.0), (10.75, 6.0), (13.5, 3.5)] {
                painter.line_segment([p(x, 8.0 - h / 2.0), p(x, 8.0 + h / 2.0)], stroke);
                painter.circle_filled(p(x, 8.0 - h / 2.0), stroke.width / 2.0, color);
                painter.circle_filled(p(x, 8.0 + h / 2.0), stroke.width / 2.0, color);
            }
        }
        Icon::Message => {
            closed(vec![
                p(4.0, 2.8),
                p(12.0, 2.8),
                p(13.6, 4.4),
                p(13.6, 9.6),
                p(12.0, 11.2),
                p(7.0, 11.2),
                p(4.0, 13.6),
                p(4.2, 11.2),
                p(4.0, 11.2),
                p(2.4, 9.6),
                p(2.4, 4.4),
            ]);
        }
        Icon::Bubbles => {
            closed(vec![p(2.0, 2.8), p(10.0, 2.8), p(10.0, 8.6), p(5.0, 8.6), p(2.6, 10.8), p(2.6, 8.6), p(2.0, 8.6)]);
            line(vec![p(11.6, 5.6), p(14.0, 5.6), p(14.0, 11.4), p(13.4, 11.4), p(13.4, 13.6), p(11.0, 11.4), p(6.4, 11.4), p(6.4, 10.4)]);
        }
        Icon::Envelope => {
            rounded(Rect::from_min_max(p(2.0, 3.5), p(14.0, 12.5)), 1.6);
            line(vec![p(2.6, 4.4), p(8.0, 8.6), p(13.4, 4.4)]);
        }
        Icon::Document => {
            closed(vec![p(3.5, 2.0), p(9.6, 2.0), p(12.5, 4.9), p(12.5, 14.0), p(3.5, 14.0)]);
            line(vec![p(5.8, 7.6), p(10.2, 7.6)]);
            line(vec![p(5.8, 10.4), p(10.2, 10.4)]);
        }
        Icon::Code => {
            line(vec![p(5.2, 4.2), p(1.8, 8.0), p(5.2, 11.8)]);
            line(vec![p(10.8, 4.2), p(14.2, 8.0), p(10.8, 11.8)]);
            line(vec![p(9.2, 3.0), p(6.8, 13.0)]);
        }
        Icon::Clock => {
            painter.circle_stroke(p(8.0, 8.0), 6.0 * s, stroke);
            line(vec![p(8.0, 4.4), p(8.0, 8.0), p(10.6, 9.6)]);
        }
        Icon::Warning => {
            closed(vec![p(8.0, 2.0), p(14.4, 13.4), p(1.6, 13.4)]);
            line(vec![p(8.0, 6.2), p(8.0, 9.4)]);
            painter.circle_filled(p(8.0, 11.4), 0.9 * s, color);
        }
        Icon::Download => {
            line(vec![p(8.0, 2.2), p(8.0, 10.0)]);
            line(vec![p(4.6, 6.8), p(8.0, 10.2), p(11.4, 6.8)]);
            line(vec![p(2.8, 13.6), p(13.2, 13.6)]);
        }
    }
}

/// A four-pointed star, filled.
pub fn sparkle(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    let mut points = Vec::with_capacity(8 * 4);
    let steps = 6;
    // Concave sides: sample curves between the four tips.
    for tip in 0..4 {
        let a0 = tip as f32 * FRAC_PI_2 - FRAC_PI_2;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let angle = a0 + t * FRAC_PI_2;
            // Radius dips to ~28% halfway between the tips.
            let r = radius * (0.28 + 0.72 * ((t - 0.5).abs() * 2.0).powf(2.2));
            points.push(center + Vec2::angled(angle) * r);
        }
    }
    fill_star(painter, center, &points, color);
}

/// Fills a star-shaped polygon (visible from `center`) with an anti-aliased edge.
fn fill_star(painter: &Painter, center: Pos2, points: &[Pos2], color: Color32) {
    let feather = 0.7 / painter.pixels_per_point();
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex { pos: center, uv: WHITE_UV, color });
    let n = points.len() as u32;
    for &point in points {
        let out = (point - center).normalized();
        mesh.vertices.push(Vertex { pos: point - out * feather * 0.5, uv: WHITE_UV, color });
        mesh.vertices.push(Vertex { pos: point + out * feather, uv: WHITE_UV, color: Color32::TRANSPARENT });
    }
    for i in 0..n {
        let (a, b) = (1 + 2 * i, 1 + 2 * ((i + 1) % n));
        mesh.add_triangle(0, a, b);
        mesh.add_triangle(a, a + 1, b);
        mesh.add_triangle(b, a + 1, b + 1);
    }
    painter.add(Shape::mesh(mesh));
}

/// The outline of a rounded rectangle, clockwise from the top-left corner's arc.
fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let segments = 10;
    let corners = [
        (pos2(rect.left() + r, rect.top() + r), PI),
        (pos2(rect.right() - r, rect.top() + r), PI * 1.5),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), FRAC_PI_2),
    ];
    let mut points = Vec::with_capacity(4 * (segments + 1));
    for (center, start) in corners {
        for i in 0..=segments {
            let angle = start + i as f32 / segments as f32 * FRAC_PI_2;
            points.push(center + Vec2::angled(angle) * r);
        }
    }
    points
}

/// A rounded rectangle filled with a diagonal gradient from `from` (top-left) to `to`.
pub fn gradient_rect(painter: &Painter, rect: Rect, radius: f32, from: Color32, to: Color32) {
    painter.add(gradient_mesh(rect, radius, from, to, painter.pixels_per_point()));
}

/// The shape for `gradient_rect`.
pub fn gradient_mesh(rect: Rect, radius: f32, from: Color32, to: Color32, pixels_per_point: f32) -> Shape {
    let outline = rounded_outline(rect, radius);
    let center = rect.center();
    let diagonal = rect.max - rect.min;
    let length = diagonal.length_sq();
    let color_at = |point: Pos2| {
        let t = ((point - rect.min).dot(diagonal) / length).clamp(0.0, 1.0);
        lerp(from, to, t)
    };
    let feather = 0.8 / pixels_per_point;
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex { pos: center, uv: WHITE_UV, color: color_at(center) });
    let n = outline.len() as u32;
    for &point in &outline {
        let out = (point - center).normalized();
        let color = color_at(point);
        mesh.vertices.push(Vertex { pos: point, uv: WHITE_UV, color });
        mesh.vertices.push(Vertex { pos: point + out * feather, uv: WHITE_UV, color: Color32::TRANSPARENT });
    }
    for i in 0..n {
        let (a, b) = (1 + 2 * i, 1 + 2 * ((i + 1) % n));
        mesh.add_triangle(0, a, b);
        mesh.add_triangle(a, a + 1, b);
        mesh.add_triangle(b, a + 1, b + 1);
    }
    Shape::mesh(mesh)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()), mix(a.a(), b.a()))
}

/// The Kudu logo: a "K" whose middle is a three-bar waveform. Geometry from the master
/// artwork (`public/kudu-icon.png`, 1254 × 1254), the same as the Mac app's `KuduMark`.
pub fn kudu_mark(painter: &Painter, rect: Rect, color: Color32) {
    let content = Rect::from_min_size(pos2(272.0, 218.0), vec2(762.0, 876.0));
    let scale = (rect.width() / content.width()).min(rect.height() / content.height());
    let origin = rect.center() - content.size() * scale / 2.0;
    let map = |x: f32, y: f32| origin + (vec2(x, y) - content.min.to_vec2()) * scale;

    let capsule = |min: Pos2, size: Vec2| {
        let r = Rect::from_min_size(map(min.x, min.y), size * scale);
        painter.rect_filled(r, r.width().min(r.height()) / 2.0, color);
    };
    capsule(pos2(292.0, 252.0), vec2(144.0, 800.0));
    for (center_x, width, height) in [(490.0, 68.0, 196.0), (586.0, 80.0, 374.0), (684.0, 68.0, 196.0)] {
        capsule(pos2(center_x - width / 2.0, 656.0 - height / 2.0), vec2(width, height));
    }
    // The arms: capsules 401 long and 144 thick, rotated ±45°.
    let (length, thickness) = (401.0, 144.0);
    for (center, angle) in [(pos2(851.0, 402.0), -FRAC_PI_4), (pos2(843.0, 911.0), FRAC_PI_4)] {
        let half = Vec2::angled(angle) * ((length - thickness) / 2.0);
        let a = map(center.x - half.x, center.y - half.y);
        let b = map(center.x + half.x, center.y + half.y);
        let radius = thickness / 2.0 * scale;
        let mut points = Vec::new();
        let segments = 12;
        for (end, start_angle) in [(b, (b - a).angle() - FRAC_PI_2), (a, (a - b).angle() - FRAC_PI_2)] {
            for i in 0..=segments {
                points.push(end + Vec2::angled(start_angle + i as f32 / segments as f32 * PI) * radius);
            }
        }
        painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
}
