//! The window's controls, ported from the Mac app's `Controls.swift` and page building blocks:
//! cards, sections and rows, an accent switch, quiet buttons, value menus, chip pickers,
//! keycaps, fields and intro cards.

use eframe::egui::{
    self, pos2, text::LayoutJob, vec2, Align, Color32, CursorIcon, FontId, Frame, Id, Label, Layout, Margin,
    Popup, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, TextEdit, Ui, UiBuilder, Vec2,
};

use super::icons::{self, Icon};
use super::theme::{display, font, Palette, Weight, MIDNIGHT};

/// Text in the interface font.
pub fn text(content: impl Into<String>, size: f32, weight: Weight, color: Color32) -> RichText {
    RichText::new(content).font(font(size, weight)).color(color)
}

/// A label that wraps at the available width.
pub fn wrapped(ui: &mut Ui, content: RichText) -> Response {
    ui.add(Label::new(content).wrap())
}

/// Small uppercase heading with letter spacing ("HISTORY", "TODAY").
pub fn caps(content: &str, size: f32, color: Color32) -> RichText {
    RichText::new(content.to_uppercase()).font(font(size, Weight::Semibold)).color(color).extra_letter_spacing(0.8)
}

/// Title block shared by the pages.
pub fn page_header(ui: &mut Ui, title: &str, subtitle: &str) {
    let pal = Palette::of(ui);
    ui.label(RichText::new(title).font(display(30.0)).color(pal.text));
    ui.add_space(4.0);
    wrapped(ui, text(subtitle, 13.5, Weight::Regular, pal.secondary));
}

pub fn card_frame(pal: &Palette, radius: u8, margin: Margin) -> Frame {
    Frame::new().fill(pal.card).stroke(Stroke::new(1.0, pal.hairline)).corner_radius(radius).inner_margin(margin)
}

/// A rounded card spanning the available width.
pub fn card<R>(ui: &mut Ui, radius: u8, margin: Margin, add: impl FnOnce(&mut Ui) -> R) -> R {
    let pal = Palette::of(ui);
    card_frame(&pal, radius, margin)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A titled group of rows inside a card, like a grouped settings section.
pub fn section(ui: &mut Ui, title: &str, footer: Option<&str>, add: impl FnOnce(&mut Ui)) {
    let pal = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(caps(title, 11.0, pal.secondary));
    });
    ui.add_space(2.0);
    card(ui, 14, Margin::ZERO, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
    });
    if let Some(footer) = footer {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.set_width(ui.available_width() - 4.0);
            wrapped(ui, text(footer, 11.5, Weight::Regular, pal.secondary));
        });
    }
}

/// A hairline across the available width, inset from the left.
pub fn divider(ui: &mut Ui, inset: f32) {
    let pal = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.left() + inset..=rect.right(), rect.center().y, Stroke::new(1.0, pal.hairline));
}

/// Lays out `left` (top to bottom, taking the remaining width) and `right` (right to left)
/// on one line, both centered vertically. Heights come from the previous pass; a change
/// discards the frame so nothing jumps.
pub fn sides(ui: &mut Ui, id: Id, gap: f32, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    let origin = ui.cursor().min;
    let (prev_left, prev_right): (f32, f32) = ui.data(|d| d.get_temp(id)).unwrap_or((0.0, 0.0));
    let height = prev_left.max(prev_right);

    let right_rect =
        Rect::from_min_size(pos2(origin.x, origin.y + (height - prev_right) / 2.0), vec2(width, prev_right.max(1.0)));
    let mut right_ui = ui.new_child(UiBuilder::new().max_rect(right_rect).layout(Layout::right_to_left(Align::Center)));
    right(&mut right_ui);
    let right_size = if right_ui.min_rect().width() > 0.5 { right_ui.min_rect().size() } else { Vec2::ZERO };

    let left_width = (width - right_size.x - if right_size.x > 0.0 { gap } else { 0.0 }).max(40.0);
    let left_rect = Rect::from_min_size(pos2(origin.x, origin.y + (height - prev_left) / 2.0), vec2(left_width, 0.0));
    let mut left_ui = ui.new_child(UiBuilder::new().max_rect(left_rect).layout(Layout::top_down(Align::Min)));
    left(&mut left_ui);
    let left_height = left_ui.min_rect().height();

    if (left_height - prev_left).abs() > 0.5 || (right_size.y - prev_right).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, (left_height, right_size.y)));
        ui.ctx().request_discard("row heights changed");
    }
    ui.advance_cursor_after_rect(Rect::from_min_size(origin, vec2(width, left_height.max(right_size.y))));
}

