//! Home: welcome, the setup card while anything is missing, what Kudu does, your numbers, and
//! the dictation history grouped by day.

use std::time::SystemTime;

use eframe::egui::{
    self, epaint::RectShape, pos2, vec2, Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Popup, Rect, RichText,
    Sense, Shape, Ui, Vec2,
};
use kudu_core::Day;
use kudu_engine::SpeechModel;

use crate::ipc::{ModelStatus, Request};
use crate::settings::RecordingMode;
use crate::window::data::{compact, day_title, time_of_day};
use crate::window::icons::{self, Icon};
use crate::window::theme::{display, font, Palette, Weight};
use crate::window::widgets::{self, button, card, caps, text, wrapped, ButtonKind};
use crate::window::App;

/// How many history rows to show before "Show more".
const PAGE: usize = 150;

#[derive(Default)]
pub struct HomeState {
    query: String,
    limit: usize,
    groups: Option<Grouped>,
}

/// The history grouped by day for a search, cached until any of its inputs change.
#[derive(Clone)]
struct Grouped {
    query: String,
    version: u64,
    limit: usize,
    today: Day,
    /// Days, newest first, with indices into the history.
    days: Vec<(Day, Vec<usize>)>,
    /// How many records match the search (some may be past `limit`).
    matched: usize,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    ui.add_space(8.0);
    ui.label(RichText::new(format!("Welcome back, {}", app.first_name)).font(display(32.0)).color(pal.text));
    ui.add_space(26.0);

    if app.status.is_some() && !app.is_ready() {
        setup_card(app, ui);
        ui.add_space(26.0);
    }

    hero_and_stats(app, ui);
    ui.add_space(26.0);
    history(app, ui);
}

// MARK: - Setup

fn setup_card(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    let Some(status) = app.status.clone() else { return };
    card(ui, 16, Margin::same(22), |ui| {
        ui.label(text("Finish setting up", 17.0, Weight::Semibold, pal.text));
        ui.add_space(4.0);
        wrapped(
            ui,
            text(
                format!(
                    "A few quick steps, then hold {} anywhere to dictate. Everything stays on this computer.",
                    app.dictate_name()
                ),
                13.0,
                Weight::Regular,
                pal.secondary,
            ),
        );
        ui.add_space(18.0);

        let mut number = 1;
        // 1. The speech model.
        let model_done = matches!(status.speech, ModelStatus::Ready | ModelStatus::Loading);
        let detail = match &status.speech {
            ModelStatus::Loading => "Downloaded. Loading it now…".to_string(),
            ModelStatus::Ready => "Downloaded and ready.".to_string(),
            _ => format!(
                "{} About {} MB, downloaded once. Works offline after that.",
                app.setup_choice.summary(),
                app.setup_choice.download_mb()
            ),
        };
        step(ui, number, "Speech model", &detail, model_done, |ui| match &status.speech {
            ModelStatus::Downloading { fraction } => {
                if button(ui, "Cancel", ButtonKind::Quiet).clicked() {
                    app.send(Request::CancelDownload);
                }
                ui.label(text(format!("{}%", (fraction * 100.0).round() as i32), 12.0, Weight::Medium, pal.secondary));
                widgets::progress_bar(ui, *fraction as f32, 140.0);
            }
            ModelStatus::Missing | ModelStatus::Failed { .. } => {
                let label = if matches!(status.speech, ModelStatus::Failed { .. }) { "Try again" } else { "Download" };
                if button(ui, label, ButtonKind::Prominent).clicked() {
                    // The background process saves the choice and downloads it.
                    app.pending_speech = Some(app.setup_choice);
                    app.send(Request::DownloadSpeechModel { model: app.setup_choice });
                }
                let titles: Vec<String> =
                    SpeechModel::ALL.iter().map(|m| format!("{} · {} MB", m.title(), m.download_mb())).collect();
                let titles: Vec<&str> = titles.iter().map(String::as_str).collect();
                let selected = SpeechModel::ALL.iter().position(|m| *m == app.setup_choice).unwrap_or(0);
                if let Some(index) = widgets::chip_picker(ui, Id::new("setup-model"), &titles, selected, None) {
                    app.setup_choice = SpeechModel::ALL[index];
                }
            }
            ModelStatus::Loading | ModelStatus::Ready => {}
        });
        if let ModelStatus::Failed { message } = &status.speech {
            indented(ui, |ui| {
                wrapped(ui, text(message, 11.5, Weight::Regular, pal.red));
            });
        }
        number += 1;

        // 2. Whatever the background process says must be fixed.
        for (index, issue) in status.issues.iter().enumerate() {
            ui.add_space(14.0);
            step(ui, number, &issue.title, &issue.detail, false, |_| {});
            if let Some(command) = &issue.command {
                ui.add_space(8.0);
                indented(ui, |ui| command_box(ui, index, command));
            }
            number += 1;
        }

        // 3. The shortcut listener.
        ui.add_space(14.0);
        let detail = if status.hotkeys_active {
            format!("Listening for {}.", app.dictate_name())
        } else if status.issues.is_empty() {
            "Kudu can't see the keyboard yet. Restart Kudu if this doesn't clear up.".to_string()
        } else {
            "Starts working once the steps above are done.".to_string()
        };
        step(ui, number, "Keyboard shortcut", &detail, status.hotkeys_active, |_| {});
    });
}

