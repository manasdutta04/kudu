# Architecture (macOS)

Kudu for macOS is a desktop app (Dock icon, main window, menu-bar item) built as a Swift
package with three layers. Paths below are relative to this `macos/` folder.

```
Sources/
  KuduCore/    pure logic, no system APIs: cleanup pipeline, formatting guard, usage stats (unit-tested)
  KuduKit/     system integration: hotkey, audio, speech, formatting, insertion, overlay, storage, branding
  Kudu/        SwiftUI app: main window (Home, Dictionary, Snippets, Style, Settings), menu bar, onboarding
  KuduBench/   command-line latency and formatting harness
Resources/     AppIcon.icns, generated from public/kudu-icon.png by scripts/make-app-icon.swift
```

## The dictation path
```
fn down ─▶ HotkeyMonitor ─▶ AudioRecorder.start ─▶ overlay shows level meter
                                   │ 16 kHz mono Float32 into memory
fn up   ─▶ AudioRecorder.stop ─▶ ParakeetEngine.transcribe ─▶ rule cleanup ─▶ SmartFormatter? ─▶ final touches ─▶ TextInserter ─▶ HistoryStore
                                   (Neural Engine, ~50–80 ms)      (<1 ms)       (long dictations, ~1 s)   (<1 ms)      (AX or ⌘V)
```

### Hotkey (`KuduKit/Hotkey`)
- A session-level `CGEventTap` watches modifier changes and key presses.
- The trigger is fn, Right ⌘ or Right ⌥. Right-side modifiers are told apart by their device-specific flag bits.
- Another key during the hold means the user is typing a shortcut, so dictation is cancelled.
- Escape cancels a dictation and is swallowed.
- Two quick taps lock hands-free recording.
- The tap runs on its own thread at the HID level (session level as a fallback).
- **The emoji picker:** macOS decides the 🌐 action below the level an app's event tap can intercept, so hiding fn events alone doesn't stop it.
  - While fn is the dictation key, `FnKeyGuard` (`KuduCore`) sets "Press 🌐 key to" to *Do Nothing*. It uses HIToolbox's `TISUpdateFnUsageType`, the same call System Settings makes, which takes effect immediately.
  - The original is saved first and restored on quit or on SIGTERM/SIGINT/SIGHUP. After a crash it is restored on the next launch, and a change the user makes meanwhile is kept.
  - The tap also hides lone fn presses (and the fn flag on other events) from apps.

### Audio (`KuduKit/Audio`)
- `AVAudioEngine` with a tap on the input node, converted to 16 kHz mono with `AVAudioConverter`.
- The engine is prepared ahead of time. After each dictation it stays warm for 60 s with a 0.3 s pre-roll buffer, so a follow-up dictation keeps its first syllable; "Keep microphone always warm" keeps it running.
- Device changes rebuild the tap. A cold microphone's start-up delay is logged.

### Speech (`KuduKit/ASR`)
- `TranscriptionEngine` is the abstraction.
- `ParakeetEngine` runs Parakeet TDT 0.6B v2 (English) through FluidAudio on the Neural Engine. It is warmed up with one dummy inference at load so the first dictation is fast.
- Each clip is padded with 200 ms of silence on both sides; transducer models garble words that start or end right at the edge of the audio.

### Cleanup and formatting (`KuduCore`, `KuduKit/Formatting`)
1. **Snippets** are checked first: if the whole dictation is a trigger, its expansion is used as is.
2. **Rule cleanup** (`TextPipeline.beforeFormatting`): whitespace, filler removal, self-corrections, spoken commands, personal dictionary.
3. **Smart formatting** (`SmartFormatter`, macOS 26+ with Apple Intelligence). It runs only for dictations of 12+ words outside code editors, using Apple's on-device model:
   - It adds punctuation, sentence and paragraph breaks, numbered lists, and written numbers (3:30, $12,000, 15%), guided by worked examples in its instructions.
   - A session is pre-warmed ahead of time, and a 2.5 s timeout falls back to the rule-cleaned text.
   - `FormattingGuard` rejects any output that changes the words (more than 35% word-level edits or a 20% length change), so the model can't answer, summarize or rewrite.
4. **Final touches** (`TextPipeline.afterFormatting`): clock times, whitespace, capitalizing new lines, fitting the text to the cursor.

Every stage is a pure function with table-driven tests, including cases that must not change.

### Insertion (`KuduKit/Insertion`)
1. **Accessibility:** set `kAXSelectedTextAttribute` on the focused text element, then verify the value changed.
2. **Paste:**
   - Snapshot every pasteboard type, write the text (marked transient for clipboard managers), and post ⌘V using the key that types "v" on the current layout.
   - Restore the clipboard 350 ms later, unless something else changed it in the meantime.
   - Apps known to ignore Accessibility writes (Electron/Chromium shells, terminals) go straight to paste.

### Overlay (`KuduKit/Overlay`)
- A borderless, non-activating `NSPanel` at status-bar level, created once and only shown or hidden after that.
- It never takes focus from the app you are dictating into.
- It sits just above the Dock, or at the screen bottom when a full-screen app or auto-hide hides the Dock, and follows app and Space switches.

### History (`KuduKit/Storage/HistoryStore`, `KuduCore/UsageStats`)
- Every dictation is appended to `~/Library/Application Support/Kudu/history.jsonl`. Each record holds the raw transcript, the final text, word count, audio length, latency, the target app, and whether formatting ran.
- History is pruned to the retention setting (90 days by default).
- `UsageStats` derives total words, words per minute (dictations of 1.5 s or longer) and the day streak for the Home page.

### App and branding (`Kudu/`, `KuduKit/Branding`)
- A regular Dock app with one main window (a `NavigationSplitView`). Closing it keeps Kudu listening, and the Dock icon reopens it.
- `KuduMark` draws the logo as vectors from the master artwork's geometry. The menu-bar icon is the mark as a template image: its three waveform bars follow the voice while recording and ripple while text is prepared.
- SwiftUI state uses `ObservableObject`, not `@State` or `@Observable`, because those are macros and the Command Line Tools lack the SwiftUI macro plugin.

## Measuring latency
- `kudu-bench file.wav ...` runs the real engine and pipeline and prints milliseconds per clip.
- `kudu-bench --format file.wav` adds smart formatting; `kudu-bench --format --text "raw words"` formats text directly.
- In the app, each dictation is a `dictation` signpost interval: open Instruments, then **Points of Interest**.
