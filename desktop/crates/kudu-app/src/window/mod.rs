//! The main window: a quiet sidebar on the window background, and the current page on a
//! rounded panel. It runs as its own process (`kudu --window`), started by the background
//! process, and talks to it over stdin (events in) and stdout (requests out), one JSON message
//! per line (see `ipc`). Files are read when they change, never per frame.

mod data;
mod icons;
mod pages;
mod theme;
mod widgets;

use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{
    self, pos2, vec2, Color32, CursorIcon, Rect, ScrollArea, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    Vec2, ViewportCommand,
};
use kudu_core::shortcut::Notation;
use kudu_engine::Pack;

use crate::ipc::{self, Event, ModelStatus, Request, ShortcutSlot, Status};
use crate::paths::{modified, Paths};
use data::Data;
use icons::Icon;
use theme::{font, Palette, Weight};
use widgets::{text, ButtonKind};

/// Opens the window and runs until it's closed or the background process goes away.
pub fn run(paths: Paths) -> anyhow::Result<()> {
    let data = Data::load(paths);
    let watched = data.watched();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Kudu")
        .with_app_id("kudu")
        .with_inner_size([1080.0, 720.0])
        .with_min_inner_size([860.0, 560.0]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native(
        "Kudu",
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::install_style(&cc.egui_ctx);
            let (sender, receiver) = crossbeam_channel::unbounded();
            read_events(sender.clone(), cc.egui_ctx.clone());
            watch_files(watched, sender, cc.egui_ctx.clone());
            Ok(Box::new(App::new(data, receiver)))
        }),
    )
    .map_err(|error| anyhow!("Couldn't open the window: {error}"))
}

fn app_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../assets/app-icon.png");
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?;
    let image = image.resize_exact(256, 256, image::imageops::FilterType::Lanczos3).to_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData { rgba: image.into_raw(), width, height })
}

enum Message {
    Event(Event),
    /// The background process closed our stdin: it's gone, so the window goes too.
    Closed,
    /// A file changed on disk.
    FilesChanged,
}

fn read_events(sender: Sender<Message>, ctx: egui::Context) {
    let spawned = std::thread::Builder::new().name("kudu-window-events".into()).spawn(move || {
        ipc::receive::<Event>(std::io::stdin().lock(), |event| {
            let _ = sender.send(Message::Event(event));
            ctx.request_repaint();
        });
        let _ = sender.send(Message::Closed);
        ctx.request_repaint();
    });
    if let Err(error) = spawned {
        log::error!("Couldn't read events: {error}");
    }
}

