//! Settings: shortcuts, dictation, the model pack, general and about.

use eframe::egui::{self, vec2, Color32, Frame, Id, Margin, Modal, Sense, Stroke, StrokeKind, Ui, Vec2};
use kudu_engine::Pack;

use crate::ipc::{ModelStatus, Request, ShortcutSlot, UpdateState};
use crate::settings::{RecordingMode, Retention};
use crate::window::data::{default_shortcuts, grouped};
use crate::window::icons::{self, Icon};
use crate::window::theme::{font, Palette, Weight};
use crate::window::widgets::{self, button, keycap, keycaps, text, wrapped, ButtonKind};
use crate::window::pages::home::gigabytes;
use crate::window::{App, WRITER};

const REPOSITORY: &str = "https://github.com/manasdutta04/kudu";
const CREDITS: &str = "https://github.com/manasdutta04/kudu#credits";

pub fn show(app: &mut App, ui: &mut Ui) {
    widgets::page_header(ui, "Settings", "Keys, sounds, models and privacy.");
    ui.add_space(24.0);
    shortcuts(app, ui);
    ui.add_space(24.0);
    dictation(app, ui);
    ui.add_space(24.0);
    models(app, ui);
    ui.add_space(24.0);
    general(app, ui);
    ui.add_space(24.0);
    about(app, ui);
}

// MARK: - Shortcuts

fn shortcuts(app: &mut App, ui: &mut Ui) {
    let shortcuts = app.data.shortcuts();
    let mode = app.data.settings.recording_mode;
    let defaults = default_shortcuts();
    let customized = app.data.settings.shortcuts.as_ref().is_some_and(|s| *s != defaults);
    let mut footer = String::from(
        "Click Change, then press the keys you want. Modifier keys alone (like Ctrl + Win or Right Alt) are held to talk; or use a key with modifiers, like Alt + Space.",
    );
    if cfg!(not(windows)) {
        footer = footer.replace("Win", "Super");
        footer.push_str(
            "\n\nKudu reads the keyboard through the input group; key shortcuts with a letter also reach the app you're in, so modifier-only shortcuts work best.",
        );
    }
    let notation = app.notation;
    widgets::section(ui, "Shortcuts", Some(&footer), |ui| {
        editable_shortcut(app, ui, "Dictate", Some("Hold to talk"), ShortcutSlot::Dictate, Some(shortcuts.dictate.keycaps(notation)));
        if mode != RecordingMode::HoldOnly {
            let prefix = if mode == RecordingMode::TapToToggle { "Tap" } else { "Double-tap" };
            let keys = shortcuts.dictate.keycaps(notation);
            widgets::row(ui, "Hands-free", Some("Talk without holding; tap again to finish"), true, |ui| {
                keycaps(ui, &keys, Some(prefix));
            });
        }
        editable_shortcut(
            app,
            ui,
            "Command mode",
            Some("Hold and say what to do with the selected text"),
            ShortcutSlot::Command,
            shortcuts.command.as_ref().map(|s| s.keycaps(notation)),
        );
        editable_shortcut(
            app,
            ui,
            "Paste last transcript",
            None,
            ShortcutSlot::PasteLast,
            shortcuts.paste_last.as_ref().map(|s| s.keycaps(notation)),
        );
        let more_below = customized || app.shortcut_error.is_some();
        widgets::row(ui, "Cancel", None, more_below, |ui| {
            keycaps(ui, &["Esc".to_string()], Some("While dictating"));
        });
        if let Some(error) = app.shortcut_error.clone() {
            let pal = Palette::of(ui);
            widgets::row(ui, &error, None, customized, |ui| {
                widgets::inline_icon(ui, Icon::Warning, 15.0, pal.orange);
            });
        }
        if customized {
            let detail = format!(
                "{} to dictate, {} for Command mode, {} to paste the last transcript.",
                defaults.dictate.display(notation),
                defaults.command.as_ref().map_or("nothing".into(), |s| s.display(notation)),
                defaults.paste_last.as_ref().map_or("nothing".into(), |s| s.display(notation)),
            );
            widgets::row(ui, "Restore the default shortcuts", Some(&detail), false, |ui| {
                if button(ui, "Restore", ButtonKind::Quiet).clicked() {
                    app.data.update_settings(|s| s.shortcuts = None);
                    app.shortcut_error = None;
                    app.send(Request::Reload);
                }
            });
        }
    });
}

