//! Meetings: turn notes on, take them (the live card), and read past meetings' notes and
//! transcripts. The same page as the Mac app's.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use eframe::egui::{self, vec2, Align, Color32, CursorIcon, Frame, Id, Layout, Margin, Modal, Sense, Stroke, Ui};
use kudu_core::meeting::{self, AudioSource, Meeting, MeetingSegment, NotesGenerator};

use crate::ipc::{MeetingState, ModelStatus, Request};
use crate::settings::MeetingAutoStart;
use crate::window::data::{day_title, time_of_day};
use crate::window::icons::{self, Icon};
use crate::window::theme::{font, Palette, Weight};
use crate::window::widgets::{self, button, button_enabled, caps, card, keycaps, text, wrapped, ButtonKind};
use crate::window::App;

#[derive(Default)]
pub struct MeetingsState {
    /// The meeting open in the detail view.
    pub selected: Option<String>,
    pub transcript_tab: bool,
    pub consent: bool,
    pub confirm_delete: Option<String>,
    pub confirm_delete_all: bool,
    /// The transcript of the open (or live) meeting, read when the meetings change.
    cache: Option<(String, u64, Vec<MeetingSegment>)>,
}

pub fn show(app: &mut App, ui: &mut Ui) {
    if let Some(id) = app.meetings.selected.clone() {
        match app.data.meetings.iter().find(|m| m.id == id).cloned() {
            Some(meeting) => detail(app, ui, &meeting),
            None => app.meetings.selected = None,
        }
    } else {
        list(app, ui);
    }
    dialogs(app, ui);
}

fn segments(app: &mut App, id: &str) -> Vec<MeetingSegment> {
    let version = app.data.meetings_version;
    let fresh = app.meetings.cache.as_ref().is_some_and(|(cached, v, _)| cached == id && *v == version);
    if !fresh {
        let segments = crate::meetings::segments(&app.data.paths.data, id);
        app.meetings.cache = Some((id.to_string(), version, segments));
    }
    app.meetings.cache.as_ref().map(|(_, _, s)| s.clone()).unwrap_or_default()
}

fn meeting_state(app: &App) -> MeetingState {
    app.status.as_ref().map(|s| s.meeting.clone()).unwrap_or_default()
}

// MARK: - List

fn list(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    widgets::page_header(ui, "Meetings", "Notes and transcripts of your calls, written on this computer.");
    ui.add_space(24.0);
    if app.data.settings.meetings.enabled {
        live_card(app, ui);
    } else {
        enable_card(app, ui);
    }
    let state = meeting_state(app);
    let live_id = match &state {
        MeetingState::Recording { id, .. } => Some(id.clone()),
        _ => None,
    };
    let meetings: Vec<Meeting> = app.data.meetings.iter().filter(|m| Some(&m.id) != live_id.as_ref()).cloned().collect();
    if meetings.is_empty() {
        if app.data.settings.meetings.enabled {
            ui.add_space(24.0);
            widgets::empty_state(ui, Icon::Meetings, "Your meetings will appear here.", None);
        }
        return;
    }
    let today = app.data.calendar.day(SystemTime::now());
    let mut groups: Vec<(kudu_core::Day, Vec<Meeting>)> = Vec::new();
    for meeting in meetings {
        let day = app.data.calendar.day(meeting.started);
        match groups.last_mut() {
            Some((last, items)) if *last == day => items.push(meeting),
            _ => groups.push((day, vec![meeting])),
        }
    }
    for (day, items) in groups {
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(caps(&day_title(day, today), 11.0, pal.secondary));
        });
        ui.add_space(8.0);
        card(ui, 14, Margin::ZERO, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, meeting) in items.iter().enumerate() {
                if index > 0 {
                    widgets::divider(ui, 102.0);
                }
                let writing = matches!(&state, MeetingState::Writing { id, .. } if *id == meeting.id);
                if meeting_row(app, ui, meeting, writing) {
                    app.meetings.selected = Some(meeting.id.clone());
                    app.meetings.transcript_tab = false;
                }
            }
        });
    }
    ui.add_space(16.0);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if button_enabled(ui, "Delete all meetings…", ButtonKind::Destructive, live_id.is_none()).clicked() {
            app.meetings.confirm_delete_all = true;
        }
    });
}

