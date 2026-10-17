//! Meetings on disk, shared by the background process (which writes them) and the window
//! (which shows them): `meetings/<id>/meeting.json` and `transcript.jsonl` in Kudu's data
//! folder, the same layout as the macOS app. Kept until the user deletes them.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use kudu_core::meeting::{Meeting, MeetingSegment};

use crate::paths::write_json;

pub fn dir(data: &Path) -> PathBuf {
    data.join("meetings")
}

fn folder(data: &Path, id: &str) -> PathBuf {
    dir(data).join(id)
}

/// Saves a meeting's details (creating its folder the first time).
pub fn save(data: &Path, meeting: &Meeting) {
    let folder = folder(data, &meeting.id);
    let result = fs::create_dir_all(&folder).map_err(anyhow::Error::from).and_then(|()| write_json(&folder.join("meeting.json"), meeting));
    if let Err(error) = result {
        log::error!("Couldn't save a meeting: {error:#}");
    }
}

/// Adds transcribed segments to a meeting's transcript file.
pub fn append(data: &Path, id: &str, segments: &[MeetingSegment]) {
    if segments.is_empty() {
        return;
    }
    let mut lines = Vec::new();
    for segment in segments {
        if let Ok(mut line) = serde_json::to_vec(segment) {
            line.push(b'\n');
            lines.extend(line);
        }
    }
    let result = fs::create_dir_all(folder(data, id))
        .and_then(|()| fs::OpenOptions::new().create(true).append(true).open(folder(data, id).join("transcript.jsonl")))
        .and_then(|mut file| file.write_all(&lines));
    if let Err(error) = result {
        log::error!("Couldn't save the meeting transcript: {error}");
    }
}

pub fn segments(data: &Path, id: &str) -> Vec<MeetingSegment> {
    fs::read_to_string(folder(data, id).join("transcript.jsonl"))
        .map(|text| text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect())
        .unwrap_or_default()
}

pub fn load(data: &Path, id: &str) -> Option<Meeting> {
    fs::read(folder(data, id).join("meeting.json")).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// Every saved meeting, newest first.
pub fn list(data: &Path) -> Vec<Meeting> {
    let mut meetings: Vec<Meeting> = fs::read_dir(dir(data))
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| fs::read(entry.path().join("meeting.json")).ok())
                .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
                .collect()
        })
        .unwrap_or_default();
    meetings.sort_by_key(|m| std::cmp::Reverse(m.started));
    meetings
}

pub fn delete(data: &Path, id: &str) {
    // An id is a UUID; never let a damaged file point outside the meetings folder.
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return;
    }
    if let Err(error) = fs::remove_dir_all(folder(data, id)) {
        log::warn!("Couldn't delete a meeting: {error}");
    }
}