fn indented(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.add_space(40.0);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    });
}

/// A command to run in a terminal, with a copy button.
fn command_box(ui: &mut Ui, index: usize, command: &str) {
    let pal = Palette::of(ui);
    Frame::new().fill(pal.selection).corner_radius(8).inner_margin(Margin { left: 12, right: 6, top: 6, bottom: 6 }).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            widgets::sides(
                ui,
                Id::new(("issue-command", index)),
                10.0,
                |ui| {
                    wrapped(ui, RichText::new(command).monospace().color(pal.text));
                },
                |ui| {
                    if widgets::icon_button(ui, Icon::Copy, "Copy command").clicked() {
                        ui.ctx().copy_text(command.to_string());
                    }
                },
            );
        },
    );
}

fn step(ui: &mut Ui, number: usize, title: &str, detail: &str, done: bool, action: impl FnOnce(&mut Ui)) {
    let pal = Palette::of(ui);
    widgets::sides(
        ui,
        Id::new(("setup-step", title)),
        14.0,
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let (circle, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                let center = circle.center();
                if done {
                    ui.painter().circle_filled(center, 13.0, pal.green);
                    icons::paint(ui.painter(), Rect::from_center_size(center, Vec2::splat(12.0)), Icon::Check, Color32::WHITE);
                } else {
                    ui.painter().circle_filled(center, 13.0, pal.selection);
                    ui.painter().text(center, egui::Align2::CENTER_CENTER, number, font(12.0, Weight::Semibold), pal.text);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(text(title, 13.5, Weight::Semibold, pal.text));
                    wrapped(ui, text(detail, 12.5, Weight::Regular, pal.secondary));
                });
            });
        },
        |ui| {
            if !done {
                action(ui);
            }
        },
    );
}

// MARK: - Hero and stats

