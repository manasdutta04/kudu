//! Dictionary and Snippets: an intro card, search, add, edit and delete.

use eframe::egui::{self, epaint::RectShape, vec2, Align, CornerRadius, Frame, Id, Key, Layout, Margin, Shape, Ui};
use kudu_core::{DictionaryEntry, Snippet};

use crate::ipc::Request;
use crate::window::icons::Icon;
use crate::window::theme::{Palette, Weight};
use crate::window::widgets::{self, button_enabled, text, ButtonKind};
use crate::window::App;

#[derive(Default)]
pub struct ListState {
    adding: bool,
    query: String,
    first: String,
    second: String,
    /// The entry being edited, with its draft.
    editing: Option<(String, String, String)>,
    focus_add: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dictionary,
    Snippets,
}

struct Item {
    id: String,
    first: String,
    second: String,
    learned: bool,
}

enum Action {
    Add(String, String),
    Save(String, String, String),
    Delete(String),
}

pub fn dictionary(app: &mut App, ui: &mut Ui) {
    let items: Vec<Item> = app
        .data
        .dictionary
        .iter()
        .rev()
        .map(|e| Item { id: e.id.clone(), first: e.written.clone(), second: e.heard.join(", "), learned: e.is_learned() })
        .collect();
    let examples: Vec<String> = app.data.dictionary.iter().take(4).map(|e| e.written.clone()).collect();
    let dismissed = app.data.prefs.dismissed_dictionary_intro;
    let mut state = std::mem::take(&mut app.dictionary);
    let (action, dismiss) = page(ui, Kind::Dictionary, &mut state, &items, &examples, dismissed);
    app.dictionary = state;
    if dismiss {
        app.data.prefs.dismissed_dictionary_intro = true;
        app.data.save_prefs();
    }
    let Some(action) = action else { return };
    let heard = |text: &str| -> Vec<String> {
        text.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
    };
    app.data.update_dictionary(|entries| match action {
        Action::Add(written, variants) => entries.push(DictionaryEntry::new(heard(&variants), written.trim(), false)),
        Action::Save(id, written, variants) => {
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.written = written.trim().to_string();
                entry.heard = heard(&variants);
            }
        }
        Action::Delete(id) => entries.retain(|e| e.id != id),
    });
    app.send(Request::Reload);
}

pub fn snippets(app: &mut App, ui: &mut Ui) {
    let items: Vec<Item> = app
        .data
        .snippets
        .iter()
        .rev()
        .map(|s| Item { id: s.id.clone(), first: s.trigger.clone(), second: s.expansion.clone(), learned: false })
        .collect();
    let dismissed = app.data.prefs.dismissed_snippets_intro;
    let mut state = std::mem::take(&mut app.snippets);
    let (action, dismiss) = page(ui, Kind::Snippets, &mut state, &items, &[], dismissed);
    app.snippets = state;
    if dismiss {
        app.data.prefs.dismissed_snippets_intro = true;
        app.data.save_prefs();
    }
    let Some(action) = action else { return };
    app.data.update_snippets(|snippets| match action {
        Action::Add(trigger, expansion) => snippets.push(Snippet::new(trigger.trim(), expansion)),
        Action::Save(id, trigger, expansion) => {
            if let Some(snippet) = snippets.iter_mut().find(|s| s.id == id) {
                snippet.trigger = trigger.trim().to_string();
                snippet.expansion = expansion;
            }
        }
        Action::Delete(id) => snippets.retain(|s| s.id != id),
    });
    app.send(Request::Reload);
}

