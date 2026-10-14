# Changelog

All notable changes to Kudu are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com) and the project uses [Semantic Versioning](https://semver.org).

## [Unreleased] - Windows and Linux
### Added
- **Kudu for Windows and Linux** (`desktop/`, Rust), in beta. The same text cleanup, styles,
  dictionary, snippets, learning and shortcuts as the Mac app, from a shared core.
- Speech on the CPU: Parakeet (Accurate, 482 MB download) or Moonshine (Compact, 251 MB).
  Finished phrases are transcribed at pauses while you speak, so release-to-text stays short.
- Optional smart formatting and Command mode with a small local model: Qwen2.5 0.5B (491 MB)
  or 1.5B (1.1 GB).
- Windows: low-level keyboard hook (Ctrl + Win to dictate by default, without opening the
  Start menu), paste that stays out of clipboard history, UI Automation, overlay pill, tray.
- Linux: X11 and Wayland through evdev and uinput, AT-SPI, a layer-shell or X11 overlay pill,
  and a StatusNotifierItem tray.
- A settings window with Home, Insights, Dictionary, Snippets, Style and Settings.

## [0.8.0] - 2026-10-14
### Added
- **Customizable shortcuts.** Settings → Shortcuts: click Change and press the keys you want
  for dictation, Command mode and paste last transcript. Modifier keys alone (fn, Right ⌥,
  ⌃⌥) are held to talk; a key with modifiers (⌥Space) works too. The same shortcut engine
  runs on Windows and Linux.

### Changed
- Kudu for Mac is for Apple Silicon (M1 or newer) only.

## [0.7.0] - 2026-10-14
### Added
- **"Added to dictionary" in the pill.** When you fix a word Kudu got wrong, the pill shows
  what it learned, with Undo.
- **Sound-alike spelling.** Words the speech model misheard are matched by sound against your
  dictionary and the names on screen: "hitmap" → heatmap, "heat map" → heatmap, "super base" →
  Supabase, "Pia" → Priya (when Priya is in your dictionary).
- **Spoken file names:** "use case dot ts" → useCase.ts, "read me dot md" → README.md.

### Changed
- Learning, screen names and Command mode now work in Electron and Chromium apps (chat apps,
  Slack, VS Code, Notion, browsers): Kudu asks them to expose their text to Accessibility.
- Corrections are learned as soon as your edit settles, within 60 seconds of dictating.
- "tag Priya" → @Priya in docs and other apps too (not in email or code).

### Fixed
- Command mode: "fix this" with nothing selected now asks you to select the text, instead of
  writing something unrelated; the model's prompt can no longer be pasted.

## [0.6.0] - 2026-10-14
### Added
- **Formats like a native writer in every app.** Spoken lists become bullets under a heading,
  units and times become digits ("2 kg", "8pm"), math becomes notation ("x² sin x"),
  "tag Priya" becomes @Priya in chat apps, and to-do apps get one item per line.
- **Code editors and terminals:** spoken symbols become symbols, identifiers are written the
  way code writes them (userService.ts, getUser), and commands come out exact.
- **Spells names the way they appear on screen.** When you start dictating, Kudu reads the
  names and terms in the window you're in (locally, nothing stored) and fixes words the speech
  model misheard (Sidarth → Siddharth, github → GitHub).

### Changed
- "No wait" and "or rather" corrections work without pauses; emails are always laid out with
  greeting and sign-off.

## [0.5.0] - 2026-10-14
### Added
- **Learns from your corrections.** Fix a name or term Kudu misspelled, right after dictating,
  and it's saved to your dictionary (marked ✦) and spelled right from then on. On-device,
  and it can be turned off.
- **Insights:** words per minute gauge, words cleaned up and learned, words dictated with the
  monthly change and time saved, a 26-week streak heat map, and where you dictate (by type of
  app and top apps).
- Dictionary and Snippets have intro cards, search and an "Add new" button.

## [0.4.1] - 2026-10-14
### Added
- A native menu-bar menu with Paste last transcript (⌃⌘V) and quick submenus for the
  dictation key, recording mode, microphone and editing strength.
- Microphone choice (System default or any input device).

### Changed
- Refined controls throughout the app: quiet value menus, themed segmented controls,
  accent switches, soft buttons and fields, and shortcuts shown as keycaps.

## [0.4.0] - 2026-10-14
### Added
- **Command mode (fn + ⌃).** Select text, hold fn and Control, and say what to do: "make
  this more formal", "translate to Hindi", "fix the grammar", "summarize". With nothing
  selected Kudu drafts it ("write a polite reply saying I'll be late"). On-device.
- **Formatting while you speak.** Recognition and formatting run during the dictation, so
  text lands about 100–700 ms after release, even for long dictations.
- **Styles per app:** Formal, Casual or Very casual for personal messages, work messages,
  email and everything else.
- **Editing strength:** Verbatim, Light or Polished.
- Letter layout for "Dear …" greetings and "Best regards, …" sign-offs.
- The text before the cursor is used as context for formatting.

### Changed
- A redesigned main window: quiet sidebar, rounded content panel, serif display type.
  Setup now lives on the Home page, and the Welcome window no longer opens on every launch.
- The start sound plays when the microphone is actually live, so the first word isn't lost.
- The language model is used only where it adds structure (lists, numbers in words, long
  passages); plain sentences are finished instantly by rules.

## [0.3.1] - 2026-10-14
### Changed
- The microphone is open only while you dictate. Removed the one-minute warm period after
  each dictation and the "Keep microphone always warm" option.

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