/// One meeting in the list. Returns true when clicked.
fn meeting_row(app: &App, ui: &mut Ui, meeting: &Meeting, writing: bool) -> bool {
    let pal = Palette::of(ui);
    let (_, seconds) = app.data.calendar.local(meeting.started);
    let subtitle = match &meeting.notes {
        Some(notes) if !notes.summary.is_empty() => notes.summary.clone(),
        _ => [meeting.app.clone(), Some(format!("{} words", meeting.words))].into_iter().flatten().collect::<Vec<_>>().join(" · "),
    };
    let response = Frame::new().inner_margin(Margin::symmetric(18, 12)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let (time, _) = ui.allocate_exact_size(vec2(84.0, 18.0), Sense::hover());
            ui.painter().text(time.left_center(), egui::Align2::LEFT_CENTER, time_of_day(seconds), font(12.5, Weight::Regular), pal.secondary);
            let right = 150.0;
            ui.allocate_ui_with_layout(vec2((ui.available_width() - right).max(80.0), 0.0), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.add(egui::Label::new(text(&meeting.title, 13.5, Weight::Semibold, pal.text)).truncate());
                ui.add(egui::Label::new(text(&subtitle, 12.5, Weight::Regular, pal.secondary)).truncate());
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                widgets::inline_icon(ui, Icon::ChevronRight, 10.0, pal.tertiary);
                ui.label(text(meeting::length(meeting.duration(SystemTime::now())), 11.5, Weight::Regular, pal.secondary));
                if writing {
                    ui.add(egui::Spinner::new().size(12.0).color(pal.secondary));
                }
            });
        });
    });
    let response = ui.interact(response.response.rect, Id::new(("meeting-row", &meeting.id)), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(response.rect, 0.0, Palette::alpha(pal.text, 0.025));
    }
    response.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

// MARK: - Turning it on

fn enable_card(app: &mut App, ui: &mut Ui) {
    let mut enable = false;
    widgets::midnight_card(ui, 24.0, 0.0, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let title = widgets::display_title(&[("Notes for every meeting, ", false), ("written on this computer", true)], 28.0, Color32::WHITE);
        ui.add(egui::Label::new(title).wrap());
        ui.add_space(12.0);
        wrapped(
            ui,
            text(
                "Kudu listens to your calls in Zoom, Teams, Meet or any other app, transcribes them as they happen, and writes up the summary, decisions and action items when the call ends. Nothing leaves your computer and the audio is never saved.",
                13.0,
                Weight::Regular,
                Color32::WHITE.gamma_multiply(0.8),
            ),
        );
        ui.add_space(14.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for label in ["Summary", "Decisions", "Action items", "Full transcript"] {
                widgets::chip(ui, label, false, false);
            }
        });
        ui.add_space(16.0);
        let (rect, response) = ui.allocate_exact_size(vec2(200.0, 34.0), Sense::click());
        ui.painter().rect_filled(rect, 17.0, if response.hovered() { Color32::from_gray(235) } else { Color32::WHITE });
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Turn on meeting notes", font(13.0, Weight::Semibold), Color32::from_rgb(20, 18, 46));
        if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            enable = true;
        }
    });
    if enable {
        app.meetings.consent = true;
    }
}

// MARK: - Live