fn page(
    ui: &mut Ui,
    kind: Kind,
    state: &mut ListState,
    items: &[Item],
    examples: &[String],
    intro_dismissed: bool,
) -> (Option<Action>, bool) {
    let pal = Palette::of(ui);
    let mut action = None;
    let mut dismiss = false;
    let (title, subtitle) = match kind {
        Kind::Dictionary => (
            "Dictionary",
            "Names, products and jargon, spelled your way. Kudu also learns them when you fix a word it got wrong.",
        ),
        Kind::Snippets => {
            ("Snippets", "Say a trigger on its own to insert saved text. Placeholders: {date}, {time}, {clipboard}.")
        }
    };
    widgets::sides(
        ui,
        Id::new(("list-header", title)),
        16.0,
        |ui| widgets::page_header(ui, title, subtitle),
        |ui| {
            if widgets::button_with_icon(ui, Some(Icon::Plus), "Add new", ButtonKind::Prominent, true).clicked() {
                state.adding = !state.adding;
                state.focus_add = state.adding;
            }
        },
    );
    ui.add_space(24.0);

    if !intro_dismissed {
        let (title, detail) = match kind {
            Kind::Dictionary => (
                widgets::display_title(&[("Kudu spells the way ", false), ("you", true), (" do.", false)], 28.0, egui::Color32::WHITE),
                "Correct a spelling once after dictating, or add it here, and your names and terms come out right from then on.",
            ),
            Kind::Snippets => (
                widgets::display_title(&[("Say it once, insert it ", false), ("anywhere", true), (".", false)], 28.0, egui::Color32::WHITE),
                "Save text you type often: your email, a link, a sign-off. Then say its trigger to drop it in.",
            ),
        };
        dismiss = widgets::intro_card(ui, title, detail, |ui| match kind {
            Kind::Dictionary => {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    if widgets::chip(ui, "Add new word", false, true).clicked() {
                        state.adding = true;
                        state.focus_add = true;
                    }
                    let samples: Vec<&str> = if examples.is_empty() {
                        vec!["Kudu", "GitHub", "Priya"]
                    } else {
                        examples.iter().map(String::as_str).collect()
                    };
                    for sample in samples {
                        widgets::chip(ui, sample, false, false);
                    }
                });
            }
            Kind::Snippets => {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (trigger, expansion) in [("my email", "you@example.com"), ("my address", "12 Park Lane, Springfield")] {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        widgets::chip(ui, &format!("“{trigger}”"), true, false);
                        widgets::inline_icon(ui, Icon::ArrowRight, 11.0, egui::Color32::WHITE.gamma_multiply(0.7));
                        widgets::chip(ui, expansion, false, false);
                    });
                }
                ui.add_space(4.0);
                if widgets::chip(ui, "Add new snippet", false, true).clicked() {
                    state.adding = true;
                    state.focus_add = true;
                }
            }
        });
        ui.add_space(24.0);
    }

    if state.adding {
        let (first_hint, second_hint) = match kind {
            Kind::Dictionary => ("Spelling, e.g. Kudu", "Heard as, e.g. kudoo, coo doo (optional)"),
            Kind::Snippets => ("Trigger, e.g. my email", "Inserts"),
        };
        let ready = !state.first.trim().is_empty() && (kind == Kind::Dictionary || !state.second.trim().is_empty());
        let mut submit = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let field_width = ((ui.available_width() - 150.0) / 2.0).max(120.0);
            let first = widgets::field(ui, &mut state.first, first_hint, field_width);
            if state.focus_add {
                first.request_focus();
                state.focus_add = false;
            }
            let second = widgets::field(ui, &mut state.second, second_hint, field_width);
            let entered = (first.lost_focus() || second.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
            if button_enabled(ui, "Add", ButtonKind::Prominent, ready).clicked() || (entered && ready) {
                submit = true;
            }
            if widgets::button(ui, "Cancel", ButtonKind::Quiet).clicked() {
                state.adding = false;
            }
        });
        if submit {
            action = Some(Action::Add(std::mem::take(&mut state.first), std::mem::take(&mut state.second)));
            state.focus_add = true;
        }
        ui.add_space(24.0);
    }

    let count = match (kind, items.len()) {
        (Kind::Dictionary, 1) => "1 word".to_string(),
        (Kind::Dictionary, n) => format!("{n} words"),
        (Kind::Snippets, 1) => "1 snippet".to_string(),
        (Kind::Snippets, n) => format!("{n} snippets"),
    };
    widgets::sides(
        ui,
        Id::new(("list-count", title)),
        12.0,
        |ui| {
            ui.label(text(count, 13.0, Weight::Semibold, pal.text));
        },
        |ui| widgets::search_field(ui, &mut state.query, 220.0),
    );
    ui.add_space(12.0);

    if items.is_empty() {
        match kind {
            Kind::Dictionary => widgets::empty_state(
                ui,
                Icon::Dictionary,
                "No words yet.",
                Some("Add names and terms, or fix a word after dictating."),
            ),
            Kind::Snippets => widgets::empty_state(ui, Icon::Snippets, "No snippets yet.", Some("Try “my email” → your address.")),
        }
        return (action, dismiss);
    }
    let needle = state.query.trim().to_lowercase();
    let shown: Vec<&Item> = items
        .iter()
        .filter(|i| needle.is_empty() || i.first.to_lowercase().contains(&needle) || i.second.to_lowercase().contains(&needle))
        .collect();
    if shown.is_empty() {
        widgets::empty_state(ui, Icon::Search, "Nothing matches your search.", None);
        return (action, dismiss);
    }
    widgets::card(ui, 14, Margin::ZERO, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (index, item) in shown.iter().enumerate() {
            if index > 0 {
                widgets::divider(ui, 18.0);
            }
            if let Some(a) = entry_row(ui, kind, state, item, index == 0, index + 1 == shown.len()) {
                action = Some(a);
            }
        }
    });
    (action, dismiss)
}

