//! Messages between the background process and the settings window.
//!
//! The background process starts the window as a child process (`kudu --window`) and talks to
//! it over the child's stdin/stdout, one JSON message per line. Settings, the dictionary and
//! snippets are files both processes read; a message tells the other side to reload them.
//!
//! A second launch of Kudu (from the Start menu or a launcher) finds the running background
//! process through a local TCP port and asks it to open the window, then exits.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::time::Duration;

use kudu_core::shortcut::Shortcut;
use serde::{Deserialize, Serialize};

/// A shortcut that can be changed in Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutSlot {
    Dictate,
    Command,
    PasteLast,
    Meeting,
}

/// Window → background process.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    /// A settings file changed: reload settings, cleanup, dictionary and snippets.
    Reload,
    RecordShortcut { slot: ShortcutSlot },
    CancelRecording,
    /// Choose a pack: download whatever of it isn't on disk yet (speech first, then the
    /// writer), and switch to it.
    DownloadPack { pack: kudu_engine::Pack },
    CancelDownload,
    /// Delete downloaded models the chosen pack doesn't use.
    RemoveUnusedModels,
    /// Ask GitHub now whether a newer Kudu is out.
    CheckForUpdates,
    /// Install the newer version (Windows: run its installer; Linux: open the download page).
    InstallUpdate,
    SetLaunchAtLogin { enabled: bool },
    PasteLast,
    /// Start or stop meeting notes.
    StartMeeting,
    StopMeeting,
    /// Write a saved meeting's notes again.
    RegenerateNotes { id: String },
    DeleteMeeting { id: String },
    DeleteAllMeetings,
    Quit,
}

/// What the background process is doing, for the window's status and setup card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub speech: ModelStatus,
    /// The writer model (smart formatting and Command mode).
    pub smart: ModelStatus,
    pub hotkeys_active: bool,
    pub recording: bool,
    pub launch_at_login: bool,
    pub microphones: Vec<String>,
    /// Problems the user must fix (permissions, missing packages).
    pub issues: Vec<Issue>,
    pub platform: String,
    /// Whether a newer Kudu is out.
    #[serde(default)]
    pub update: UpdateState,
    /// Meeting notes being taken or written.
    #[serde(default)]
    pub meeting: MeetingState,
    /// Whether this computer can tell when a meeting app uses the microphone.
    #[serde(default)]
    pub meeting_detection: bool,
}

/// Meeting notes, for the window.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum MeetingState {
    #[default]
    Idle,
    /// Recording since `started` (Unix seconds).
    #[serde(rename_all = "camelCase")]
    Recording { id: String, started: f64, app: Option<String>, system_audio: bool },
    /// Writing the notes: `done` of `total` steps.
    Writing { id: String, done: usize, total: usize },
}

/// The update check's result, for the window.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available { version: String },
    /// The installer is downloading or running; Kudu will be closed and updated.
    Installing,
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub title: String,
    pub detail: String,
    pub command: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ModelStatus {
    #[default]
    Missing,
    Downloading { fraction: f64 },
    Loading,
    Ready,
    Failed { message: String },
}

/// Background process → window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    Status(Status),
    /// Keys held so far while recording a shortcut.
    RecordingPreview { keys: Vec<String> },
    /// The recorded shortcut (already saved), or why it wasn't accepted.
    Recorded { slot: ShortcutSlot, shortcut: Option<Shortcut>, error: Option<String> },
    /// History, dictionary or usage changed (a dictation finished, a word was learned).
    DataChanged,
    /// Bring the window to the front.
    Focus,
}

/// Writes one message as a JSON line.
pub fn send<T: Serialize>(writer: &mut impl Write, message: &T) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()
}

/// Reads JSON-line messages until the stream closes, calling `handle` for each.
pub fn receive<T: for<'de> Deserialize<'de>>(reader: impl Read, mut handle: impl FnMut(T)) {
    for line in BufReader::new(reader).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(&line) {
            Ok(message) => handle(message),
            Err(error) => log::warn!("Ignoring a malformed message: {error}"),
        }
    }
}

/// Local port that marks a running Kudu (and takes "open the window" requests).
const INSTANCE_PORT: u16 = 47_391;

/// Becomes the single running instance, or `None` if one is already running.
pub fn claim_instance() -> Option<TcpListener> {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, INSTANCE_PORT)).ok()
}

/// Asks the running instance to open its window. Returns false if none answered.
pub fn ask_running_instance_to_open() -> bool {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, INSTANCE_PORT).into();
    match TcpStream::connect_timeout(&address, Duration::from_millis(500)) {
        Ok(mut stream) => stream.write_all(b"open\n").is_ok(),
        Err(_) => false,
    }
}

/// Serves "open" requests from later launches, calling `open` for each.
pub fn serve_instance(listener: TcpListener, open: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("kudu-instance".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let mut line = String::new();
                if BufReader::new(stream).read_line(&mut line).is_ok() && line.trim() == "open" {
                    open();
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip_as_json_lines() {
        let mut buffer = Vec::new();
        send(&mut buffer, &Request::RecordShortcut { slot: ShortcutSlot::Command }).unwrap();
        send(&mut buffer, &Request::Reload).unwrap();
        send(&mut buffer, &Request::DownloadPack { pack: kudu_engine::Pack::Large }).unwrap();
        let mut received = Vec::new();
        receive::<Request>(&buffer[..], |m| received.push(m));
        assert_eq!(
            received,
            vec![
                Request::RecordShortcut { slot: ShortcutSlot::Command },
                Request::Reload,
                Request::DownloadPack { pack: kudu_engine::Pack::Large }
            ]
        );

        let mut buffer = Vec::new();
        send(&mut buffer, &Event::Status(Status { speech: ModelStatus::Downloading { fraction: 0.5 }, ..Default::default() })).unwrap();
        let mut events = Vec::new();
        receive::<Event>(&buffer[..], |m| events.push(m));
        assert!(matches!(&events[0], Event::Status(s) if s.speech == ModelStatus::Downloading { fraction: 0.5 }));
    }
}
