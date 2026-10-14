//! Insights: speed, cleanups, words, the streak heat map and where you dictate. Everything is
//! computed from the daily totals in usage.json (when it changes, not per frame).

use eframe::egui::{
    self, epaint::PathShape, epaint::PathStroke, pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};
use kudu_core::insights::Cell;
use kudu_core::Insights;

use crate::window::data::{day_long, grouped, month_short};
use crate::window::icons::{self, Icon};
use crate::window::theme::{display, font, Palette, Weight};
use crate::window::widgets::{self, caps};
use crate::window::App;

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_header(ui, "Insights", "Your dictation at a glance. Counted on this computer, never uploaded.");
    ui.add_space(24.0);
    let insights = &app.data.insights;

    // Three cards side by side, the same height.
    let width = ui.available_width();
    let gap = 14.0;
    let card_width = (width - 2.0 * gap) / 3.0;
    let height = 196.0;
    let top = ui.cursor().min;
    let rects: Vec<Rect> = (0..3)
        .map(|i| Rect::from_min_size(top + vec2(i as f32 * (card_width + gap), 0.0), vec2(card_width, height)))
        .collect();
    for rect in &rects {
        paint_card(ui, *rect, 16.0);
    }
    speed_card(ui, rects[0].shrink(20.0), insights);
    cleanup_card(ui, rects[1].shrink(20.0), insights);
    words_card(ui, rects[2].shrink(20.0), insights);
    ui.advance_cursor_after_rect(rects[0].union(rects[2]));

    ui.add_space(18.0);
    streak_card(ui, insights);
    ui.add_space(18.0);
    usage_card(ui, insights);
}

fn paint_card(ui: &Ui, rect: Rect, radius: f32) {
    let pal = Palette::of(ui);
    ui.painter().rect(rect, radius, pal.card, Stroke::new(1.0, pal.hairline), StrokeKind::Inside);
}