fn live_card(app: &mut App, ui: &mut Ui) {
    let pal = Palette::of(ui);
    let state = meeting_state(app);
    let shortcut = app.data.shortcuts().meeting.map(|s| s.keycaps(app.notation));
    let speech_ready = app.status.as_ref().is_some_and(|s| s.speech == ModelStatus::Ready);
    let detection = app.status.as_ref().is_some_and(|s| s.meeting_detection);
    let auto = app.data.settings.meetings.auto_start;
    let live = match &state {
        MeetingState::Recording { id, .. } => segments(app, id),
        _ => Vec::new(),
    };
    let mut start = false;
    let mut stop = false;
    card(ui, 16, Margin::same(20), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        match &state {
            MeetingState::Recording { started, app: meeting_app, system_audio, .. } => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
                let elapsed = (now - started).max(0.0);
                ui.ctx().request_repaint_after(Duration::from_millis(500));
                widgets::sides(
                    ui,
                    Id::new("meeting-live"),
                    12.0,
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 14.0;
                            live_dot(ui, &pal);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let title = meeting_app.as_ref().map_or_else(|| "Taking notes".to_string(), |app| format!("Taking notes · {app}"));
                                ui.label(text(title, 14.0, Weight::Semibold, pal.text));
                                let sources = if *system_audio { "Your microphone and the other people" } else { "Your microphone only" };
                                ui.label(text(sources, 12.5, Weight::Regular, pal.secondary));
                            });
                        });
                    },
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        stop = button(ui, "Stop", ButtonKind::Prominent).clicked();
                        ui.label(text(meeting::clock(elapsed), 15.0, Weight::Medium, pal.secondary));
                    },
                );
                let turns = meeting::turns(&live);
                if !turns.is_empty() {
                    ui.add_space(14.0);
                    Frame::new().fill(Palette::alpha(pal.selection, 0.5)).corner_radius(10).inner_margin(Margin::same(12)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 6.0;
                        for turn in turns.iter().rev().take(3).rev() {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                if *system_audio {
                                    let color = if turn.source == AudioSource::Mic { pal.accent } else { pal.secondary };
                                    let (rect, _) = ui.allocate_exact_size(vec2(46.0, 16.0), Sense::hover());
                                    ui.painter().text(rect.left_top(), egui::Align2::LEFT_TOP, turn.source.speaker(), font(11.5, Weight::Semibold), color);
                                }
                                ui.add(egui::Label::new(text(&turn.text, 12.5, Weight::Regular, pal.secondary)).wrap());
                            });
                        }
                    });
                }
            }
            MeetingState::Writing { done, total, .. } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    ui.add(egui::Spinner::new().size(16.0).color(pal.secondary));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(text("Writing the notes…", 14.0, Weight::Semibold, pal.text));
                        let detail = if *total > 1 { format!("Step {} of {total}", (done + 1).min(*total)) } else { "This takes a few seconds.".into() };
                        ui.label(text(detail, 12.5, Weight::Regular, pal.secondary));
                    });
                });
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
            MeetingState::Idle => {
                let blocker = if speech_ready { None } else { Some("Download a speech model first.") };
                let detail = blocker.map(str::to_string).unwrap_or_else(|| match auto {
                    MeetingAutoStart::Off => "Press the shortcut or Start when the call begins.".into(),
                    _ if !detection => "Press the shortcut or Start when the call begins. (This computer can't tell when a meeting starts.)".into(),
                    MeetingAutoStart::Ask => "Kudu reminds you when a meeting app starts using the microphone.".into(),
                    MeetingAutoStart::Automatic => "Kudu starts by itself when a meeting begins, and stops when it ends.".into(),
                });
                widgets::sides(
                    ui,
                    Id::new("meeting-idle"),
                    12.0,
                    |ui| {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 14.0;
                            let (tile, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
                            ui.painter().rect_filled(tile, 11.0, Palette::alpha(pal.accent, 0.12));
                            icons::paint(ui.painter(), tile.shrink(12.0), Icon::Meetings, pal.accent);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.label(text("Take notes in your next meeting", 14.0, Weight::Semibold, pal.text));
                                wrapped(ui, text(&detail, 12.5, Weight::Regular, pal.secondary));
                            });
                        });
                    },
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        start = button_enabled(ui, "Start", ButtonKind::Prominent, blocker.is_none()).clicked();
                        if let Some(keys) = &shortcut {
                            keycaps(ui, keys, None);
                        }
                    },
                );
            }
        }
    });
    if start {
        app.send(Request::StartMeeting);
    }
    if stop {
        app.send(Request::StopMeeting);
    }
}