/// One row in a `section`: a title (with optional detail) and a control on the right.
pub fn row(ui: &mut Ui, title: &str, detail: Option<&str>, divider_below: bool, control: impl FnOnce(&mut Ui)) {
    let pal = Palette::of(ui);
    let id = ui.id().with(("row", title));
    Frame::new().inner_margin(Margin::symmetric(18, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        sides(
            ui,
            id,
            16.0,
            |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                wrapped(ui, text(title, 13.5, Weight::Regular, pal.text));
                if let Some(detail) = detail {
                    wrapped(ui, text(detail, 11.5, Weight::Regular, pal.secondary));
                }
            },
            control,
        );
    });
    if divider_below {
        divider(ui, 18.0);
    }
}

/// A switch in the accent color.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let pal = Palette::of(ui);
    let size = vec2(34.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.14);
    let off = if pal.dark { Color32::from_rgb(0x3E, 0x3E, 0x45) } else { Color32::from_rgb(0xDB, 0xD9, 0xD3) };
    let track = icons::lerp(off, pal.accent, t);
    let painter = ui.painter();
    painter.rect_filled(rect, rect.height() / 2.0, track);
    let radius = rect.height() / 2.0 - 2.0;
    let x = egui::lerp(rect.left() + 2.0 + radius..=rect.right() - 2.0 - radius, t);
    let center = pos2(x, rect.center().y);
    painter.circle_filled(center + vec2(0.0, 0.6), radius + 0.4, Color32::from_black_alpha(28));
    painter.circle_filled(center, radius, Color32::WHITE);
    response.on_hover_cursor(CursorIcon::PointingHand)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    Quiet,
    Prominent,
    Destructive,
}

/// A soft, borderless button that sits quietly on the panel.
pub fn button(ui: &mut Ui, label: &str, kind: ButtonKind) -> Response {
    button_with_icon(ui, None, label, kind, true)
}

pub fn button_enabled(ui: &mut Ui, label: &str, kind: ButtonKind, enabled: bool) -> Response {
    button_with_icon(ui, None, label, kind, enabled)
}

pub fn button_with_icon(ui: &mut Ui, icon: Option<Icon>, label: &str, kind: ButtonKind, enabled: bool) -> Response {
    let pal = Palette::of(ui);
    let fg = match kind {
        ButtonKind::Prominent => Color32::WHITE,
        ButtonKind::Destructive => pal.red,
        ButtonKind::Quiet => pal.text,
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font(12.5, Weight::Medium), fg);
    let icon_width = if icon.is_some() { 16.0 } else { 0.0 };
    let size = vec2(galley.size().x + 24.0 + icon_width, (galley.size().y + 10.0).max(26.0));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let base = match kind {
        ButtonKind::Prominent => pal.accent,
        _ => pal.selection,
    };
    let hovered = enabled && response.hovered();
    let mut fill = if hovered {
        match kind {
            ButtonKind::Prominent => icons::lerp(pal.accent, Color32::WHITE, 0.1),
            _ => icons::lerp(pal.selection, pal.text, 0.06),
        }
    } else {
        base
    };
    let mut fg = fg;
    if response.is_pointer_button_down_on() {
        fill = fill.gamma_multiply(0.75);
    }
    if !enabled {
        fill = fill.gamma_multiply(0.5);
        fg = fg.gamma_multiply(0.45);
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 7.0, fill);
    let mut x = rect.left() + 12.0;
    if let Some(icon) = icon {
        icons::paint(painter, Rect::from_center_size(pos2(x + 6.0, rect.center().y), Vec2::splat(12.0)), icon, fg);
        x += icon_width;
    }
    painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A small borderless icon button (row actions).
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let pal = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, pal.selection);
    }
    let color = if response.hovered() { pal.text } else { pal.secondary };
    icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(14.0)), icon, color);
    response.on_hover_text(tooltip).on_hover_cursor(CursorIcon::PointingHand)
}

