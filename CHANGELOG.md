# Changelog

All notable changes to Kudu are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com) and the project uses [Semantic Versioning](https://semver.org).

## [0.2.0] - 2026-10-14
### Added
- Cleanup pipeline:
  - filler removal;
  - spoken self-corrections;
  - "new line" and "new paragraph";
  - clock times written as 10:30.
- Personal dictionary and snippets, with `{date}`, `{time}` and `{clipboard}` placeholders.
- Settings window for cleanup options, dictionary and snippets.
- Hands-free recording: double-tap the dictation key, tap again to finish.
- In-memory history with copy and copy-raw.
- Start, stop and cancel sounds; open at login; warning when the microphone is silent.
- Paste key that follows the current keyboard layout.
- CI, contributing guide, privacy statement, architecture notes and roadmap.

## [0.1.0] - 2026-10-14
### Added
- Hold-to-dictate menu-bar app with on-device Parakeet speech recognition.
- Accessibility insertion with clipboard-paste fallback and clipboard restore.
- Floating level-meter overlay, first-run onboarding, `kudu-bench` latency harness.
