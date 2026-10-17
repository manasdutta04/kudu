//! The background process: shortcuts, microphone, speech model, overlay, tray. It starts the
//! settings window as a child process when the user opens it.

mod controller;
mod learning;
mod levels;
mod store;
mod vocabulary;
mod window;
mod worker;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossbeam_channel::{select, tick, unbounded};

use crate::ipc::{self, ModelStatus, Request, UpdateState};
use crate::paths::Paths;
use crate::platform::{self, PlatformEvent, TrayAction};
use controller::Controller;
use store::Store;
use worker::Job;

pub fn run(args: &[String], paths: Paths) -> Result<()> {
    let background = args.iter().any(|a| a == "--background");

    // One Kudu at a time: a second launch just opens the running one's window.
    let Some(instance) = ipc::claim_instance() else {
        if ipc::ask_running_instance_to_open() {
            return Ok(());
        }
        anyhow::bail!("Kudu is already running");
    };

    let (platform_tx, platform_rx) = unbounded::<PlatformEvent>();
    let platform = platform::current(platform_tx.clone())?;
    let (outcome_tx, outcome_rx) = unbounded();
    let (jobs, meeting_jobs) = worker::spawn(outcome_tx);
    let (learned_tx, learned_rx) = unbounded();
    let (open_tx, open_rx) = unbounded::<()>();
    let window = window::Window::new(paths.config.clone());
    let store = Store::load(paths);
    store.prune_history();

    let mut controller = Controller::new(platform.clone(), store, jobs.clone(), meeting_jobs, learned_tx, window.events());
    ipc::serve_instance(instance, move || {
        let _ = open_tx.send(());
    });

    controller.hotkeys_active = match platform.start_hotkeys(controller.matcher.clone(), platform_tx.clone()) {
        Ok(()) => true,
        Err(error) => {
            log::error!("Couldn't listen for shortcuts: {error:#}");
            false
        }
    };
    if let Err(error) = platform.start_tray(platform_tx.clone()) {
        log::warn!("No tray icon: {error:#}");
    }

    let models = controller.store.paths.models.clone();
    // Settle on a pack (settings from older versions name a speech model and an optional
    // language model instead), and delete the general-purpose models Kudu's writers replace.
    let pack = controller.store.settings.pack();
    if controller.store.settings.pack != Some(pack) {
        controller.store.settings.pack = Some(pack);
        controller.store.save_settings();
    }
    let removed = kudu_engine::models::remove_legacy_models(&models);
    if removed > 0 {
        log::info!("Removed {removed} old language model file(s)");
    }
    let speech_model = pack.speech();
    if speech_model.is_installed(&models) {
        controller.speech = ModelStatus::Loading;
        let _ = jobs.send(Job::LoadSpeech { model: speech_model, models: models.clone() });
    }
    let writer = pack.writer();
    if WRITER && writer.is_installed(&models) {
        controller.smart = ModelStatus::Loading;
        let _ = jobs.send(Job::LoadSmart { model: writer, models: models.clone() });
    }
    let downloads = Downloads::new(platform_tx.clone());
    // Updated from a version without the writer: fetch it now (dictation works meanwhile).
    if WRITER && speech_model.is_installed(&models) && !writer.is_installed(&models) {
        controller.smart = ModelStatus::Downloading { fraction: 0.0 };
        downloads.pack(None, Some(writer), models.clone());
    }
    controller.idle_status();
    controller.refresh_microphones();

    // First launch, or setup unfinished: show the window (unless started at login).
    if !background && (!controller.store.settings.onboarded || controller.speech == ModelStatus::Missing) {
        window.open(&controller.status());
    }

    let updates = Updates::new();
    let ticker = tick(Duration::from_millis(250));
    let mut ticks: u64 = 0;
    loop {
        select! {
            recv(platform_rx) -> event => match event {
                Ok(PlatformEvent::Hotkey(event)) => controller.hotkey(event),
                Ok(PlatformEvent::Recorded(result)) => controller.recorded(result),
                Ok(PlatformEvent::RecordingPreview(keys)) => window.send(ipc::Event::RecordingPreview { keys }),
                Ok(PlatformEvent::UndoLearned) => controller.undo_learned(),
                Ok(PlatformEvent::Tray(TrayAction::OpenWindow)) => {
                    window.open(&controller.status());
                    controller.refresh_microphones();
                }
                Ok(PlatformEvent::Tray(TrayAction::PasteLast)) => controller.paste_last(),
                Ok(PlatformEvent::Tray(TrayAction::Meeting)) => controller.toggle_meeting(),
                Ok(PlatformEvent::Tray(TrayAction::Quit)) => break,
                Err(_) => break,
            },
            recv(outcome_rx) -> outcome => if let Ok(outcome) = outcome { controller.worker_outcome(outcome) },
            recv(learned_rx) -> learned => if let Ok(learned) = learned { controller.learned(learned) },
            recv(open_rx) -> _ => {
                window.open(&controller.status());
                controller.refresh_microphones();
            }
            recv(window.requests()) -> request => match request {
                Ok(Request::Quit) => break,
                Ok(request) => handle_request(&mut controller, &downloads, &updates, &jobs, request),
                Err(_) => {}
            },
            recv(downloads.events()) -> event => if let Ok(event) = event {
                downloads.apply(event, &mut controller, &jobs);
            },
            recv(updates.events()) -> event => if let Ok(event) = event {
                updates.apply(event, &mut controller);
            },
            recv(ticker) -> _ => {
                ticks += 1;
                controller.tick();
                // The daily update check (first one a few seconds after starting).
                if ticks >= 20 && controller.store.settings.check_updates && updates.due() {
                    updates.check(&mut controller);
                }
                // The window may have changed settings without telling us (or crashed).
                if ticks.is_multiple_of(4) && !controller.is_recording() && controller.store.reload_if_changed() {
                    controller.settings_reloaded();
                }
            },
        }
    }
    downloads.cancel();
    window.close();
    log::info!("Kudu quit");
    Ok(())
}