/// One row of a pop-up menu. Returns whether it was clicked.
pub fn menu_item(ui: &mut Ui, label: &str, checked: bool, destructive: bool) -> bool {
    let pal = Palette::of(ui);
    let color = if destructive { pal.red } else { pal.text };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font(13.0, Weight::Regular), color);
    let width = (galley.size().x + 44.0).max(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, pal.selection);
    }
    if checked {
        icons::paint(
            ui.painter(),
            Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), Vec2::splat(12.0)),
            Icon::Check,
            pal.accent,
        );
    }
    ui.painter().galley(pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0), galley, color);
    response.clicked()
}

/// A quiet pop-up: the current value and a small chevron on a soft fill, opening a menu of
/// choices. Returns the newly chosen index.
pub fn value_menu(ui: &mut Ui, id: Id, items: &[String], selected: Option<usize>, placeholder: &str) -> Option<usize> {
    let pal = Palette::of(ui);
    let current = selected.and_then(|i| items.get(i)).map_or(placeholder, |s| s.as_str());
    let galley = ui.painter().layout_no_wrap(current.to_string(), font(13.0, Weight::Regular), pal.text);
    let size = vec2(galley.size().x + 20.0 + 16.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let open = Popup::is_id_open(ui.ctx(), id);
    let fill = if response.hovered() || open { icons::lerp(pal.selection, pal.text, 0.05) } else { pal.selection };
    ui.painter().rect_filled(rect, 7.0, fill);
    ui.painter().galley(pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0), galley, pal.text);
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(rect.right() - 14.0, rect.center().y), Vec2::splat(11.0)),
        Icon::Chevrons,
        pal.secondary,
    );
    let mut chosen = None;
    Popup::menu(&response).id(id).gap(4.0).show(|ui| {
        ui.set_min_width(rect.width().max(160.0));
        ui.spacing_mut().item_spacing.y = 0.0;
        for (index, item) in items.iter().enumerate() {
            if menu_item(ui, item, Some(index) == selected, false) {
                chosen = Some(index);
                ui.close();
            }
        }
    });
    chosen
}

/// A segmented control: a soft track with the selected option raised. With `width`, the
/// options share it equally. Returns the newly chosen index.
pub fn chip_picker(ui: &mut Ui, id: Id, items: &[&str], selected: usize, width: Option<f32>) -> Option<usize> {
    let pal = Palette::of(ui);
    let fonts: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let weight = if i == selected { Weight::Semibold } else { Weight::Regular };
            ui.painter().layout_no_wrap(item.to_string(), font(12.5, weight), pal.text)
        })
        .collect();
    let natural: f32 = fonts.iter().map(|g| g.size().x + 24.0).fold(0.0, f32::max);
    let item_width = width.map_or(natural, |w| ((w - 4.0) / items.len() as f32).max(natural));
    let height = 26.0;
    let (rect, _) = ui.allocate_exact_size(vec2(item_width * items.len() as f32 + 4.0, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, pal.selection);
    let slot = |i: f32| Rect::from_min_size(pos2(rect.left() + 2.0 + i * item_width, rect.top() + 2.0), vec2(item_width, height - 4.0));
    let x = ui.ctx().animate_value_with_time(id, selected as f32, 0.15);
    let raised = slot(x);
    painter.rect_filled(raised.translate(vec2(0.0, 0.8)), 6.0, Color32::from_black_alpha(if pal.dark { 50 } else { 22 }));
    painter.rect_filled(raised, 6.0, pal.raised());
    let mut chosen = None;
    for (i, galley) in fonts.into_iter().enumerate() {
        let r = slot(i as f32);
        let response = ui.interact(r, id.with(i), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        if response.clicked() && i != selected {
            chosen = Some(i);
        }
        let color = if i == selected || response.hovered() { pal.text } else { pal.secondary };
        ui.painter().galley(r.center() - galley.size() / 2.0, galley, color);
    }
    chosen
}

/// A keyboard key, for shortcut hints.
pub fn keycap(ui: &mut Ui, label: &str) {
    let pal = Palette::of(ui);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font(11.5, Weight::Semibold), pal.text);
    let size = vec2((galley.size().x + 14.0).max(22.0), 22.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect.translate(vec2(0.0, 1.0)), 5.0, Color32::from_black_alpha(if pal.dark { 60 } else { 20 }));
    painter.rect(rect, 5.0, pal.raised(), Stroke::new(1.0, Palette::alpha(pal.text, 0.14)), StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, pal.text);
}

