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

use crate::ipc::{self, ModelStatus, Request};
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
    let jobs = worker::spawn(outcome_tx);
    let (learned_tx, learned_rx) = unbounded();
    let (open_tx, open_rx) = unbounded::<()>();
    let window = window::Window::new(paths.config.clone());
    let store = Store::load(paths);
    store.prune_history();

    let mut controller = Controller::new(platform.clone(), store, jobs.clone(), learned_tx, window.events());
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
    let speech_model = controller.store.settings.speech_model;
    if speech_model.is_installed(&models) {
        controller.speech = ModelStatus::Loading;
        let _ = jobs.send(Job::LoadSpeech { model: speech_model, models: models.clone() });
    }
    if let Some(smart) = controller.store.settings.smart_model.filter(|m| m.is_installed(&models)) {
        controller.smart = ModelStatus::Loading;
        let _ = jobs.send(Job::LoadSmart { model: smart, models: models.clone() });
    }
    controller.idle_status();
    controller.refresh_microphones();

    // First launch, or setup unfinished: show the window (unless started at login).
    if !background && (!controller.store.settings.onboarded || controller.speech == ModelStatus::Missing) {
        window.open(&controller.status());
    }

    let downloads = Downloads::new(platform_tx.clone());
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
                Ok(request) => handle_request(&mut controller, &downloads, &jobs, request),
                Err(_) => {}
            },
            recv(downloads.events()) -> event => if let Ok(event) = event {
                downloads.apply(event, &mut controller, &jobs);
            },
            recv(ticker) -> _ => {
                ticks += 1;
                controller.tick();
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

fn handle_request(controller: &mut Controller, downloads: &Downloads, jobs: &crossbeam_channel::Sender<Job>, request: Request) {
    match request {
        Request::Reload => {
            controller.store.reload();
            controller.settings_reloaded();
            let models = controller.store.paths.models.clone();
            // A newly chosen speech model that's already downloaded: switch to it.
            let wanted = controller.store.settings.speech_model;
            if wanted.is_installed(&models) && controller.speech != ModelStatus::Loading {
                let _ = jobs.send(Job::LoadSpeech { model: wanted, models });
                controller.speech = ModelStatus::Loading;
            }
            controller.send_status();
        }
        Request::RecordShortcut { slot } => controller.record_shortcut(slot),
        Request::CancelRecording => controller.cancel_shortcut_recording(),
        Request::DownloadSpeechModel { model } => {
            controller.store.settings.speech_model = model;
            controller.store.save_settings();
            if model.is_installed(&controller.store.paths.models) {
                controller.speech = ModelStatus::Loading;
                let _ = jobs.send(Job::LoadSpeech { model, models: controller.store.paths.models.clone() });
            } else {
                controller.speech = ModelStatus::Downloading { fraction: 0.0 };
                downloads.speech(model, controller.store.paths.models.clone());
            }
            controller.send_status();
        }
        Request::DownloadSmartModel { model } => {
            controller.store.settings.smart_model = Some(model);
            controller.store.save_settings();
            if model.is_installed(&controller.store.paths.models) {
                controller.smart = ModelStatus::Loading;
                let _ = jobs.send(Job::LoadSmart { model, models: controller.store.paths.models.clone() });
            } else {
                controller.smart = ModelStatus::Downloading { fraction: 0.0 };
                downloads.smart(model, controller.store.paths.models.clone());
            }
            controller.send_status();
        }
        Request::CancelDownload => downloads.cancel(),
        Request::RemoveSmartModel => {
            if let Some(model) = controller.store.settings.smart_model.take() {
                let _ = jobs.send(Job::UnloadSmart);
                let _ = kudu_engine::models::remove_smart_model(model, &controller.store.paths.models);
            }
            controller.store.save_settings();
            controller.smart = ModelStatus::Missing;
            controller.send_status();
        }
        Request::SetLaunchAtLogin { enabled } => {
            if let Err(error) = controller.platform.set_launch_at_login(enabled) {
                log::error!("Couldn't change launch at login: {error:#}");
            }
            controller.send_status();
        }
        Request::PasteLast => controller.paste_last(),
        Request::Quit => {}
    }
}

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
    SmartDone(kudu_engine::SmartModel, Result<(), String>),
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

    fn speech(&self, model: kudu_engine::SpeechModel, models: std::path::PathBuf) {
        self.cancelled.store(false, Ordering::SeqCst);
        let (tx, cancelled) = (self.events.0.clone(), self.cancelled.clone());
        std::thread::spawn(move || {
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
            let _ = tx.send(DownloadEvent::SpeechDone(model, result));
        });
    }

    fn smart(&self, model: kudu_engine::SmartModel, models: std::path::PathBuf) {
        self.cancelled.store(false, Ordering::SeqCst);
        let (tx, cancelled) = (self.events.0.clone(), self.cancelled.clone());
        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let result = guarded(|| {
                kudu_engine::models::install_smart_model(
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
        }
        controller.idle_status();
    }
}
