//! Style: editing strength, writing style per kind of app, and the cleanup rules.

use eframe::egui::{Frame, Id, Label, Margin, Ui};
use kudu_core::{CleanupSettings, EditingStrength, WritingStyle};

use crate::ipc::Request;
use crate::window::theme::{Palette, Weight};
use crate::window::widgets::{self, text, wrapped};
use crate::window::App;

pub fn show(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    widgets::page_header(ui, "Style", "How Kudu turns what you say into clean, well-formatted text.");
    ui.add_space(24.0);

    let mut cleanup = app.data.cleanup;
    let smart = app.data.settings.smart_model.is_some_and(|m| app.data.smart_installed.contains(&m));
    let footer = if smart {
        "Lists, numbers and long passages are formatted by the small model on this PC while you speak, so text still lands almost instantly. Nothing leaves this computer."
    } else {
        "Light and Polished format lists, numbers and long passages with smart formatting, an optional download in Settings. Until then Kudu uses its instant cleanup rules."
    };
    widgets::section(ui, "Editing", Some(footer), |ui| {
        Frame::new().inner_margin(Margin::same(18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let titles: Vec<&str> = EditingStrength::ALL.iter().map(|s| s.title()).collect();
            let selected = EditingStrength::ALL.iter().position(|s| *s == cleanup.editing_strength).unwrap_or(1);
            let width = ui.available_width().min(420.0);
            if let Some(index) = widgets::chip_picker(ui, Id::new("editing-strength"), &titles, selected, Some(width)) {
                cleanup.editing_strength = EditingStrength::ALL[index];
            }
            ui.add_space(10.0);
            wrapped(ui, text(cleanup.editing_strength.summary(), 12.5, Weight::Regular, pal.secondary));
        });
    });
    ui.add_space(24.0);

    widgets::section(ui, "Style by app", Some("Kudu picks the style from the app you're typing in."), |ui| {
        let styles = &mut cleanup.styles;
        style_row(ui, "Personal messages", "WhatsApp, Telegram, Signal", &mut styles.personal_messages, true);
        style_row(ui, "Work messages", "Slack, Teams, Discord", &mut styles.work_messages, true);
        style_row(ui, "Email", "Outlook, Thunderbird, web mail", &mut styles.email, true);
        style_row(ui, "Everything else", "Notes, docs, browsers", &mut styles.other, false);
    });
    ui.add_space(24.0);

    widgets::section(ui, "Cleanup", Some("These rules run instantly on every dictation."), |ui| {
        widgets::row(ui, "Remove filler words", Some("um, uh, erm"), true, |ui| {
            widgets::switch(ui, &mut cleanup.remove_fillers);
        });
        widgets::row(ui, "Apply spoken corrections", Some("“Tuesday, no, Wednesday” → “Wednesday”"), true, |ui| {
            widgets::switch(ui, &mut cleanup.apply_self_corrections);
        });
        widgets::row(ui, "Line break commands", Some("Say “new line” or “new paragraph”"), true, |ui| {
            widgets::switch(ui, &mut cleanup.spoken_commands);
        });
        widgets::row(ui, "Write times as 10:30", None, false, |ui| {
            widgets::switch(ui, &mut cleanup.normalize_times);
        });
    });

    if cleanup != app.data.cleanup {
        save(app, cleanup);
    }
}

fn save(app: &mut App, cleanup: CleanupSettings) {
    app.data.update_cleanup(|stored| *stored = cleanup);
    app.send(Request::Reload);
}

fn style_row(ui: &mut Ui, title: &str, apps: &str, style: &mut WritingStyle, divider: bool) {
    let pal = Palette::of(ui);
    Frame::new().inner_margin(Margin::symmetric(18, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let picker_width = 300.0_f32.min(ui.available_width() * 0.5);
        let show_example = ui.available_width() > 600.0;
        let example = style.example();
        widgets::sides(
            ui,
            Id::new(("style-row", title)),
            16.0,
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    ui.allocate_ui(eframe::egui::vec2(170.0, 0.0), |ui| {
                        ui.set_width(170.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 3.0;
                            ui.label(text(title, 13.5, Weight::Medium, pal.text));
                            ui.label(text(apps, 11.5, Weight::Regular, pal.secondary));
                        });
                    });
                    if show_example {
                        ui.add(Label::new(text(example, 12.5, Weight::Regular, pal.secondary).italics()).truncate());
                    }
                });
            },
            |ui| {
                let titles: Vec<&str> = WritingStyle::ALL.iter().map(|s| s.title()).collect();
                let selected = WritingStyle::ALL.iter().position(|s| s == style).unwrap_or(0);
                if let Some(index) = widgets::chip_picker(ui, Id::new(("style", title)), &titles, selected, Some(picker_width)) {
                    *style = WritingStyle::ALL[index];
                }
            },
        );
    });
    if divider {
        widgets::divider(ui, 18.0);
    }
}