fn hero_and_stats(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    let width = ui.available_width();
    let stats_width = 236.0;
    let height = 200.0;
    let top_left = ui.cursor().min;
    let hero = Rect::from_min_size(top_left, vec2(width - stats_width - 16.0, height));
    let stats = Rect::from_min_size(pos2(hero.right() + 16.0, top_left.y), vec2(stats_width, height));

    // The midnight card: what Kudu does and the gestures.
    icons::gradient_rect(ui.painter(), hero, 18.0, crate::window::theme::MIDNIGHT.0, crate::window::theme::MIDNIGHT.1);
    let mark = Rect::from_center_size(pos2(hero.right() - 24.0 - 42.0 - 6.0, hero.center().y), Vec2::splat(84.0));
    let show_mark = hero.width() > 420.0;
    if show_mark {
        icons::kudu_mark(ui.painter(), mark, Color32::WHITE.gamma_multiply(0.92));
    }
    let content_right = if show_mark { mark.left() - 20.0 } else { hero.right() - 24.0 };
    let content = Rect::from_min_max(hero.min + vec2(24.0, 24.0), pos2(content_right, hero.bottom() - 24.0));
    let shortcuts = app.data.shortcuts();
    let dictate = shortcuts.dictate.display(app.notation);
    let mut hints = vec![(format!("Hold {dictate}"), "dictate")];
    match app.data.settings.recording_mode {
        RecordingMode::HoldOrDoubleTap => hints.push((format!("Double-tap {dictate}"), "hands-free")),
        RecordingMode::TapToToggle => hints.push((format!("Tap {dictate}"), "hands-free")),
        RecordingMode::HoldOnly => {}
    }
    if let Some(command) = &shortcuts.command {
        hints.push((command.display(app.notation), "command: rewrite, translate, draft"));
    }
    let mut hero_ui = ui.new_child(egui::UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)));
    let title = RichText::new("Speak anywhere you write").font(display(25.0)).color(Color32::WHITE);
    let title_galley = egui::WidgetText::from(title).into_galley(&hero_ui, Some(egui::TextWrapMode::Wrap), content.width(), egui::FontSelection::Default);
    let hints_height = hints.len() as f32 * 22.0 + (hints.len().saturating_sub(1)) as f32 * 7.0;
    let block = title_galley.size().y + 14.0 + hints_height;
    hero_ui.add_space(((content.height() - block) / 2.0).max(0.0));
    hero_ui.label(title_galley);
    hero_ui.add_space(14.0);
    for (keys, what) in hints {
        hero_ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let galley = ui.painter().layout_no_wrap(keys, font(11.5, Weight::Semibold), Color32::WHITE);
            let (pill, _) = ui.allocate_exact_size(galley.size() + vec2(16.0, 6.0), Sense::hover());
            ui.painter().rect_filled(pill, 6.0, Color32::WHITE.gamma_multiply(0.14));
            ui.painter().galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE.gamma_multiply(0.9));
            ui.label(text(what, 12.5, Weight::Regular, Color32::WHITE.gamma_multiply(0.88)));
        });
        hero_ui.add_space(7.0);
    }

    // Your numbers.
    ui.painter().rect(stats, 18.0, pal.card, egui::Stroke::new(1.0, pal.hairline), egui::StrokeKind::Inside);
    let numbers = app.data.stats;
    let rows = [
        (compact(numbers.total_words), "total words"),
        (if numbers.words_per_minute > 0 { numbers.words_per_minute.to_string() } else { "–".into() }, "wpm"),
        (numbers.day_streak.to_string(), "day streak"),
    ];
    let inner = stats.shrink(22.0);
    let slot = inner.height() / 3.0;
    for (index, (value, label)) in rows.iter().enumerate() {
        let center_y = inner.top() + slot * (index as f32 + 0.5);
        let value_galley = ui.painter().layout_no_wrap(value.clone(), display(30.0), pal.text);
        let label_galley = ui.painter().layout_no_wrap(label.to_string(), font(13.0, Weight::Regular), pal.secondary);
        let value_pos = pos2(inner.left(), center_y - value_galley.size().y / 2.0);
        // Align the label's baseline with the number's.
        let baseline = value_pos.y + widgets::baseline(&value_galley);
        let label_pos = pos2(inner.left() + value_galley.size().x + 8.0, baseline - widgets::baseline(&label_galley));
        ui.painter().galley(value_pos, value_galley, pal.text);
        ui.painter().galley(label_pos, label_galley, pal.secondary);
        if index > 0 {
            let y = inner.top() + slot * index as f32;
            ui.painter().hline(inner.left()..=inner.right(), y, egui::Stroke::new(1.0, pal.hairline));
        }
    }
    ui.advance_cursor_after_rect(hero.union(stats));
}

// MARK: - History

fn history(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    widgets::sides(
        ui,
        Id::new("history-header"),
        12.0,
        |ui| {
            ui.label(text("History", 17.0, Weight::Semibold, pal.text));
        },
        |ui| widgets::search_field(ui, &mut app.home.query, 230.0),
    );
    ui.add_space(12.0);

    if app.home.limit == 0 {
        app.home.limit = PAGE;
    }
    let today = app.data.calendar.day(SystemTime::now());
    let key_matches = app.home.groups.as_ref().is_some_and(|g| {
        g.query == app.home.query && g.version == app.data.history_version && g.limit == app.home.limit && g.today == today
    });
    if !key_matches {
        let needle = app.home.query.trim().to_lowercase();
        let mut groups: Vec<(Day, Vec<usize>)> = Vec::new();
        let mut matched = 0;
        for (index, record) in app.data.history.iter().enumerate() {
            if !needle.is_empty() && !record.text.to_lowercase().contains(&needle) && !record.raw.to_lowercase().contains(&needle) {
                continue;
            }
            matched += 1;
            if matched > app.home.limit {
                continue;
            }
            let day = app.data.calendar.day(record.date);
            match groups.last_mut() {
                Some((last, records)) if *last == day => records.push(index),
                _ => groups.push((day, vec![index])),
            }
        }
        app.home.groups = Some(Grouped {
            query: app.home.query.clone(),
            version: app.data.history_version,
            limit: app.home.limit,
            today,
            days: groups,
            matched,
        });
    }
    let Some(Grouped { days: groups, matched, .. }) = app.home.groups.clone() else { return };

    if groups.is_empty() {
        let searching = !app.home.query.trim().is_empty();
        let shortcuts = app.data.shortcuts();
        let hint = app.data.settings.recording_mode.hint(&shortcuts.dictate.display(app.notation));
        widgets::empty_state(
            ui,
            if searching { Icon::Search } else { Icon::Waveform },
            if searching { "No dictations match your search." } else { "Your dictations will appear here." },
            (!searching).then_some(hint.as_str()),
        );
        return;
    }

    let mut delete: Option<String> = None;
    for (day, records) in &groups {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.label(caps(&day_title(*day, today), 11.0, pal.secondary));
        });
        ui.add_space(8.0);
        card(ui, 14, Margin::ZERO, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (position, &index) in records.iter().enumerate() {
                if position > 0 {
                    widgets::divider(ui, 112.0);
                }
                let first = position == 0;
                let last = position + 1 == records.len();
                if let Some(id) = history_row(app, ui, index, first, last) {
                    delete = Some(id);
                }
            }
        });
    }
    if matched > app.home.limit {
        ui.add_space(14.0);
        ui.vertical_centered(|ui| {
            if button(ui, "Show more", ButtonKind::Quiet).clicked() {
                app.home.limit += PAGE;
            }
        });
    }
    if let Some(id) = delete {
        app.data.rewrite_history(|record| record.id != id);
        app.send(Request::Reload);
    }
}