/// A shortcut as keycaps, with an optional prefix ("Double-tap"). In a right-to-left row the
/// parts are added in reverse so they read left to right.
pub fn keycaps(ui: &mut Ui, keys: &[String], prefix: Option<&str>) {
    let pal = Palette::of(ui);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let right_to_left = ui.layout().prefer_right_to_left();
        let mut parts: Vec<Option<&str>> = Vec::new();
        if let Some(prefix) = prefix {
            parts.push(Some(prefix));
        }
        parts.extend(keys.iter().map(|k| Some(k.as_str())));
        let prefix_count = usize::from(prefix.is_some());
        let order: Vec<usize> = if right_to_left { (0..parts.len()).rev().collect() } else { (0..parts.len()).collect() };
        for index in order {
            let part = parts[index].unwrap_or_default();
            if index < prefix_count {
                ui.label(text(part, 12.0, Weight::Regular, pal.secondary));
            } else {
                keycap(ui, part);
            }
        }
    });
}

/// A soft single-line field that matches the cards. Returns the text edit's response.
pub fn field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    let pal = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
    let background = ui.painter().add(Shape::Noop);
    let inner = rect.shrink2(vec2(12.0, 8.0));
    let response = ui.put(
        inner,
        TextEdit::singleline(value)
            .frame(Frame::NONE)
            .hint_text(text(hint, 13.0, Weight::Regular, pal.tertiary))
            .font(font(13.0, Weight::Regular))
            .text_color(pal.text)
            .desired_width(inner.width())
            .vertical_align(Align::Center),
    );
    let stroke = if response.has_focus() {
        Stroke::new(1.0, Palette::alpha(pal.accent, 0.7))
    } else {
        Stroke::new(1.0, pal.hairline)
    };
    ui.painter().set(background, egui::epaint::RectShape::new(rect, 9.0, pal.panel, stroke, StrokeKind::Inside));
    response
}

/// The capsule search field.
pub fn search_field(ui: &mut Ui, query: &mut String, width: f32) {
    let pal = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
    ui.painter().rect(rect, 15.0, pal.card, Stroke::new(1.0, pal.hairline), StrokeKind::Inside);
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), Vec2::splat(12.0)),
        Icon::Search,
        pal.secondary,
    );
    let clear_width = if query.is_empty() { 0.0 } else { 22.0 };
    let inner = Rect::from_min_max(pos2(rect.left() + 30.0, rect.top() + 6.0), pos2(rect.right() - 10.0 - clear_width, rect.bottom() - 6.0));
    ui.put(
        inner,
        TextEdit::singleline(query)
            .frame(Frame::NONE)
            .hint_text(text("Search", 13.0, Weight::Regular, pal.tertiary))
            .font(font(13.0, Weight::Regular))
            .text_color(pal.text)
            .desired_width(inner.width())
            .vertical_align(Align::Center),
    );
    if !query.is_empty() {
        let clear = Rect::from_center_size(pos2(rect.right() - 18.0, rect.center().y), Vec2::splat(18.0));
        let response = ui.interact(clear, ui.id().with("clear-search"), Sense::click());
        ui.painter().circle_filled(clear.center(), 7.0, if response.hovered() { pal.secondary } else { pal.tertiary });
        icons::paint(ui.painter(), Rect::from_center_size(clear.center(), Vec2::splat(8.0)), Icon::Close, pal.panel);
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            query.clear();
        }
    }
}

/// A thin progress bar.
pub fn progress_bar(ui: &mut Ui, fraction: f32, width: f32) {
    let pal = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, pal.selection);
    let filled = Rect::from_min_size(rect.min, vec2((rect.width() * fraction.clamp(0.0, 1.0)).max(6.0), rect.height()));
    ui.painter().rect_filled(filled, 3.0, pal.accent);
}