fn handle_request(
    controller: &mut Controller,
    downloads: &Downloads,
    updates: &Updates,
    jobs: &crossbeam_channel::Sender<Job>,
    request: Request,
) {
    match request {
        Request::Reload => {
            controller.store.reload();
            controller.settings_reloaded();
            let models = controller.store.paths.models.clone();
            // A newly chosen speech model that's already downloaded: switch to it.
            let wanted = controller.store.settings.speech();
            if wanted.is_installed(&models) && controller.speech != ModelStatus::Loading {
                let _ = jobs.send(Job::LoadSpeech { model: wanted, models });
                controller.speech = ModelStatus::Loading;
            }
            controller.send_status();
        }
        Request::RecordShortcut { slot } => controller.record_shortcut(slot),
        Request::CancelRecording => controller.cancel_shortcut_recording(),
        Request::DownloadPack { pack } => {
            let previous = controller.store.settings.pack();
            controller.store.settings.pack = Some(pack);
            controller.store.save_settings();
            let models = controller.store.paths.models.clone();
            let (speech, writer) = (pack.speech(), pack.writer());
            let mut fetch_speech = None;
            if !speech.is_installed(&models) {
                controller.speech = ModelStatus::Downloading { fraction: 0.0 };
                fetch_speech = Some(speech);
            } else if speech != previous.speech() || !matches!(controller.speech, ModelStatus::Ready | ModelStatus::Loading) {
                controller.speech = ModelStatus::Loading;
                let _ = jobs.send(Job::LoadSpeech { model: speech, models: models.clone() });
            }
            let mut fetch_writer = None;
            if WRITER {
                if !writer.is_installed(&models) {
                    controller.smart = ModelStatus::Downloading { fraction: 0.0 };
                    fetch_writer = Some(writer);
                } else if writer != previous.writer() || !matches!(controller.smart, ModelStatus::Ready | ModelStatus::Loading) {
                    controller.smart = ModelStatus::Loading;
                    let _ = jobs.send(Job::LoadSmart { model: writer, models: models.clone() });
                }
            }
            if fetch_speech.is_some() || fetch_writer.is_some() {
                downloads.pack(fetch_speech, fetch_writer, models);
            }
            controller.send_status();
        }
        Request::CancelDownload => downloads.cancel(),
        Request::CheckForUpdates => updates.check(controller),
        Request::InstallUpdate => updates.install(controller),
        Request::RemoveUnusedModels => {
            let pack = controller.store.settings.pack();
            if let Err(error) = kudu_engine::models::remove_unused(pack, &controller.store.paths.models) {
                log::error!("Couldn't remove unused models: {error:#}");
            }
            controller.send_status();
        }
        Request::SetLaunchAtLogin { enabled } => {
            if let Err(error) = controller.platform.set_launch_at_login(enabled) {
                log::error!("Couldn't change launch at login: {error:#}");
            }
            controller.send_status();
        }
        Request::PasteLast => controller.paste_last(),
        Request::StartMeeting => {
            if !controller.meeting_is_recording() {
                controller.toggle_meeting();
            }
        }
        Request::StopMeeting => controller.stop_meeting(),
        Request::RegenerateNotes { id } => controller.regenerate_notes(id),
        Request::DeleteMeeting { id } => controller.delete_meeting(&id),
        Request::DeleteAllMeetings => controller.delete_all_meetings(),
        Request::Quit => {}
    }
}