/// One dictation. Returns the record's id when the user deleted it.
fn history_row(app: &App, ui: &mut Ui, index: usize, first: bool, last: bool) -> Option<String> {
    let pal = Palette::of(ui);
    let record = &app.data.history[index];
    let row_id = Id::new(("history-row", &record.id));
    let menu_id = row_id.with("menu");
    let was_hovered = ui.data(|d| d.get_temp::<bool>(row_id)).unwrap_or(false) || Popup::is_id_open(ui.ctx(), menu_id);
    let background = ui.painter().add(Shape::Noop);
    let mut deleted = None;
    let (_, seconds) = app.data.calendar.local(record.date);

    let response = Frame::new().inner_margin(Margin::symmetric(20, 14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (time_rect, _) = ui.allocate_exact_size(vec2(92.0, 18.0), Sense::hover());
            ui.painter().text(
                time_rect.left_top() + vec2(0.0, 1.5),
                egui::Align2::LEFT_TOP,
                time_of_day(seconds),
                font(12.5, Weight::Regular),
                pal.secondary,
            );
            let text_width = (ui.available_width() - 12.0 - 54.0).max(80.0);
            ui.allocate_ui_with_layout(vec2(text_width, 0.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(text_width);
                ui.spacing_mut().item_spacing.y = 4.0;
                if record.is_command() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        widgets::inline_icon(ui, Icon::Sparkle, 11.0, pal.accent);
                        ui.add(egui::Label::new(text(&record.raw, 11.5, Weight::Regular, pal.accent)).truncate());
                    });
                }
                let body = text(&record.text, 13.5, Weight::Regular, pal.text).line_height(Some(19.0));
                ui.add(egui::Label::new(body).wrap());
            });
            ui.add_space(12.0);
            ui.allocate_ui_with_layout(vec2(54.0, 26.0), Layout::left_to_right(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if was_hovered {
                    if widgets::icon_button(ui, Icon::Copy, "Copy").clicked() {
                        ui.ctx().copy_text(record.text.clone());
                    }
                    let more = widgets::icon_button(ui, Icon::Ellipsis, "More");
                    Popup::menu(&more).id(menu_id).gap(4.0).show(|ui| {
                        ui.set_min_width(190.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if widgets::menu_item(ui, "Copy text", false, false) {
                            ui.ctx().copy_text(record.text.clone());
                            ui.close();
                        }
                        if widgets::menu_item(ui, "Copy raw transcript", false, false) {
                            ui.ctx().copy_text(record.raw.clone());
                            ui.close();
                        }
                        ui.add_space(4.0);
                        widgets::divider(ui, 0.0);
                        ui.add_space(4.0);
                        if widgets::menu_item(ui, "Delete", false, true) {
                            deleted = Some(record.id.clone());
                            ui.close();
                        }
                    });
                } else {
                    ui.allocate_exact_size(vec2(54.0, 26.0), Sense::hover());
                }
            });
        });
    });
    let rect = response.response.rect;
    let hovered = ui.rect_contains_pointer(rect);
    if hovered != ui.data(|d| d.get_temp::<bool>(row_id)).unwrap_or(false) {
        ui.data_mut(|d| d.insert_temp(row_id, hovered));
        ui.ctx().request_repaint();
    }
    if hovered || was_hovered {
        let radius = CornerRadius {
            nw: if first { 14 } else { 0 },
            ne: if first { 14 } else { 0 },
            sw: if last { 14 } else { 0 },
            se: if last { 14 } else { 0 },
        };
        ui.painter().set(background, RectShape::filled(rect.shrink(1.0), radius, pal.hover));
    }
    deleted
}