/// A white chip on the midnight intro cards. Clickable when `sense` is click.
pub fn chip(ui: &mut Ui, label: &str, italic: bool, clickable: bool) -> Response {
    let mut rich = text(label, 12.5, Weight::Medium, Color32::WHITE);
    if italic {
        rich = rich.italics();
    }
    let galley = egui::WidgetText::from(rich).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::FontSelection::Default);
    let size = galley.size() + vec2(24.0, 12.0);
    let (rect, response) = ui.allocate_exact_size(size, if clickable { Sense::click() } else { Sense::hover() });
    let alpha = if clickable && response.hovered() { 0.22 } else { 0.14 };
    ui.painter().rect_filled(rect, 8.0, Color32::WHITE.gamma_multiply(alpha));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    if clickable {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A rounded midnight card, painted behind whatever `add` draws.
pub fn midnight_card<R>(ui: &mut Ui, padding: f32, min_height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let background = ui.painter().add(Shape::Noop);
    let width = ui.available_width();
    let top_left = ui.cursor().min;
    let inner_rect = Rect::from_min_size(top_left + Vec2::splat(padding), vec2(width - 2.0 * padding, 0.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner_rect).layout(Layout::top_down(Align::Min)));
    let result = add(&mut child);
    let height = (child.min_rect().height() + 2.0 * padding).max(min_height);
    let rect = Rect::from_min_size(top_left, vec2(width, height));
    // Paint the gradient into the reserved slot so it sits behind the content.
    let gradient = icons::gradient_mesh(rect, 18.0, MIDNIGHT.0, MIDNIGHT.1, ui.ctx().pixels_per_point());
    ui.painter().set(background, gradient);
    ui.advance_cursor_after_rect(rect);
    result
}

/// The dismissible intro card on Dictionary and Snippets. Returns true when dismissed.
pub fn intro_card(ui: &mut Ui, title: LayoutJob, detail: &str, chips: impl FnOnce(&mut Ui)) -> bool {
    let mut dismissed = false;
    midnight_card(ui, 24.0, 0.0, |ui| {
        let id = ui.id().with("intro");
        sides(
            ui,
            id,
            12.0,
            |ui| {
                ui.add(Label::new(title).wrap());
            },
            |ui| {
                let (rect, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                let alpha = if response.hovered() { 0.25 } else { 0.15 };
                ui.painter().circle_filled(rect.center(), 11.0, Color32::WHITE.gamma_multiply(alpha));
                icons::paint(ui.painter(), Rect::from_center_size(rect.center(), Vec2::splat(9.0)), Icon::Close, Color32::WHITE);
                if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Hide").clicked() {
                    dismissed = true;
                }
            },
        );
        ui.add_space(10.0);
        wrapped(ui, text(detail, 13.0, Weight::Regular, Color32::WHITE.gamma_multiply(0.8)));
        ui.add_space(14.0);
        chips(ui);
    });
    dismissed
}

/// "Kudu spells the way *you* do." in the display face, with the emphasized part in italics.
pub fn display_title(parts: &[(&str, bool)], size: f32, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (part, italic) in parts {
        job.append(
            part,
            0.0,
            egui::TextFormat { font_id: display(size), color, italics: *italic, ..Default::default() },
        );
    }
    job
}

/// A centered message in a card, for empty lists.
pub fn empty_state(ui: &mut Ui, icon: Icon, message: &str, hint: Option<&str>) {
    let pal = Palette::of(ui);
    card(ui, 14, Margin::symmetric(24, 40), |ui| {
        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
            icons::paint(ui.painter(), rect, icon, pal.tertiary);
            ui.add_space(6.0);
            ui.label(text(message, 13.5, Weight::Medium, pal.text));
            if let Some(hint) = hint {
                ui.add_space(2.0);
                ui.label(text(hint, 12.5, Weight::Regular, pal.secondary));
            }
        });
    });
}

/// A small icon drawn inline with text.
pub fn inline_icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    icons::paint(ui.painter(), rect, icon, color);
}

/// The first line's baseline, from the top of the galley.
pub fn baseline(galley: &egui::Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y))
        .unwrap_or(galley.size().y * 0.8)
}

/// The baseline of a line of text in `font_id`, from its top.
pub fn baseline_of(ui: &Ui, font_id: FontId) -> f32 {
    baseline(&ui.painter().layout_no_wrap("Ag".into(), font_id, Color32::WHITE))
}
