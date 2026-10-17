//! Which meeting apps are using the microphone, from the PulseAudio / PipeWire list of
//! recording streams (`pactl list source-outputs`).

use std::process::Command;

use kudu_core::meeting_apps::{app_for_program, MeetingApp};

/// `None` when `pactl` isn't available (no PulseAudio or PipeWire-Pulse).
pub fn apps_using_microphone() -> Option<Vec<MeetingApp>> {
    let output = Command::new("pactl").args(["list", "source-outputs"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse(&String::from_utf8_lossy(&output.stdout)))
}

/// The meeting apps named in `pactl list source-outputs`. Kudu's own streams (its microphone
/// and the `parec` monitor) aren't meeting apps, so they never count. Monitor sources (an app
/// recording what's played) are skipped too.
fn parse(listing: &str) -> Vec<MeetingApp> {
    listing
        .split("Source Output #")
        .filter(|stream| !stream.contains(".monitor"))
        .filter_map(|stream| {
            let value = |key: &str| {
                stream.lines().find_map(|line| {
                    let line = line.trim();
                    line.strip_prefix(key)?.trim().strip_prefix('=')?.trim().strip_prefix('"')?.strip_suffix('"').map(str::to_string)
                })
            };
            value("application.process.binary").and_then(|b| app_for_program(&b)).or_else(|| value("application.name").and_then(|n| app_for_program(&n)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_meeting_apps_recording() {
        let listing = r#"Source Output #12
	Driver: protocol-native.c
	Source: 1
	Properties:
		application.name = "ZOOM VoiceEngine"
		application.process.binary = "zoom"
Source Output #13
	Source: 2
	Properties:
		media.name = "Kudu meeting notes"
		application.process.binary = "parec"
		device.description = "Monitor of Speakers"
		node.target = "alsa_output.pci.analog-stereo.monitor"
"#;
        let apps = parse(listing);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Zoom");
    }
}
