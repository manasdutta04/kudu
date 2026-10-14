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
while speaking ─▶ StreamingSession: re-transcribe every 1.5 s, format complete sentence chunks in the background
fn up   ─▶ AudioRecorder.stop ─▶ final transcription ─▶ reuse formatted chunks + format the rest ─▶ final touches ─▶ TextInserter ─▶ HistoryStore
                                   (~0.1–0.2 s)               (usually rules only, <1 ms)              (<1 ms)      (AX or ⌘V)
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
- The microphone is open only during a dictation: `start()` starts the engine and `stop()` stops it, so the macOS microphone indicator is off otherwise. The engine is prepared ahead of time, and again after each stop, so starting is quick.
- Device changes rebuild the tap. A cold microphone's start-up delay is logged.

### Speech (`KuduKit/ASR`)
- `TranscriptionEngine` is the abstraction.
- `ParakeetEngine` runs Parakeet TDT 0.6B v2 (English) through FluidAudio on the Neural Engine. It is warmed up with one dummy inference at load so the first dictation is fast.
- Each clip is padded with 200 ms of silence on both sides; transducer models garble words that start or end right at the edge of the audio.

### Formatting while you speak (`KuduKit/Streaming`, `KuduKit/Formatting`, `KuduCore`)
`StreamingSession` does the slow part of a dictation while it is still being spoken:
1. **Re-transcribe as you speak.** Every 1.5 s it transcribes the whole recording so far. That's cheap (about 0.1–0.2 s for 30 s of speech) and more accurate than transcribing pieces.
2. **Chunk and format in the background.** `SentenceChunker` splits the transcript into sentence-aligned chunks of 12+ words; the last sentence is held back. Complete chunks are formatted in the background, each as a continuation of the formatted text before it, and cached by their source text.
3. **On release:**
   - One final transcription.
   - Cached chunks are reused while the transcript still agrees with them.
   - Only the remaining sentences are formatted.
   - Measured on M4 with 8–38 s dictations: 100–700 ms from release to text.

**Each chunk:**
- **Rule cleanup** first (`TextPipeline.beforeFormatting`): whitespace, fillers, self-corrections, spoken commands, dictionary. Snippets are matched against the whole utterance.
- **The model only where it adds something** (`FormattingNeed`): spoken lists, numbers in words, 40+ word passages, or any chunk at the Polished strength. The speech model already punctuates ordinary sentences.
- **How the model is called** (`SmartFormatter`, on-device, macOS 26+):
  - Short instructions with two worked examples, plus the last ~200 characters of earlier text, the text before the cursor, a style hint and dictionary terms.
  - Prompt length is the main per-call cost, so these are kept short.
  - `FormattingGuard` rejects output that changes the words, with limits set by the editing strength.

**Final touches** (`TextPipeline.afterFormatting`):
- clock times;
- `MessageLayout` for "Dear …" greetings and "Best regards, …" sign-offs;
- capitalized new lines;
- `StyleStage` (Formal / Casual / Very casual per app category);
- fitting to the cursor.

### Command mode (`KuduKit/Command`)
- fn + ⌃ (`HotkeyMonitor` reports `commandPressed` / `commandReleased`) records a spoken instruction.
- The selection is read through Accessibility, or by a simulated ⌘C with the clipboard restored (`SelectionReader`).
- `CommandEngine` applies the instruction with the on-device model: an edit when text is selected, a draft when it isn't. `CommandOutput` strips preambles and quotes.
- The result replaces the selection through `TextInserter`.

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

### Screen context (`KuduKit/Context`, `KuduCore/TermExtractor`)
1. **Read.** When recording starts, `ScreenContext` walks the focused window through Accessibility off the main thread (at most 150 ms and 400 elements) and collects its visible text.
2. **Extract.** `TermExtractor` keeps names, @handles and code identifiers (top 40). They are passed to the formatter as spelling hints.
3. **Respell.** After transcription, `ScreenSpeller` replaces a word with an on-screen term when the casing differs only (github → GitHub), or when the system spell checker doesn't know the word and it sounds like the term (Sidarth → Siddharth). Real words are never touched. Nothing is stored.

### Learning from corrections (`KuduKit/Learning`, `KuduCore/CorrectionLearner`)
1. **Snapshot.** After inserting text, `EditObserver` snapshots the field through Accessibility and finds the inserted text by the unchanged text around it.
2. **Watch.** It polls every 1.5 s for up to 45 s, and stops when focus moves or the text around it changes.
3. **Compare.** `CorrectionLearner` word-aligns what Kudu typed with the user's edited version.
4. **Decide.** It keeps 1–3-word substitutions that look like a fixed spelling of a name or term: letter similarity, a matching consonant-sound key, or a proper-noun capitalization. Rewording, everyday words, numbers and rewrites are ignored.
5. **Save.** Learned words go into the dictionary marked ✦ and are used from the next dictation.

### Insights (`KuduKit/Storage/UsageLedger`, `KuduCore/Insights`)
- `UsageLedger` keeps text-free daily totals in `usage.json`: words, dictations, audio time, cleanups, dictionary fixes, learned words, and words per category and per app. It is backfilled from history on first run.
- `Insights` derives speed, the 26-week heat map (levels relative to your 90th-percentile day), current and longest streak, the monthly change and time saved.

### App and branding (`Kudu/`, `KuduKit/Branding`)
- A regular Dock app with one main window: a hidden title bar, a custom sidebar and a rounded content panel. Closing it keeps Kudu listening, and the Dock icon reopens it.
- Setup lives on the Home page (`SetupCard`), shown only while something is missing.
- `KuduMark` draws the logo as vectors from the master artwork's geometry. The menu-bar icon is the mark as a template image: its three waveform bars follow the voice while recording and ripple while text is prepared.
- SwiftUI state uses `ObservableObject`, not `@State` or `@Observable`, because those are macros and the Command Line Tools lack the SwiftUI macro plugin.

## Measuring latency
- `kudu-bench file.wav ...` runs the real engine and pipeline and prints milliseconds per clip.
- `kudu-bench --format file.wav` adds smart formatting; `kudu-bench --format --text "raw words"` formats text directly.
- `kudu-bench --stream file.wav` replays a recording in real time through `StreamingSession` and reports release → text.
- `kudu-bench --command "instruction" ["selected text"]` runs Command mode.
- In the app, each dictation is a `dictation` signpost interval: open Instruments, then **Points of Interest**.