fn live_dot(ui: &mut Ui, pal: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
    let time = ui.input(|i| i.time);
    let pulse = ((time * std::f64::consts::PI).sin() * 0.5 + 0.5) as f32;
    ui.painter().circle_filled(rect.center(), 12.0 + 3.0 * pulse, Palette::alpha(pal.red, 0.18));
    ui.painter().circle_filled(rect.center(), 5.0, pal.red);
    ui.ctx().request_repaint_after(Duration::from_millis(33));
}

// MARK: - Detail

fn detail(app: &mut App, ui: &mut Ui, meeting: &Meeting) {
    let pal = Palette::of(ui);
    let segments = segments(app, &meeting.id);
    let state = meeting_state(app);
    let writing = matches!(&state, MeetingState::Writing { id, .. } if *id == meeting.id);
    let busy = !matches!(state, MeetingState::Idle);

    let (back, response) = ui.allocate_exact_size(vec2(100.0, 20.0), Sense::click());
    let color = if response.hovered() { pal.text } else { pal.secondary };
    icons::paint(ui.painter(), egui::Rect::from_min_size(back.left_top() + vec2(0.0, 5.0), vec2(10.0, 10.0)), Icon::ChevronLeft, color);
    ui.painter().text(back.left_center() + vec2(16.0, 0.0), egui::Align2::LEFT_CENTER, "Meetings", font(12.5, Weight::Medium), color);
    if response.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        app.meetings.selected = None;
        return;
    }
    ui.add_space(14.0);

    let today = app.data.calendar.day(SystemTime::now());
    let day = app.data.calendar.day(meeting.started);
    let (_, seconds) = app.data.calendar.local(meeting.started);
    let mut meta = vec![format!("{}, {}", title_case(&day_title(day, today)), time_of_day(seconds)), meeting::length(meeting.duration(SystemTime::now()))];
    meta.extend(meeting.app.clone());
    let markdown = meeting::markdown(meeting, &segments, &meta[0], SystemTime::now());
    let transcript = meeting::transcript_text(&segments, meeting.heard_others());
    let mut action: Option<&str> = None;
    widgets::sides(
        ui,
        Id::new(("meeting-title", &meeting.id)),
        16.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.add(egui::Label::new(egui::RichText::new(&meeting.title).font(crate::window::theme::display(30.0)).color(pal.text)).wrap());
            wrapped(ui, text(meta.join(" · "), 13.0, Weight::Regular, pal.secondary));
        },
        |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let more = widgets::icon_button(ui, Icon::Ellipsis, "More");
            egui::Popup::menu(&more).gap(4.0).show(|ui| {
                ui.set_min_width(200.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                if widgets::menu_item(ui, "Copy transcript only", false, false) {
                    action = Some("copy-transcript");
                    ui.close();
                }
                if !busy && widgets::menu_item(ui, "Write the notes again", false, false) {
                    action = Some("regenerate");
                    ui.close();
                }
                if widgets::menu_item(ui, "Show the files", false, false) {
                    action = Some("open-folder");
                    ui.close();
                }
                ui.add_space(4.0);
                widgets::divider(ui, 0.0);
                ui.add_space(4.0);
                if widgets::menu_item(ui, "Delete…", false, true) {
                    action = Some("delete");
                    ui.close();
                }
            });
            if widgets::button_with_icon(ui, Some(Icon::Copy), "Copy", ButtonKind::Quiet, true).on_hover_text("Copy the notes and transcript as Markdown").clicked() {
                action = Some("copy");
            }
        },
    );
    match action {
        Some("copy") => ui.ctx().copy_text(markdown),
        Some("copy-transcript") => ui.ctx().copy_text(transcript),
        Some("regenerate") => app.send(Request::RegenerateNotes { id: meeting.id.clone() }),
        Some("open-folder") => open_folder(&crate::meetings::dir(&app.data.paths.data).join(&meeting.id)),
        Some("delete") => app.meetings.confirm_delete = Some(meeting.id.clone()),
        _ => {}
    }

    if !meeting.heard_others() {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            widgets::inline_icon(ui, Icon::Warning, 13.0, pal.secondary);
            wrapped(ui, text("Only your microphone was recorded, so the other people may be missing from the transcript.", 11.5, Weight::Regular, pal.secondary));
        });
    }

    ui.add_space(18.0);
    let selected = usize::from(app.meetings.transcript_tab);
    if let Some(index) = widgets::chip_picker(ui, Id::new("meeting-tab"), &["Notes", "Transcript"], selected, Some(240.0)) {
        app.meetings.transcript_tab = index == 1;
    }
    ui.add_space(18.0);
    if app.meetings.transcript_tab {
        transcript_view(ui, meeting, &segments);
    } else {
        notes_view(ui, meeting, writing, &state);
    }
}

