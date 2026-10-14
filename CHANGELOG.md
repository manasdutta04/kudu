# Changelog

All notable changes to Kudu are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com) and the project uses [Semantic Versioning](https://semver.org).

## [0.3.0] - 2026-10-14
### Added
- Kudu is now a desktop app:
  - A Dock icon with the Kudu logo, and a main window with Home (welcome, total words,
    words per minute, day streak, searchable history by day), Dictionary, Snippets, Style and
    Settings pages.
  - An animated menu-bar icon whose bars follow your voice.
- Smart formatting with Apple's on-device model (macOS 26+): punctuation, sentence and
  paragraph breaks, numbered lists and written numbers, for dictations of 12+ words. A guard
  rejects any output that changes your words.
- On-device dictation history with retention (30 days / 90 days / forever) and clear.
- `kudu-bench --format` and `--text`; `--overlay-demo`.

### Changed
- The macOS app lives in `macos/`.
- The overlay is a smaller frosted-glass pill with a voice-shaped waveform. It sits above the
  Dock, or at the screen bottom in full-screen apps, and follows app and Space switches.
- The microphone stays warm for a minute after each dictation.

### Fixed
- fn taps and double-taps no longer open the emoji picker while Kudu runs.
- Fewer garbled first and last words: clips are padded before recognition.
- Accessibility permission survives rebuilds when signed with a stable certificate.

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