/// Checks GitHub once a day for a newer Kudu, and installs it when asked. Sends nothing but
/// the request for the latest release's address.
struct Updates {
    events: (crossbeam_channel::Sender<UpdateEvent>, crossbeam_channel::Receiver<UpdateEvent>),
    last_check: std::cell::Cell<Option<std::time::Instant>>,
}

enum UpdateEvent {
    Checked(Result<String, String>),
    InstallStarted(Result<(), String>),
}

impl Updates {
    const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
    const RELEASES: &'static str = "https://github.com/manasdutta04/kudu/releases/latest";

    fn new() -> Self {
        Self { events: unbounded(), last_check: std::cell::Cell::new(None) }
    }

    fn events(&self) -> &crossbeam_channel::Receiver<UpdateEvent> {
        &self.events.1
    }

    fn due(&self) -> bool {
        self.last_check.get().is_none_or(|last| last.elapsed() >= Self::INTERVAL)
    }

    fn check(&self, controller: &mut Controller) {
        if matches!(controller.update, UpdateState::Checking | UpdateState::Installing) {
            return;
        }
        self.last_check.set(Some(std::time::Instant::now()));
        controller.update = UpdateState::Checking;
        controller.send_status();
        let tx = self.events.0.clone();
        std::thread::spawn(move || {
            let result = guarded(|| kudu_engine::models::latest_release().map_err(|e| format!("{e:#}")));
            let _ = tx.send(UpdateEvent::Checked(result));
        });
    }

    /// Windows: downloads the new installer and runs it (it closes Kudu, updates it and offers
    /// to start it again). Linux: opens the download page, as the right package depends on how
    /// Kudu was installed.
    fn install(&self, controller: &mut Controller) {
        if !matches!(controller.update, UpdateState::Available { .. }) {
            return;
        }
        if cfg!(windows) {
            controller.update = UpdateState::Installing;
            controller.send_status();
            let tx = self.events.0.clone();
            std::thread::spawn(move || {
                let result = guarded(|| {
                    let name = "Kudu-Windows-x64-Setup.exe";
                    let dest = std::env::temp_dir().join(name);
                    let url = kudu_engine::models::latest_release_file(name);
                    kudu_engine::models::download_file(&url, &dest, None, &mut |_| {}, &|| false)
                        .map_err(|e| format!("{e:#}"))?;
                    std::process::Command::new(&dest).spawn().map_err(|e| format!("Couldn't start the installer: {e}"))?;
                    Ok(())
                });
                let _ = tx.send(UpdateEvent::InstallStarted(result));
            });
        } else if let Err(error) = std::process::Command::new("xdg-open").arg(Self::RELEASES).spawn() {
            log::error!("Couldn't open the download page: {error}");
        }
    }

    fn apply(&self, event: UpdateEvent, controller: &mut Controller) {
        controller.update = match event {
            UpdateEvent::Checked(Ok(latest)) if kudu_core::release_version::is_newer(&latest, env!("CARGO_PKG_VERSION")) => {
                UpdateState::Available { version: latest }
            }
            UpdateEvent::Checked(Ok(_)) => UpdateState::UpToDate,
            UpdateEvent::Checked(Err(message)) => {
                log::info!("Update check failed: {message}");
                UpdateState::Failed { message: "Couldn't reach GitHub".into() }
            }
            UpdateEvent::InstallStarted(Ok(())) => UpdateState::Installing,
            UpdateEvent::InstallStarted(Err(message)) => UpdateState::Failed { message },
        };
        controller.send_status();
    }
}

/// Whether this build runs a writer model (smart formatting and Command mode). On Windows and
/// Linux the writer is part of every pack; a build without llama.cpp uses the rules only.
const WRITER: bool = cfg!(feature = "smart");