/// Polls the files' modification times once a second (a fallback for missed `DataChanged`).
fn watch_files(paths: Vec<std::path::PathBuf>, sender: Sender<Message>, ctx: egui::Context) {
    let _ = std::thread::Builder::new().name("kudu-window-files".into()).spawn(move || {
        let stamp = || paths.iter().map(|p| modified(p)).collect::<Vec<_>>();
        let mut last = stamp();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let now = stamp();
            if now != last {
                last = now;
                if sender.send(Message::FilesChanged).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Home,
    Insights,
    Dictionary,
    Snippets,
    Style,
    Settings,
}

impl Page {
    const ALL: [Page; 6] = [Page::Home, Page::Insights, Page::Dictionary, Page::Snippets, Page::Style, Page::Settings];

    fn title(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Insights => "Insights",
            Page::Dictionary => "Dictionary",
            Page::Snippets => "Snippets",
            Page::Style => "Style",
            Page::Settings => "Settings",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::Home => Icon::Home,
            Page::Insights => Icon::Insights,
            Page::Dictionary => Icon::Dictionary,
            Page::Snippets => Icon::Snippets,
            Page::Style => Icon::Style,
            Page::Settings => Icon::Settings,
        }
    }
}

pub struct App {
    pub data: Data,
    /// `None` until the background process reports.
    pub status: Option<Status>,
    pub page: Page,
    receiver: Receiver<Message>,
    pub notation: Notation,
    pub first_name: String,
    /// The shortcut being recorded, the keys held so far, and why the last one was rejected.
    pub recording: Option<ShortcutSlot>,
    pub preview: Vec<String>,
    pub shortcut_error: Option<String>,
    /// The pack chosen on the setup card.
    pub setup_choice: Pack,
    pub home: pages::home::HomeState,
    pub dictionary: pages::lists::ListState,
    pub snippets: pages::lists::ListState,
    pub confirm_clear: bool,
}

impl App {
    fn new(data: Data, receiver: Receiver<Message>) -> Self {
        let setup_choice = data.settings.pack();
        Self {
            data,
            status: None,
            page: Page::Home,
            receiver,
            notation: if cfg!(windows) { Notation::Windows } else { Notation::Linux },
            first_name: data::first_name(),
            recording: None,
            preview: Vec::new(),
            shortcut_error: None,
            setup_choice,
            home: Default::default(),
            dictionary: Default::default(),
            snippets: Default::default(),
            confirm_clear: false,
        }
    }

    /// Sends a request to the background process.
    pub fn send(&self, request: Request) {
        let mut out = std::io::stdout().lock();
        if let Err(error) = ipc::send(&mut out, &request) {
            log::warn!("Couldn't reach the background process: {error}");
        }
    }

    fn handle_messages(&mut self, ctx: &egui::Context) {
        for message in self.receiver.try_iter().collect::<Vec<_>>() {
            match message {
                Message::Event(Event::Status(status)) => {
                    let models_changed = self
                        .status
                        .as_ref()
                        .is_none_or(|old| old.speech != status.speech || old.smart != status.smart);
                    self.status = Some(status);
                    if models_changed {
                        self.data.check_models();
                    }
                }
                Message::Event(Event::RecordingPreview { keys }) => self.preview = keys,
                Message::Event(Event::Recorded { slot, shortcut, error }) => {
                    if self.recording == Some(slot) {
                        self.recording = None;
                    }
                    self.preview.clear();
                    self.shortcut_error = error;
                    if let (Some(shortcut), None) = (shortcut, &self.shortcut_error) {
                        // Already saved by the background process; show it right away.
                        let mut shortcuts = self.data.shortcuts();
                        match slot {
                            ShortcutSlot::Dictate => shortcuts.dictate = shortcut,
                            ShortcutSlot::Command => shortcuts.command = Some(shortcut),
                            ShortcutSlot::PasteLast => shortcuts.paste_last = Some(shortcut),
                        }
                        self.data.settings.shortcuts = Some(shortcuts);
                    }
                    self.data.reload_changed();
                }
                Message::Event(Event::DataChanged) => self.data.reload_data(),
                Message::Event(Event::Focus) => {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                Message::Closed => ctx.send_viewport_cmd(ViewportCommand::Close),
                Message::FilesChanged => {
                    self.data.reload_changed();
                }
            }
        }
    }

    /// The chosen pack's state: its speech model and writer together, with one progress
    /// fraction weighted by download size.
    pub fn pack_status(&self) -> Option<ModelStatus> {
        let status = self.status.as_ref()?;
        Some(combined_status(&status.speech, &status.smart, self.data.settings.pack()))
    }

    /// Whether setup is finished: dictation works and the pack's writer is in place too.
    pub fn setup_complete(&self) -> bool {
        self.is_ready()
            && (!WRITER || self.status.as_ref().is_some_and(|s| matches!(s.smart, ModelStatus::Ready | ModelStatus::Loading)))
    }

    /// Whether everything needed to dictate is in place.
    pub fn is_ready(&self) -> bool {
        self.status.as_ref().is_some_and(|s| {
            matches!(s.speech, ModelStatus::Ready | ModelStatus::Loading) && s.hotkeys_active && s.issues.is_empty()
        })
    }

    /// The dictation shortcut as text: "Ctrl + Win".
    pub fn dictate_name(&self) -> String {
        self.data.shortcuts().dictate.display(self.notation)
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        let pal = Palette::of(ui);
        let rect = ui.max_rect();
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let (mark, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            icons::kudu_mark(ui.painter(), mark, pal.text);
            ui.add_space(1.0);
            ui.label(text("Kudu", 19.0, Weight::Semibold, pal.text));
        });
        ui.add_space(18.0);
        for page in Page::ALL {
            let selected = self.page == page;
            let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
            let response = response.on_hover_cursor(CursorIcon::PointingHand);
            if selected {
                ui.painter().rect_filled(row, 9.0, pal.selection);
            } else if response.hovered() {
                ui.painter().rect_filled(row, 9.0, Palette::alpha(pal.selection, 0.55));
            }
            let icon_color = if selected { pal.text } else { pal.secondary };
            icons::paint(
                ui.painter(),
                Rect::from_center_size(pos2(row.left() + 20.0, row.center().y), Vec2::splat(15.0)),
                page.icon(),
                icon_color,
            );
            let weight = if selected { Weight::Semibold } else { Weight::Regular };
            ui.painter().text(
                pos2(row.left() + 40.0, row.center().y),
                egui::Align2::LEFT_CENTER,
                page.title(),
                font(13.5, weight),
                pal.text,
            );
            if response.clicked() {
                self.page = page;
            }
            ui.add_space(2.0);
        }

        // Status at the bottom.
        let (color, message) = self.status_line(&pal);
        let ready = self.is_ready() || self.status.is_none();
        let galley_width = rect.width() - 24.0 - 19.0 - if ready { 0.0 } else { 62.0 };
        let galley = ui.painter().layout(message, font(11.5, Weight::Regular), pal.secondary, galley_width.max(40.0));
        let height = galley.size().y.max(26.0) + 20.0;
        let card = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 14.0 - height), pos2(rect.right(), rect.bottom() - 14.0));
        ui.painter().rect_filled(card, 10.0, Palette::alpha(pal.selection, 0.6));
        ui.painter().circle_filled(pos2(card.left() + 15.5, card.center().y), 3.5, color);
        ui.painter().galley(pos2(card.left() + 27.0, card.center().y - galley.size().y / 2.0), galley, pal.secondary);
        if !ready {
            let button = Rect::from_min_max(pos2(card.right() - 66.0, card.top()), pos2(card.right() - 8.0, card.bottom()));
            let mut child = ui.new_child(UiBuilder::new().max_rect(button).layout(egui::Layout::right_to_left(egui::Align::Center)));
            if widgets::button(&mut child, "Set up", ButtonKind::Quiet).clicked() {
                self.page = Page::Home;
            }
        }
    }

    fn status_line(&self, pal: &Palette) -> (Color32, String) {
        let Some(status) = &self.status else {
            return (pal.tertiary, "Starting Kudu…".into());
        };
        if status.recording {
            return (pal.accent, "Listening…".into());
        }
        if let Some(issue) = status.issues.first() {
            return (pal.orange, issue.title.clone());
        }
        match &status.speech {
            ModelStatus::Ready if status.hotkeys_active => (pal.green, format!("Ready · hold {}", self.dictate_name())),
            ModelStatus::Ready => (pal.orange, "Shortcuts aren't active yet".into()),
            ModelStatus::Downloading { fraction } => {
                (pal.orange, format!("Downloading model {}%", (fraction * 100.0).round() as i32))
            }
            ModelStatus::Loading => (pal.orange, "Loading speech model…".into()),
            ModelStatus::Missing => (pal.orange, "Speech model not downloaded".into()),
            ModelStatus::Failed { .. } => (pal.red, "Speech model error".into()),
        }
    }

    fn content(&mut self, ui: &mut Ui) {
        let (max_width, top) = match self.page {
            Page::Home => (980.0, 34.0),
            Page::Insights => (900.0, 42.0),
            _ => (820.0, 42.0),
        };
        ScrollArea::vertical()
            .id_salt(self.page)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                let available = ui.available_width();
                let width = (available - 80.0).min(max_width).max(200.0);
                let left = ui.max_rect().left() + (available - width) / 2.0;
                ui.add_space(top);
                let start = pos2(left, ui.cursor().top());
                let column = Rect::from_min_size(start, vec2(width, f32::INFINITY));
                ui.scope_builder(UiBuilder::new().max_rect(column), |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    match self.page {
                        Page::Home => pages::home::show(self, ui),
                        Page::Insights => pages::insights::show(self, ui),
                        Page::Dictionary => pages::lists::dictionary(self, ui),
                        Page::Snippets => pages::lists::snippets(self, ui),
                        Page::Style => pages::style::show(self, ui),
                        Page::Settings => pages::settings::show(self, ui),
                    }
                });
                ui.add_space(top);
            });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_messages(&ctx);
        let pal = Palette::of(ui);
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, pal.canvas);

        let sidebar = Rect::from_min_max(full.min + vec2(12.0, 0.0), pos2(full.left() + 228.0 - 12.0, full.bottom()));
        ui.scope_builder(UiBuilder::new().max_rect(sidebar).id_salt("sidebar"), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            self.sidebar(ui);
        });

        let panel = Rect::from_min_max(pos2(full.left() + 228.0, full.top() + 10.0), full.max - vec2(10.0, 10.0));
        ui.painter().rect(panel, 14.0, pal.panel, Stroke::new(1.0, pal.hairline), StrokeKind::Inside);
        let mut child = ui.new_child(UiBuilder::new().max_rect(panel.shrink(1.0)).id_salt("panel"));
        child.set_clip_rect(panel.shrink(1.0));
        self.content(&mut child);

        pages::settings::confirm_dialog(self, ui);
    }
}