fn notes_view(ui: &mut Ui, meeting: &Meeting, writing: bool, state: &MeetingState) {
    let pal = Palette::of(ui);
    if writing {
        card(ui, 14, Margin::same(20), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.add(egui::Spinner::new().size(14.0).color(pal.secondary));
                let label = match state {
                    MeetingState::Writing { done, total, .. } if *total > 1 => format!("Writing the notes… step {} of {total}", (done + 1).min(*total)),
                    _ => "Writing the notes…".into(),
                };
                ui.label(text(label, 13.0, Weight::Regular, pal.secondary));
            });
        });
        return;
    }
    let Some(notes) = &meeting.notes else {
        widgets::empty_state(ui, Icon::Document, "No notes yet.", None);
        return;
    };
    let block = |ui: &mut Ui, title: &str, add: &mut dyn FnMut(&mut Ui)| {
        card(ui, 14, Margin::same(20), |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(caps(title, 11.0, pal.secondary));
            add(ui);
        });
        ui.add_space(14.0);
    };
    if !notes.summary.is_empty() {
        block(ui, "Summary", &mut |ui| {
            ui.add(egui::Label::new(text(&notes.summary, 14.0, Weight::Regular, pal.text).line_height(Some(21.0))).wrap());
        });
    }
    for (title, items, icon) in [
        ("Action items", &notes.actions, Icon::Circle),
        ("Decisions", &notes.decisions, Icon::Check),
        ("Key points", &notes.key_points, Icon::Dot),
    ] {
        if items.is_empty() {
            continue;
        }
        block(ui, title, &mut |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            for item in items {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 19.0), Sense::hover());
                    icons::paint(ui.painter(), egui::Rect::from_center_size(rect.center(), vec2(10.0, 10.0)), icon, pal.accent);
                    ui.add(egui::Label::new(text(item, 13.5, Weight::Regular, pal.text).line_height(Some(19.0))).wrap());
                });
            }
        });
    }
    let note = match notes.generator {
        NotesGenerator::KuduWriter => "Written by the Kudu writer model on this computer. Check important details against the transcript.",
        NotesGenerator::AppleIntelligence => "Written by Apple Intelligence on a Mac.",
        NotesGenerator::Extractive => "Basic notes picked from the transcript (the writer model wasn't available).",
    };
    wrapped(ui, text(note, 11.5, Weight::Regular, pal.tertiary));
}

fn transcript_view(ui: &mut Ui, meeting: &Meeting, segments: &[MeetingSegment]) {
    let pal = Palette::of(ui);
    let turns = meeting::turns(segments);
    card(ui, 14, Margin::ZERO, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        if turns.is_empty() {
            Frame::new().inner_margin(Margin::same(20)).show(ui, |ui| {
                ui.label(text("Nothing was transcribed.", 13.0, Weight::Regular, pal.secondary));
            });
        }
        for (index, turn) in turns.iter().enumerate() {
            if index > 0 {
                widgets::divider(ui, 84.0);
            }
            Frame::new().inner_margin(Margin::symmetric(18, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let (rect, _) = ui.allocate_exact_size(vec2(52.0, 18.0), Sense::hover());
                    ui.painter().text(rect.left_top() + vec2(0.0, 1.0), egui::Align2::LEFT_TOP, meeting::clock(turn.start), font(12.0, Weight::Regular), pal.tertiary);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        if meeting.heard_others() {
                            let color = if turn.source == AudioSource::Mic { pal.accent } else { pal.secondary };
                            ui.label(text(turn.source.speaker(), 12.0, Weight::Semibold, color));
                        }
                        ui.add(egui::Label::new(text(&turn.text, 13.5, Weight::Regular, pal.text).line_height(Some(19.0))).wrap());
                    });
                });
            });
        }
    });
}