/// A shortcut with a Change button. While recording, it shows the keys held so far.
fn editable_shortcut(app: &mut App, ui: &mut Ui, title: &str, detail: Option<&str>, slot: ShortcutSlot, keys: Option<Vec<String>>) {
    let pal = Palette::of(ui);
    let recording = app.recording == Some(slot);
    let preview = app.preview.clone();
    let mut clicked_change = false;
    let mut clicked_cancel = false;
    widgets::row(ui, title, detail, true, |ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if recording {
            clicked_cancel = button(ui, "Cancel", ButtonKind::Quiet).clicked();
            // The keys held so far, in an accent capsule.
            let content_width = if preview.is_empty() {
                ui.painter().layout_no_wrap("Press keys…".into(), font(12.5, Weight::Regular), pal.accent).size().x
            } else {
                preview
                    .iter()
                    .map(|k| ui.painter().layout_no_wrap(k.clone(), font(11.5, Weight::Semibold), pal.text).size().x.max(8.0) + 14.0)
                    .sum::<f32>()
                    + 4.0 * (preview.len() as f32 - 1.0)
            };
            let (rect, _) = ui.allocate_exact_size(vec2(content_width + 20.0, 30.0), Sense::hover());
            ui.painter().rect_stroke(rect, 15.0, Stroke::new(1.0, pal.accent.gamma_multiply(0.7)), StrokeKind::Inside);
            let mut inner = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect.shrink2(vec2(10.0, 4.0)))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            inner.spacing_mut().item_spacing.x = 4.0;
            if preview.is_empty() {
                inner.label(text("Press keys…", 12.5, Weight::Regular, pal.accent));
            } else {
                for key in &preview {
                    keycap(&mut inner, key);
                }
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        } else {
            clicked_change = button(ui, "Change", ButtonKind::Quiet).clicked();
            match &keys {
                Some(keys) => keycaps(ui, keys, None),
                None => {
                    ui.label(text("Off", 12.5, Weight::Regular, pal.secondary));
                }
            }
        }
    });
    if clicked_change {
        if let Some(other) = app.recording {
            if other != slot {
                app.send(Request::CancelRecording);
            }
        }
        app.recording = Some(slot);
        app.preview.clear();
        app.shortcut_error = None;
        app.send(Request::RecordShortcut { slot });
    }
    if clicked_cancel {
        app.recording = None;
        app.preview.clear();
        app.send(Request::CancelRecording);
    }
}

// MARK: - Dictation

fn dictation(app: &mut App, ui: &mut Ui) {
    let mut settings = app.data.settings.clone();
    let dictate = app.dictate_name();
    let microphones: Vec<String> = app.status.as_ref().map(|s| s.microphones.clone()).unwrap_or_default();
    widgets::section(ui, "Dictation", None, |ui| {
        let hint = settings.recording_mode.hint(&dictate);
        widgets::row(ui, "Recording", Some(&hint), true, |ui| {
            let titles: Vec<String> = RecordingMode::ALL.iter().map(|m| m.title().to_string()).collect();
            let selected = RecordingMode::ALL.iter().position(|m| *m == settings.recording_mode);
            if let Some(index) = widgets::value_menu(ui, Id::new("recording-mode"), &titles, selected, "Choose") {
                settings.recording_mode = RecordingMode::ALL[index];
            }
        });
        widgets::row(ui, "Microphone", Some("Kudu opens it only while you dictate."), true, |ui| {
            let mut choices: Vec<Option<String>> = vec![None];
            choices.extend(microphones.iter().cloned().map(Some));
            if let Some(current) = &settings.microphone {
                if !microphones.contains(current) {
                    choices.push(Some(current.clone()));
                }
            }
            let titles: Vec<String> = choices
                .iter()
                .map(|c| match c {
                    None => "System default".to_string(),
                    Some(name) if !microphones.contains(name) => format!("{name} (not connected)"),
                    Some(name) => name.clone(),
                })
                .collect();
            let selected = choices.iter().position(|c| *c == settings.microphone);
            if let Some(index) = widgets::value_menu(ui, Id::new("microphone"), &titles, selected, "System default") {
                settings.microphone = choices[index].clone();
            }
        });
        widgets::row(ui, "Sounds", Some("A soft tone when dictation starts and stops."), true, |ui| {
            widgets::switch(ui, &mut settings.sounds);
        });
        widgets::row(
            ui,
            "Learn from my corrections",
            Some("When you fix a name Kudu misspelled, it remembers it. Learned words appear in the Dictionary with ✦."),
            true,
            |ui| {
                widgets::switch(ui, &mut settings.learn_from_corrections);
            },
        );
        widgets::row(
            ui,
            "Spell names from the screen",
            Some("Reads the names and terms in the window you're dictating into, to spell them right. Nothing is saved."),
            false,
            |ui| {
                widgets::switch(ui, &mut settings.screen_context);
            },
        );
    });
    if settings != app.data.settings {
        app.data.update_settings(|stored| {
            stored.recording_mode = settings.recording_mode;
            stored.microphone = settings.microphone.clone();
            stored.sounds = settings.sounds;
            stored.learn_from_corrections = settings.learn_from_corrections;
            stored.screen_context = settings.screen_context;
        });
        app.send(Request::Reload);
    }
}