fn entry_row(ui: &mut Ui, kind: Kind, state: &mut ListState, item: &Item, first: bool, last: bool) -> Option<Action> {
    let pal = Palette::of(ui);
    let row_id = Id::new(("entry", &item.id));
    let was_hovered = ui.data(|d| d.get_temp::<bool>(row_id)).unwrap_or(false);
    let background = ui.painter().add(Shape::Noop);
    let mut action = None;

    let editing = state.editing.as_ref().is_some_and(|(id, _, _)| *id == item.id);
    let response = Frame::new().inner_margin(Margin::symmetric(18, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if editing {
            let Some((id, first_text, second_text)) = state.editing.as_mut() else { return };
            let mut done = None;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let field_width = ((ui.available_width() - 150.0) / 2.0).max(100.0);
                let (first_hint, second_hint) = match kind {
                    Kind::Dictionary => ("Spelling", "Heard as (comma separated)"),
                    Kind::Snippets => ("Trigger", "Inserts"),
                };
                let a = widgets::field(ui, first_text, first_hint, field_width);
                let b = widgets::field(ui, second_text, second_hint, field_width);
                let ready = !first_text.trim().is_empty() && (kind == Kind::Dictionary || !second_text.trim().is_empty());
                let entered = (a.lost_focus() || b.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter));
                if button_enabled(ui, "Save", ButtonKind::Prominent, ready).clicked() || (entered && ready) {
                    done = Some(Some(Action::Save(id.clone(), first_text.clone(), second_text.clone())));
                }
                if widgets::button(ui, "Cancel", ButtonKind::Quiet).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    done = Some(None);
                }
            });
            if let Some(result) = done {
                action = result;
                state.editing = None;
            }
            return;
        }
        widgets::sides(
            ui,
            row_id.with("sides"),
            14.0,
            |ui| match kind {
                Kind::Dictionary => {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(text(&item.first, 13.5, Weight::Medium, pal.text));
                        if item.learned {
                            let (rect, response) = ui.allocate_exact_size(vec2(11.0, 11.0), egui::Sense::hover());
                            crate::window::icons::sparkle(ui.painter(), rect.center(), 5.5, pal.accent);
                            response.on_hover_text("Learned from a correction you made");
                        }
                    });
                    if !item.second.is_empty() {
                        ui.add(egui::Label::new(text(format!("heard as {}", item.second), 12.0, Weight::Regular, pal.secondary)).truncate());
                    }
                }
                Kind::Snippets => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.label(text(format!("“{}”", item.first), 13.5, Weight::Medium, pal.text));
                        widgets::inline_icon(ui, Icon::ArrowRight, 11.0, pal.tertiary);
                        ui.add(egui::Label::new(text(&item.second, 13.0, Weight::Regular, pal.secondary)).truncate());
                    });
                }
            },
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if was_hovered {
                    if widgets::icon_button(ui, Icon::Trash, "Delete").clicked() {
                        action = Some(Action::Delete(item.id.clone()));
                    }
                    if widgets::icon_button(ui, Icon::Pencil, "Edit").clicked() {
                        state.editing = Some((item.id.clone(), item.first.clone(), item.second.clone()));
                    }
                } else {
                    ui.allocate_ui_with_layout(vec2(54.0, 26.0), Layout::right_to_left(Align::Center), |ui| {
                        ui.allocate_exact_size(vec2(54.0, 26.0), egui::Sense::hover());
                    });
                }
            },
        );
    });
    let rect = response.response.rect;
    let hovered = ui.rect_contains_pointer(rect) && !editing;
    if hovered != was_hovered {
        ui.data_mut(|d| d.insert_temp(row_id, hovered));
        ui.ctx().request_repaint();
    }
    if hovered {
        let r = |on: bool| if on { 14 } else { 0 };
        let radius = CornerRadius { nw: r(first), ne: r(first), sw: r(last), se: r(last) };
        ui.painter().set(background, RectShape::filled(rect.shrink(1.0), radius, pal.hover));
    }
    action
}
