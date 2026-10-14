//! Starts the settings window (`kudu --window`) as a child process on demand and talks to it
//! over its stdin/stdout. Closing the window ends that process; Kudu keeps running.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;

use crate::ipc::{self, Event, Request, Status};

pub struct Window {
    child: Arc<Mutex<Option<(Child, ChildStdin)>>>,
    requests: (Sender<Request>, Receiver<Request>),
    events: Sender<Event>,
}

impl Window {
    pub fn new(_config: PathBuf) -> Self {
        let child: Arc<Mutex<Option<(Child, ChildStdin)>>> = Arc::new(Mutex::new(None));
        let (events, events_rx) = unbounded::<Event>();
        // Events are written on their own thread so a busy window never blocks the controller.
        let writer = child.clone();
        std::thread::Builder::new()
            .name("kudu-window-out".into())
            .spawn(move || {
                for event in events_rx {
                    let mut guard = writer.lock();
                    let failed = match guard.as_mut() {
                        Some((_, stdin)) => ipc::send(stdin, &event).is_err(),
                        None => false,
                    };
                    if failed {
                        *guard = None; // the window was closed
                    }
                }
            })
            .ok();
        Self { child, requests: unbounded(), events }
    }

    /// Where the window's requests arrive.
    pub fn requests(&self) -> &Receiver<Request> {
        &self.requests.1
    }

    /// A sender for events to the window (dropped silently while it's closed).
    pub fn events(&self) -> Sender<Event> {
        self.events.clone()
    }

    pub fn send(&self, event: Event) {
        let _ = self.events.send(event);
    }

    /// Opens the window, or brings it to the front if it's open.
    pub fn open(&self, status: &Status) {
        let mut guard = self.child.lock();
        if let Some((child, _)) = guard.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                drop(guard);
                self.send(Event::Focus);
                return;
            }
        }
        let exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(error) => return log::error!("Can't find Kudu's executable: {error}"),
        };
        let spawned = Command::new(exe)
            .arg("--window")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => return log::error!("Couldn't open the window: {error}"),
        };
        let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else { return };
        let _ = ipc::send(&mut stdin, &Event::Status(status.clone()));
        let _ = stdin.flush();
        let requests = self.requests.0.clone();
        std::thread::Builder::new()
            .name("kudu-window-in".into())
            .spawn(move || ipc::receive::<Request>(stdout, |request| {
                let _ = requests.send(request);
            }))
            .ok();
        *guard = Some((child, stdin));
    }

    pub fn close(&self) {
        if let Some((mut child, stdin)) = self.child.lock().take() {
            drop(stdin); // the window closes when its input ends
            let _ = child.wait();
        }
    }
}