// MARK: - Models

/// What a model row offers.
enum ModelAction {
    Use,
    Download,
    Cancel,
    Remove,
}

/// The right side of a model row for its state. Returns the clicked action.
fn model_control(ui: &mut Ui, state: Option<&ModelStatus>, selected: bool, installed: bool, removable: bool) -> Option<ModelAction> {
    let pal = Palette::of(ui);
    let mut action = None;
    ui.spacing_mut().item_spacing.x = 10.0;
    match state.filter(|_| selected) {
        Some(ModelStatus::Downloading { fraction }) => {
            if button(ui, "Cancel", ButtonKind::Quiet).clicked() {
                action = Some(ModelAction::Cancel);
            }
            ui.label(text(format!("{}%", (fraction * 100.0).round() as i32), 12.0, Weight::Medium, pal.secondary));
            widgets::progress_bar(ui, *fraction as f32, 120.0);
        }
        Some(ModelStatus::Loading) => {
            ui.label(text("Loading…", 12.5, Weight::Regular, pal.secondary));
        }
        Some(ModelStatus::Failed { .. }) => {
            if button(ui, "Try again", ButtonKind::Quiet).clicked() {
                action = Some(ModelAction::Download);
            }
        }
        Some(ModelStatus::Ready) => {
            if removable && button(ui, "Remove", ButtonKind::Destructive).clicked() {
                action = Some(ModelAction::Remove);
            }
            in_use(ui);
        }
        Some(ModelStatus::Missing) | None if selected && installed => {
            if removable && button(ui, "Remove", ButtonKind::Destructive).clicked() {
                action = Some(ModelAction::Remove);
            }
            in_use(ui);
        }
        _ if installed => {
            if button(ui, "Use", ButtonKind::Quiet).clicked() {
                action = Some(ModelAction::Use);
            }
        }
        _ => {
            if widgets::button_with_icon(ui, Some(Icon::Download), "Download", ButtonKind::Quiet, true).clicked() {
                action = Some(ModelAction::Download);
            }
        }
    }
    action
}

fn in_use(ui: &mut Ui) {
    let pal = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        ui.label(text("In use", 12.5, Weight::Medium, pal.green));
        widgets::inline_icon(ui, Icon::Check, 12.0, pal.green);
    });
}

/// What a model row shows on the left.
struct ModelInfo<'a> {
    title: &'a str,
    size: &'a str,
    summary: &'a str,
    selected: bool,
    failure: Option<&'a str>,
}

/// A model choice: a radio dot, the title and size, and the summary.
fn model_row(ui: &mut Ui, info: ModelInfo<'_>, divider: bool, control: impl FnOnce(&mut Ui)) {
    let pal = Palette::of(ui);
    let ModelInfo { title, size, summary, selected, failure } = info;
    Frame::new().inner_margin(Margin::symmetric(18, 13)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        widgets::sides(
            ui,
            Id::new(("model-row", title)),
            16.0,
            |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let (dot, _) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::hover());
                    let center = dot.center();
                    if selected {
                        ui.painter().circle_filled(center, 8.0, pal.accent);
                        ui.painter().circle_filled(center, 3.0, Color32::WHITE);
                    } else {
                        ui.painter().circle_stroke(center, 7.5, Stroke::new(1.2, pal.tertiary));
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            ui.label(text(title, 13.5, Weight::Medium, pal.text));
                            ui.label(text(size, 11.5, Weight::Regular, pal.secondary));
                        });
                        wrapped(ui, text(summary, 11.5, Weight::Regular, pal.secondary));
                        if let Some(failure) = failure {
                            wrapped(ui, text(failure, 11.5, Weight::Regular, pal.red));
                        }
                    });
                });
            },
            control,
        );
    });
    if divider {
        widgets::divider(ui, 46.0);
    }
}