fn card_title(ui: &Ui, pos: Pos2, title: &str) -> Rect {
    let pal = Palette::of(ui);
    let galley = egui::WidgetText::from(caps(title, 10.5, pal.secondary)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let rect = Rect::from_min_size(pos, galley.size());
    ui.painter().galley(pos, galley, pal.secondary);
    rect
}

fn speed_card(ui: &mut Ui, inner: Rect, insights: &Insights) {
    let pal = Palette::of(ui);
    let title = card_title(ui, inner.min, "Words per minute");
    let wpm = insights.words_per_minute;
    // A half-ring gauge.
    let gauge = Rect::from_center_size(pos2(inner.center().x, title.bottom() + 12.0 + 41.0), vec2(150.0, 82.0));
    let center = pos2(gauge.center().x, gauge.bottom());
    let radius = (gauge.width() / 2.0).min(gauge.height()) - 6.0;
    let arc = |progress: f32| -> Vec<Pos2> {
        let steps = (64.0 * progress).ceil().max(2.0) as usize;
        (0..=steps)
            .map(|i| {
                let angle = std::f32::consts::PI * (1.0 + progress * i as f32 / steps as f32);
                center + Vec2::angled(angle) * radius
            })
            .collect()
    };
    let track = arc(1.0);
    ui.painter().add(PathShape::line(track.clone(), PathStroke::new(12.0, pal.selection)));
    for end in [track[0], track[track.len() - 1]] {
        ui.painter().circle_filled(end, 6.0, pal.selection);
    }
    let value = (wpm as f32 / 200.0).clamp(0.02, 1.0);
    let filled = arc(value);
    let (from, to) = (pal.accent.gamma_multiply(0.55), pal.accent);
    let (left, right) = (center.x - radius, center.x + radius);
    let gradient = move |_: Rect, p: Pos2| icons::lerp(from, to, ((p.x - left) / (right - left)).clamp(0.0, 1.0));
    ui.painter().add(PathShape::line(filled.clone(), PathStroke::new_uv(12.0, gradient)));
    ui.painter().circle_filled(filled[0], 6.0, from);
    let tip = filled[filled.len() - 1];
    ui.painter().circle_filled(tip, 6.0, gradient(Rect::NOTHING, tip));

    let number = if wpm > 0 { wpm.to_string() } else { "–".into() };
    let number_rect = ui.painter().text(pos2(center.x, center.y - 10.0), Align2::CENTER_BOTTOM, number, display(32.0), pal.text);
    ui.painter().text(pos2(center.x, number_rect.bottom() - 2.0), Align2::CENTER_TOP, "wpm", font(11.5, Weight::Regular), pal.secondary);

    let line = if wpm > 0 {
        format!("{:.1}× faster than typing", wpm as f64 / Insights::TYPING_WORDS_PER_MINUTE)
    } else {
        "Dictate a bit to see your speed".into()
    };
    ui.painter().text(pos2(inner.center().x, inner.bottom()), Align2::CENTER_BOTTOM, line, font(12.5, Weight::Regular), pal.secondary);
}

/// A headline number with a unit after it, baseline-aligned. Returns its bottom.
fn headline(ui: &Ui, pos: Pos2, number: &str, unit: &str) -> f32 {
    let pal = Palette::of(ui);
    let value = ui.painter().layout_no_wrap(number.to_string(), display(32.0), pal.text);
    let label = ui.painter().layout_no_wrap(unit.to_string(), font(12.5, Weight::Regular), pal.secondary);
    let baseline = pos.y + widgets::baseline(&value);
    let bottom = pos.y + value.size().y;
    let label_pos = pos2(pos.x + value.size().x + 6.0, baseline - widgets::baseline(&label));
    ui.painter().galley(pos, value, pal.text);
    ui.painter().galley(label_pos, label, pal.secondary);
    bottom
}

fn icon_line(ui: &Ui, pos: Pos2, icon: Icon, line: &str) {
    let pal = Palette::of(ui);
    icons::paint(ui.painter(), Rect::from_center_size(pos + vec2(7.0, 8.0), Vec2::splat(13.0)), icon, pal.secondary);
    ui.painter().text(pos + vec2(24.0, 8.0), Align2::LEFT_CENTER, line, font(12.5, Weight::Regular), pal.text);
}

fn cleanup_card(ui: &mut Ui, inner: Rect, insights: &Insights) {
    let pal = Palette::of(ui);
    let title = card_title(ui, inner.min, "Cleaned up for you");
    let bottom = headline(ui, pos2(inner.left(), title.bottom() + 10.0), &grouped(insights.cleanups), "words");
    let y = bottom + 12.0;
    ui.painter().hline(inner.left()..=inner.right(), y, Stroke::new(1.0, pal.hairline));
    icon_line(ui, pos2(inner.left(), y + 14.0), Icon::Dictionary, &format!("{} dictionary fixes", grouped(insights.dictionary_fixes)));
    icon_line(ui, pos2(inner.left(), y + 42.0), Icon::Sparkle, &format!("{} words learned from you", grouped(insights.learned)));
}

fn words_card(ui: &mut Ui, inner: Rect, insights: &Insights) {
    let pal = Palette::of(ui);
    let title = card_title(ui, inner.min, "Words dictated");
    let top = title.bottom() + 10.0;
    let bottom = headline(ui, pos2(inner.left(), top), &grouped(insights.total_words), "");
    if let Some(change) = insights.month_change {
        let up = change >= 0.0;
        let label = format!("{}{:.0}%", if up { "↗ +" } else { "↘ " }, change * 100.0);
        let color = if up { pal.green } else { pal.orange };
        let galley = ui.painter().layout_no_wrap(label, font(11.0, Weight::Semibold), color);
        let pill = Rect::from_min_size(
            pos2(inner.right() - galley.size().x - 14.0, top + 10.0),
            galley.size() + vec2(14.0, 6.0),
        );
        ui.painter().rect_filled(pill, pill.height() / 2.0, color.gamma_multiply(0.14));
        ui.painter().galley(pill.center() - galley.size() / 2.0, galley, color);
        ui.interact(pill, Id::new("month-change"), Sense::hover()).on_hover_text("Compared with the same days last month");
    }
    let y = bottom + 12.0;
    ui.painter().hline(inner.left()..=inner.right(), y, Stroke::new(1.0, pal.hairline));
    icon_line(ui, pos2(inner.left(), y + 14.0), Icon::Waveform, &format!("{} dictations", grouped(insights.total_dictations)));
    icon_line(ui, pos2(inner.left(), y + 42.0), Icon::Clock, &format!("{} saved vs typing", duration(insights.minutes_saved)));
}

fn duration(minutes: usize) -> String {
    if minutes >= 60 {
        format!("{}h {}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

// MARK: - Streak heat map

fn level_color(pal: &Palette, level: u8) -> Color32 {
    match level {
        0 => pal.selection,
        1 => pal.accent.gamma_multiply(0.28),
        2 => pal.accent.gamma_multiply(0.5),
        3 => pal.accent.gamma_multiply(0.75),
        _ => pal.accent,
    }
}

fn streak_card(ui: &mut Ui, insights: &Insights) {
    let pal = Palette::of(ui);
    let weeks = insights.heat_map.len().max(1);
    let (max_cell, gap, label_width) = (22.0, 4.0, 28.0);
    let width = ui.available_width();
    let inner_width = width - 44.0;
    let cell = ((inner_width - label_width - 8.0 - gap * (weeks as f32 - 1.0)) / weeks as f32).clamp(8.0, max_cell);
    let grid_height = 14.0 + gap + 7.0 * cell + 6.0 * gap;
    let height = 22.0 + 32.0 + 16.0 + grid_height + 16.0 + 14.0 + 22.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    paint_card(ui, rect, 16.0);
    let inner = rect.shrink(22.0);

    let streak = insights.current_streak;
    let title = if streak == 1 { "1 day streak".to_string() } else { format!("{streak} day streak") };
    let title_rect = ui.painter().text(inner.min, Align2::LEFT_TOP, title, display(26.0), pal.text);
    let longest = format!("Longest streak · {} days", insights.longest_streak);
    let galley = egui::WidgetText::from(caps(&longest, 10.5, pal.secondary)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::FontSelection::Default,
    );
    let longest_pos = pos2(inner.right() - galley.size().x, title_rect.top() + widgets::baseline_of(ui, display(26.0)) - widgets::baseline(&galley));
    ui.painter().galley(longest_pos, galley, pal.secondary);

    let grid_top = inner.top() + 32.0 + 16.0;
    let cells_left = inner.left() + label_width + 8.0;
    let cells_top = grid_top + 14.0 + gap;
    for (row, label) in [(1, "Mon"), (3, "Wed"), (5, "Fri")] {
        let y = cells_top + row as f32 * (cell + gap) + cell / 2.0;
        ui.painter().text(pos2(inner.left() + label_width, y), Align2::RIGHT_CENTER, label, font(9.5, Weight::Regular), pal.secondary);
    }
    let cell_rect = |week: usize, day: usize| {
        Rect::from_min_size(pos2(cells_left + week as f32 * (cell + gap), cells_top + day as f32 * (cell + gap)), Vec2::splat(cell))
    };
    let mut last_month = 0;
    for (week_index, week) in insights.heat_map.iter().enumerate() {
        if let Some(first) = week.first() {
            let (_, month, date) = first.date.ymd();
            // Label the first week of each month (and the first column).
            if (date <= 7 || week_index == 0) && month != last_month {
                last_month = month;
                ui.painter().text(
                    pos2(cells_left + week_index as f32 * (cell + gap), grid_top),
                    Align2::LEFT_TOP,
                    month_short(month),
                    font(9.5, Weight::Regular),
                    pal.secondary,
                );
            }
        }
        for (day_index, day) in week.iter().enumerate() {
            let color = if day.is_future { pal.selection.gamma_multiply(0.35) } else { level_color(&pal, day.level) };
            ui.painter().rect_filled(cell_rect(week_index, day_index), cell * 0.22, color);
        }
    }
    // One hover target for the grid; the tooltip names the day under the pointer.
    let grid = Rect::from_min_max(cell_rect(0, 0).min, cell_rect(weeks - 1, 6).max);
    let response = ui.interact(grid, Id::new("heat-map"), Sense::hover());
    if let Some(pointer) = response.hover_pos() {
        let week = ((pointer.x - cells_left) / (cell + gap)).floor() as usize;
        let day = ((pointer.y - cells_top) / (cell + gap)).floor() as usize;
        let found: Option<&Cell> = insights.heat_map.get(week).and_then(|w| w.get(day));
        if let Some(found) = found.filter(|c| cell_rect(week, day).expand(gap / 2.0).contains(pointer) && !c.is_future) {
            let tip = if found.words == 0 {
                format!("No dictation · {}", day_long(found.date))
            } else {
                format!("{} words · {}", grouped(found.words), day_long(found.date))
            };
            response.on_hover_text_at_pointer(tip);
        }
    }

    // Legend.
    let legend_y = grid_top + grid_height + 16.0 + 7.0;
    let mut x = inner.left();
    let less = ui.painter().text(pos2(x, legend_y), Align2::LEFT_CENTER, "Less", font(11.5, Weight::Regular), pal.secondary);
    x = less.right() + 6.0;
    for level in 0..5 {
        ui.painter().rect_filled(Rect::from_min_size(pos2(x, legend_y - 5.5), Vec2::splat(11.0)), 3.0, level_color(&pal, level));
        x += 16.0;
    }
    ui.painter().text(pos2(x + 1.0, legend_y), Align2::LEFT_CENTER, "More", font(11.5, Weight::Regular), pal.secondary);
    if inner.width() > 560.0 {
        ui.painter().text(
            pos2(inner.right(), legend_y),
            Align2::RIGHT_CENTER,
            "Each square is a day; darker means more words.",
            font(11.5, Weight::Regular),
            pal.secondary,
        );
    }
}

// MARK: - Where you dictate

fn category(name: &str) -> (&'static str, Icon) {
    match name {
        "personalMessage" => ("Personal messages", Icon::Message),
        "workMessage" => ("Work messages", Icon::Bubbles),
        "email" => ("Email", Icon::Envelope),
        "code" => ("Code", Icon::Code),
        _ => ("Everything else", Icon::Document),
    }
}

/// A readable name for an app identifier ("slack.exe", "org.mozilla.firefox", "Code").
fn app_name(id: &str) -> String {
    let base = id.rsplit(['\\', '/']).next().unwrap_or(id);
    let base = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    let base = base.rsplit('.').next().filter(|_| base.contains('.') && !base.contains(' ')).unwrap_or(base);
    let known = match base.to_ascii_lowercase().as_str() {
        "slack" => Some("Slack"),
        "ms-teams" | "teams" | "msteams" => Some("Microsoft Teams"),
        "discord" => Some("Discord"),
        "whatsapp" => Some("WhatsApp"),
        "telegram" | "telegram-desktop" | "telegramdesktop" => Some("Telegram"),
        "signal" | "signal-desktop" => Some("Signal"),
        "code" | "vscode" => Some("Visual Studio Code"),
        "chrome" | "google-chrome" => Some("Google Chrome"),
        "firefox" => Some("Firefox"),
        "msedge" | "microsoft-edge" => Some("Microsoft Edge"),
        "olk" | "outlook" => Some("Outlook"),
        "thunderbird" => Some("Thunderbird"),
        "winword" => Some("Word"),
        "notepad" => Some("Notepad"),
        "obsidian" => Some("Obsidian"),
        "explorer" => Some("File Explorer"),
        "windowsterminal" => Some("Terminal"),
        _ => None,
    };
    if let Some(known) = known {
        return known.to_string();
    }
    let mut chars = base.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_else(|| id.to_string())
}

fn usage_card(ui: &mut Ui, insights: &Insights) {
    let pal = Palette::of(ui);
    let width = ui.available_width();
    let apps_width = 250.0;
    let rows = insights.categories.len().max(1).max(insights.apps.len().min(5));
    let height = 22.0 + 14.0 + 14.0 + rows as f32 * 30.0 + 22.0 - 8.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    paint_card(ui, rect, 16.0);
    let inner = rect.shrink(22.0);
    let left = Rect::from_min_max(inner.min, pos2(inner.right() - apps_width - 28.0, inner.bottom()));
    let right = Rect::from_min_max(pos2(inner.right() - apps_width, inner.top()), inner.max);

    let title = card_title(ui, left.min, "Where you dictate");
    let mut y = title.bottom() + 14.0;
    if insights.categories.is_empty() {
        ui.painter().text(pos2(left.left(), y), Align2::LEFT_TOP, "Nothing yet.", font(13.0, Weight::Regular), pal.secondary);
    }
    let total = insights.total_words.max(1) as f32;
    for (index, (name, words)) in insights.categories.iter().enumerate() {
        let (title, icon) = category(name);
        let share = *words as f32 / total;
        let center = y + 11.0;
        icons::paint(ui.painter(), Rect::from_center_size(pos2(left.left() + 9.0, center), Vec2::splat(14.0)), icon, pal.secondary);
        ui.painter().text(pos2(left.left() + 28.0, center), Align2::LEFT_CENTER, title, font(12.5, Weight::Regular), pal.text);
        let bar = Rect::from_min_max(pos2(left.left() + 28.0 + 130.0, center - 4.0), pos2(left.right() - 48.0, center + 4.0));
        ui.painter().rect_filled(bar, 4.0, pal.selection);
        let filled = Rect::from_min_size(bar.min, vec2((bar.width() * share).max(6.0), bar.height()));
        ui.painter().rect_filled(filled, 4.0, pal.accent);
        ui.painter().text(
            pos2(left.right(), center),
            Align2::RIGHT_CENTER,
            format!("{:.0}%", share * 100.0),
            font(12.0, Weight::Medium),
            pal.text,
        );
        let hover = Rect::from_min_max(pos2(left.left(), y), pos2(left.right(), y + 22.0));
        ui.interact(hover, Id::new(("category", index)), Sense::hover()).on_hover_text(format!("{} words", grouped(*words)));
        y += 30.0;
    }

    let title = card_title(ui, right.min, "Top apps");
    let mut y = title.bottom() + 14.0;
    if insights.apps.is_empty() {
        ui.painter().text(pos2(right.left(), y), Align2::LEFT_TOP, "Nothing yet.", font(13.0, Weight::Regular), pal.secondary);
    }
    for (id, words) in insights.apps.iter().take(5) {
        let name = app_name(id);
        let center = y + 11.0;
        let avatar = Rect::from_center_size(pos2(right.left() + 11.0, center), Vec2::splat(22.0));
        ui.painter().rect_filled(avatar, 6.0, pal.selection);
        let initial = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
        ui.painter().text(avatar.center(), Align2::CENTER_CENTER, initial, font(11.5, Weight::Semibold), pal.secondary);
        let words_rect =
            ui.painter().text(pos2(right.right(), center), Align2::RIGHT_CENTER, grouped(*words), font(12.0, Weight::Regular), pal.secondary);
        let galley = ui.painter().layout(
            name,
            font(12.5, Weight::Regular),
            pal.text,
            (words_rect.left() - avatar.right() - 20.0).max(20.0),
        );
        let first_row = galley.rows.first().map_or(galley.size().y, |r| r.size.y);
        ui.painter().with_clip_rect(Rect::from_min_max(pos2(avatar.right() + 10.0, y), pos2(words_rect.left() - 8.0, y + 22.0))).galley(
            pos2(avatar.right() + 10.0, center - first_row / 2.0),
            galley,
            pal.text,
        );
        y += 30.0;
    }
}