/// Runs `work`, turning a crash inside it into an error message instead of losing it.
fn guarded<T>(work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".into());
        Err(format!("Something went wrong: {message}"))
    })
}

/// Model downloads, one at a time, on their own thread.
struct Downloads {
    events: (crossbeam_channel::Sender<DownloadEvent>, crossbeam_channel::Receiver<DownloadEvent>),
    cancelled: Arc<AtomicBool>,
    _platform: crossbeam_channel::Sender<PlatformEvent>,
}

enum DownloadEvent {
    SpeechProgress(f64),
    SpeechDone(kudu_engine::SpeechModel, Result<(), String>),
    SmartProgress(f64),
    SmartDone(kudu_engine::WriterModel, Result<(), String>),
    /// The writer wasn't downloaded because the speech model's download stopped first.
    SmartSkipped,
}

impl Downloads {
    fn new(platform: crossbeam_channel::Sender<PlatformEvent>) -> Self {
        Self { events: unbounded(), cancelled: Arc::new(AtomicBool::new(false)), _platform: platform }
    }

    fn events(&self) -> &crossbeam_channel::Receiver<DownloadEvent> {
        &self.events.1
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Downloads a pack's missing models on one thread: the speech model first (so dictation
    /// can start), then the writer.
    fn pack(&self, speech: Option<kudu_engine::SpeechModel>, writer: Option<kudu_engine::WriterModel>, models: std::path::PathBuf) {
        self.cancelled.store(false, Ordering::SeqCst);
        let (tx, cancelled) = (self.events.0.clone(), self.cancelled.clone());
        std::thread::spawn(move || {
            if let Some(model) = speech {
                let progress_tx = tx.clone();
                let result = guarded(|| {
                    kudu_engine::models::install_speech_model(
                        model,
                        &models,
                        &mut |f| {
                            let _ = progress_tx.send(DownloadEvent::SpeechProgress(f));
                        },
                        &|| cancelled.load(Ordering::SeqCst),
                    )
                    .map_err(|e| format!("{e:#}"))
                });
                let failed = result.is_err();
                let _ = tx.send(DownloadEvent::SpeechDone(model, result));
                if failed {
                    if writer.is_some() {
                        let _ = tx.send(DownloadEvent::SmartSkipped);
                    }
                    return;
                }
            }
            if let Some(model) = writer {
                let progress_tx = tx.clone();
                let result = guarded(|| {
                    kudu_engine::models::install_writer(
                        model,
                        &models,
                        &mut |f| {
                            let _ = progress_tx.send(DownloadEvent::SmartProgress(f));
                        },
                        &|| cancelled.load(Ordering::SeqCst),
                    )
                    .map_err(|e| format!("{e:#}"))
                });
                let _ = tx.send(DownloadEvent::SmartDone(model, result));
            }
        });
    }

    fn apply(&self, event: DownloadEvent, controller: &mut Controller, jobs: &crossbeam_channel::Sender<Job>) {
        let models = controller.store.paths.models.clone();
        match event {
            DownloadEvent::SpeechProgress(fraction) => {
                // Status updates at most every percent, to keep the window calm.
                if let ModelStatus::Downloading { fraction: old } = controller.speech {
                    if fraction - old < 0.01 && fraction < 1.0 {
                        return;
                    }
                }
                controller.speech = ModelStatus::Downloading { fraction };
            }
            DownloadEvent::SpeechDone(model, Ok(())) => {
                controller.speech = ModelStatus::Loading;
                controller.store.settings.onboarded = true;
                controller.store.save_settings();
                let _ = jobs.send(Job::LoadSpeech { model, models });
            }
            DownloadEvent::SpeechDone(_, Err(message)) => controller.speech = ModelStatus::Failed { message },
            DownloadEvent::SmartProgress(fraction) => {
                if let ModelStatus::Downloading { fraction: old } = controller.smart {
                    if fraction - old < 0.01 && fraction < 1.0 {
                        return;
                    }
                }
                controller.smart = ModelStatus::Downloading { fraction };
            }
            DownloadEvent::SmartDone(model, Ok(())) => {
                controller.smart = ModelStatus::Loading;
                let _ = jobs.send(Job::LoadSmart { model, models });
            }
            DownloadEvent::SmartDone(_, Err(message)) => controller.smart = ModelStatus::Failed { message },
            DownloadEvent::SmartSkipped => controller.smart = ModelStatus::Missing,
        }
        controller.idle_status();
    }
}