fn models(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    let current = app.data.settings.pack();
    let state = app.pack_status();
    let mut chosen: Option<(Pack, ModelAction)> = None;
    let command = app.data.shortcuts().command.map(|s| s.display(app.notation));
    let writer_ready = app.status.as_ref().is_some_and(|s| matches!(s.smart, ModelStatus::Ready | ModelStatus::Loading));
    let installed = |pack: Pack| {
        app.data.speech_installed.contains(&pack.speech()) && (!WRITER || app.data.writer_installed.contains(&pack.writer()))
    };
    let unused = app.data.speech_installed.iter().any(|m| *m != current.speech())
        || app.data.writer_installed.iter().any(|m| *m != current.writer());
    widgets::section(
        ui,
        "Models",
        Some("Each pack is a speech model plus a writer that formats your text and runs Command mode. Both run on this PC and work offline once downloaded."),
        |ui| {
            for pack in Pack::ALL {
                let selected = pack == current;
                let size = format!("{} download · {} on disk", gigabytes(pack.download_bytes()), gigabytes(u64::from(pack.disk_mb()) * 1_000_000));
                let summary = format!("{} Speech recognition and the writer.", pack.summary());
                let failure = match (&state, selected) {
                    (Some(ModelStatus::Failed { message }), true) => Some(message.as_str()),
                    _ => None,
                };
                let info = ModelInfo { title: pack.name(), size: &size, summary: &summary, selected, failure };
                let on_disk = installed(pack);
                model_row(ui, info, true, |ui| {
                    if let Some(action) = model_control(ui, state.as_ref(), selected, on_disk, false) {
                        chosen = Some((pack, action));
                    }
                });
            }
            widgets::row(ui, "Unused models", Some("Packs you switched away from stay on disk until you remove them."), true, |ui| {
                if widgets::button_enabled(ui, "Remove unused models", ButtonKind::Quiet, unused).clicked() {
                    app.send(Request::RemoveUnusedModels);
                }
            });
            let title = match &command {
                Some(keys) => format!("Command mode: select text, hold {keys} and say what to do"),
                None => "Command mode: select text, hold the Command shortcut and say what to do".to_string(),
            };
            widgets::row(
                ui,
                &title,
                Some("“Make this more formal” · “Translate to Hindi” · “Fix the grammar” · “Summarize”. With nothing selected, Kudu writes it: “Write a polite reply saying I'll be late.”"),
                false,
                |ui| {
                    if writer_ready {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(text("On this PC", 11.5, Weight::Medium, pal.accent));
                            let (rect, _) = ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
                            icons::sparkle(ui.painter(), rect.center(), 5.5, pal.accent);
                        });
                    } else {
                        ui.label(text("Needs the writer", 11.5, Weight::Regular, pal.secondary));
                    }
                },
            );
        },
    );
    match chosen {
        Some((pack, ModelAction::Use | ModelAction::Download)) => app.send(Request::DownloadPack { pack }),
        Some((_, ModelAction::Cancel)) => app.send(Request::CancelDownload),
        Some((_, ModelAction::Remove)) | None => {}
    }
}

// MARK: - General and about

fn general(app: &mut App, ui: &mut Ui) {
    let launch = app.status.as_ref().map(|s| s.launch_at_login);
    let mut retention = app.data.settings.retention;
    let count = app.data.history.len();
    widgets::section(
        ui,
        "General",
        Some("The microphone is on only while you dictate. Audio is never saved; history stays on this computer."),
        |ui| {
            widgets::row(ui, "Open Kudu at login", None, true, |ui| {
                let mut on = launch.unwrap_or(false);
                if widgets::switch(ui, &mut on).changed() {
                    app.send(Request::SetLaunchAtLogin { enabled: on });
                    if let Some(status) = app.status.as_mut() {
                        status.launch_at_login = on;
                    }
                }
            });
            widgets::row(ui, "Keep history", None, true, |ui| {
                let titles: Vec<String> = Retention::ALL.iter().map(|r| r.title().to_string()).collect();
                let selected = Retention::ALL.iter().position(|r| *r == retention);
                if let Some(index) = widgets::value_menu(ui, Id::new("retention"), &titles, selected, "Choose") {
                    retention = Retention::ALL[index];
                }
            });
            let saved = match count {
                1 => "1 dictation saved on this computer".to_string(),
                n => format!("{} dictations saved on this computer", grouped(n)),
            };
            widgets::row(ui, &saved, None, true, |ui| {
                if widgets::button_enabled(ui, "Clear history…", ButtonKind::Destructive, count > 0).clicked() {
                    app.confirm_clear = true;
                }
            });
            widgets::row(ui, "Quit Kudu", Some("Stops listening for the shortcut until you open Kudu again."), false, |ui| {
                if button(ui, "Quit", ButtonKind::Quiet).clicked() {
                    app.send(Request::Quit);
                }
            });
        },
    );
    if retention != app.data.settings.retention {
        app.data.update_settings(|s| s.retention = retention);
        app.send(Request::Reload);
    }
}