// MARK: - Dialogs

fn dialogs(app: &mut App, ui: &mut Ui) {
    if app.meetings.consent {
        let mut answer = None;
        modal(ui, "meeting-consent", |ui, pal| {
            ui.label(text("Before you take notes", 16.0, Weight::Semibold, pal.text));
            ui.add_space(12.0);
            for line in [
                "Let people know you're taking notes. In some places everyone in a call must agree to it being recorded or transcribed.",
                "Everything stays on this computer: the transcript and notes are saved in Kudu's folder, and the audio is never saved.",
                "To hear the other people, Kudu records what the computer plays as well as your microphone. You can turn that off in Settings.",
            ] {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.label(text("•", 13.0, Weight::Regular, pal.secondary));
                    wrapped(ui, text(line, 12.5, Weight::Regular, pal.secondary));
                });
                ui.add_space(8.0);
            }
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if button(ui, "Turn on", ButtonKind::Prominent).clicked() {
                    answer = Some(true);
                }
                if button(ui, "Not now", ButtonKind::Quiet).clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(yes) = answer {
            app.meetings.consent = false;
            if yes {
                app.data.update_settings(|s| s.meetings.enabled = true);
                app.send(Request::Reload);
            }
        }
    }
    if let Some(id) = app.meetings.confirm_delete.clone() {
        let mut answer = None;
        modal(ui, "meeting-delete", |ui, pal| {
            ui.label(text("Delete this meeting?", 15.0, Weight::Semibold, pal.text));
            ui.add_space(6.0);
            wrapped(ui, text("Its notes and transcript are deleted from this computer. This can't be undone.", 12.5, Weight::Regular, pal.secondary));
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if button(ui, "Delete", ButtonKind::Destructive).clicked() {
                    answer = Some(true);
                }
                if button(ui, "Cancel", ButtonKind::Quiet).clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(yes) = answer {
            app.meetings.confirm_delete = None;
            if yes {
                app.send(Request::DeleteMeeting { id });
                app.meetings.selected = None;
            }
        }
    }
    if app.meetings.confirm_delete_all {
        let mut answer = None;
        modal(ui, "meeting-delete-all", |ui, pal| {
            ui.label(text("Delete all meetings?", 15.0, Weight::Semibold, pal.text));
            ui.add_space(6.0);
            wrapped(ui, text("Every meeting's notes and transcript are deleted from this computer. This can't be undone.", 12.5, Weight::Regular, pal.secondary));
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if button(ui, "Delete all", ButtonKind::Destructive).clicked() {
                    answer = Some(true);
                }
                if button(ui, "Cancel", ButtonKind::Quiet).clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(yes) = answer {
            app.meetings.confirm_delete_all = false;
            if yes {
                app.send(Request::DeleteAllMeetings);
            }
        }
    }
}

fn modal(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui, &Palette)) {
    let pal = Palette::of(ui);
    let frame = Frame::new()
        .fill(pal.panel)
        .stroke(Stroke::new(1.0, pal.hairline))
        .corner_radius(14)
        .inner_margin(Margin::same(22))
        .shadow(ui.visuals().window_shadow);
    Modal::new(Id::new(id))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(if pal.dark { 120 } else { 60 }))
        .show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui, &pal);
        });
}

/// "TODAY" → "Today", "OCTOBER 17, 2026" → "October 17, 2026".
fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let lower = word.to_lowercase();
            let mut chars = lower.chars();
            chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn open_folder(path: &std::path::Path) {
    let program = if cfg!(windows) { "explorer" } else { "xdg-open" };
    if let Err(error) = std::process::Command::new(program).arg(path).spawn() {
        log::warn!("Couldn't open the folder: {error}");
    }
}