/// Whether this build runs a writer model; without llama.cpp, Kudu uses the rules only.
pub const WRITER: bool = cfg!(feature = "smart");

/// One status for a pack's two downloads: a failure or a download in progress shows first,
/// and the progress counts both files by size.
pub fn combined_status(speech: &ModelStatus, writer: &ModelStatus, pack: Pack) -> ModelStatus {
    let writer = if WRITER { writer } else { &ModelStatus::Ready };
    for status in [speech, writer] {
        if let ModelStatus::Failed { message } = status {
            return ModelStatus::Failed { message: message.clone() };
        }
    }
    let done = |status: &ModelStatus| match status {
        ModelStatus::Downloading { fraction } => *fraction,
        ModelStatus::Ready | ModelStatus::Loading => 1.0,
        _ => 0.0,
    };
    if matches!(speech, ModelStatus::Downloading { .. }) || matches!(writer, ModelStatus::Downloading { .. }) {
        let (a, b) = (pack.speech().component().bytes as f64, pack.writer().component().bytes as f64);
        return ModelStatus::Downloading { fraction: (done(speech) * a + done(writer) * b) / (a + b) };
    }
    if matches!(speech, ModelStatus::Missing) || matches!(writer, ModelStatus::Missing) {
        return ModelStatus::Missing;
    }
    if matches!(speech, ModelStatus::Loading) || matches!(writer, ModelStatus::Loading) {
        return ModelStatus::Loading;
    }
    ModelStatus::Ready
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_downloads_as_one() {
        use ModelStatus::*;
        let pack = Pack::Base;
        // Speech done, writer halfway: well past half, as the speech model is the bigger file.
        let Downloading { fraction } = combined_status(&Ready, &Downloading { fraction: 0.5 }, pack) else { panic!() };
        assert!(fraction > 0.75 && fraction < 0.9, "{fraction}");
        assert_eq!(combined_status(&Ready, &Ready, pack), Ready);
        assert_eq!(combined_status(&Loading, &Ready, pack), Loading);
        let failed = Failed { message: "x".into() };
        assert_eq!(combined_status(&Ready, &failed, pack), if WRITER { failed.clone() } else { Ready });
        assert_eq!(combined_status(&Missing, &Missing, pack), Missing);
    }
}