fn about(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    let pack = app.data.settings.pack();
    widgets::section(ui, "About", None, |ui| {
        widgets::row(ui, "Version", None, true, |ui| {
            ui.label(text(env!("CARGO_PKG_VERSION"), 13.0, Weight::Regular, pal.secondary));
        });
        widgets::row(ui, "Models", None, true, |ui| {
            ui.label(text(format!("{}, on this computer", pack.name()), 13.0, Weight::Regular, pal.secondary));
        });
        let mut automatic = app.data.settings.check_updates;
        widgets::row(
            ui,
            "Check for updates automatically",
            Some("Once a day, Kudu asks GitHub which version is the latest. Nothing about you is sent."),
            true,
            |ui| {
                widgets::switch(ui, &mut automatic);
            },
        );
        if automatic != app.data.settings.check_updates {
            app.data.update_settings(|s| s.check_updates = automatic);
            app.send(Request::Reload);
        }
        let version = env!("CARGO_PKG_VERSION");
        let update = app.status.as_ref().map(|s| s.update.clone()).unwrap_or_default();
        let detail = match &update {
            UpdateState::Idle => format!("You're on {version}."),
            UpdateState::Checking => "Checking…".to_string(),
            UpdateState::UpToDate => format!("You're on {version}, the latest version."),
            UpdateState::Available { version: latest } => format!("Kudu {latest} is available. You're on {version}, an older version."),
            UpdateState::Installing => "Installing… Kudu will close while it updates.".to_string(),
            UpdateState::Failed { message } => format!("{message}. You're on {version}."),
        };
        widgets::row(ui, "Updates", Some(&detail), true, |ui| match &update {
            UpdateState::Available { .. } => {
                let label = if cfg!(windows) { "Install update" } else { "Download update" };
                if button(ui, label, ButtonKind::Prominent).clicked() {
                    app.send(Request::InstallUpdate);
                }
            }
            UpdateState::Checking | UpdateState::Installing => {
                ui.label(text("…", 13.0, Weight::Regular, pal.secondary));
            }
            _ => {
                if button(ui, "Check now", ButtonKind::Quiet).clicked() {
                    app.send(Request::CheckForUpdates);
                }
            }
        });
        widgets::row(ui, "Open-source credits", Some("The open models and libraries Kudu is built on, and their licenses."), true, |ui| {
            if link(ui, "View").clicked() {
                open_url(CREDITS);
            }
        });
        widgets::row(ui, "Everything stays on this computer.", Some("Kudu is open source under the MIT license."), false, |ui| {
            let response = link(ui, "github.com/manasdutta04/kudu");
            if response.clicked() {
                open_url(REPOSITORY);
            }
        });
    });
}

fn link(ui: &mut Ui, label: &str) -> egui::Response {
    let pal = Palette::of(ui);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font(13.0, Weight::Regular), pal.accent);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(REPOSITORY);
    if response.hovered() {
        ui.painter().hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0, pal.accent));
    }
    ui.painter().galley(rect.min, galley, pal.accent);
    response
}

/// Opens a web page in the default browser.
fn open_url(url: &str) {
    let result = if cfg!(windows) {
        std::process::Command::new("explorer").arg(url).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if let Err(error) = result {
        log::warn!("Couldn't open {url}: {error}");
    }
}

/// "Clear all dictation history?", shown over the window.
pub fn confirm_dialog(app: &mut App, ui: &mut Ui) {
    if !app.confirm_clear {
        return;
    }
    let pal = Palette::of(ui);
    let frame = Frame::new()
        .fill(pal.panel)
        .stroke(Stroke::new(1.0, pal.hairline))
        .corner_radius(14)
        .inner_margin(Margin::same(22))
        .shadow(ui.visuals().window_shadow);
    let mut clear = false;
    let mut cancel = false;
    let response = Modal::new(Id::new("confirm-clear"))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(if pal.dark { 120 } else { 60 }))
        .show(ui.ctx(), |ui| {
            ui.set_width(340.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(text("Clear all dictation history?", 15.0, Weight::Semibold, pal.text));
            ui.add_space(6.0);
            wrapped(ui, text("This deletes every saved dictation from this computer. It can't be undone.", 12.5, Weight::Regular, pal.secondary));
            ui.add_space(18.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if button(ui, "Clear history", ButtonKind::Destructive).clicked() {
                    clear = true;
                }
                if button(ui, "Cancel", ButtonKind::Quiet).clicked() {
                    cancel = true;
                }
            });
        });
    if response.should_close() || clear || cancel {
        app.confirm_clear = false;
    }
    if clear {
        app.data.rewrite_history(|_| false);
        app.send(Request::Reload);
    }
}
