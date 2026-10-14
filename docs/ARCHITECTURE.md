# Architecture

Kudu is a menu-bar app built as a Swift package with three layers.

```
Sources/
  KuduCore/    pure logic, no system APIs: the text-cleanup pipeline (unit-tested)
  KuduKit/     system integration: hotkey, audio, speech engine, insertion, overlay, storage
  Kudu/        SwiftUI menu-bar app, onboarding and settings
  KuduBench/   command-line latency harness
```

## The dictation path
```
fn down ─▶ HotkeyMonitor ─▶ AudioRecorder.start ─▶ overlay shows level meter
                                   │ 16 kHz mono Float32 into memory
fn up   ─▶ AudioRecorder.stop ─▶ ParakeetEngine.transcribe ─▶ TextPipeline ─▶ TextInserter
                                   (Neural Engine, ~50–80 ms)      (<1 ms)      (AX or ⌘V)
```

### Hotkey (`KuduKit/Hotkey`)
- A session-level `CGEventTap` watches modifier changes and key presses.
- The trigger is fn, Right ⌘ or Right ⌥. Right-side modifiers are told apart by their device-specific flag bits.
- Another key during the hold means the user is typing a shortcut, so dictation is cancelled.
- Escape cancels and is swallowed. The fn release after a dictation is also swallowed, so the emoji picker doesn't open.
- Two quick taps lock hands-free recording.
- The tap runs on its own thread at the HID level (session level as a fallback).
- **The emoji picker:** while fn is the dictation key, `FnKeyGuard` (`KuduCore`) sets "Press 🌐 key to" (`com.apple.HIToolbox AppleFnUsageType`) to *Do Nothing*.
  - It saves the original value first and restores it on quit or on SIGTERM/SIGINT/SIGHUP.
  - After a crash, the original is restored on the next launch.
  - If the user changes the setting while Kudu runs, their choice is kept.

### Audio (`KuduKit/Audio`)
- `AVAudioEngine` with a tap on the input node, converted to 16 kHz mono with `AVAudioConverter`.
- The engine is prepared ahead of time; optionally it keeps running with a 0.3 s pre-roll buffer.
- Device changes rebuild the tap.

### Speech (`KuduKit/ASR`)
- `TranscriptionEngine` is the abstraction.
- `ParakeetEngine` runs Parakeet TDT 0.6B v2 (English) through FluidAudio on the Neural Engine. It is warmed up with one dummy inference at load so the first dictation is fast.

### Cleanup (`KuduCore`)
`TextPipeline.standard` runs pure `TextStage`s in this order:
1. Whitespace.
2. Snippets (whole utterance).
3. Filler removal.
4. Self-corrections.
5. Spoken commands.
6. Clock times.
7. Personal dictionary.
8. Fitting the text to the cursor.

Each stage is a pure function with table-driven tests, including cases that must not change.

### Insertion (`KuduKit/Insertion`)
1. **Accessibility:** set `kAXSelectedTextAttribute` on the focused text element, then verify the value changed.
2. **Paste:**
   - Snapshot every pasteboard type, write the text (marked transient for clipboard managers), and post ⌘V using the key that types "v" on the current layout.
   - Restore the clipboard 350 ms later, unless something else changed it in the meantime.
   - Apps known to ignore Accessibility writes (Electron/Chromium shells, terminals) go straight to paste.

### Overlay (`KuduKit/Overlay`)
- A borderless, non-activating `NSPanel` at status-bar level, created once and only shown or hidden after that.
- It never takes focus from the app you are dictating into.

## Measuring latency
- `kudu-bench file.wav ...` runs the real engine and pipeline and prints milliseconds per clip.
- In the app, each dictation is a `dictation` signpost interval: open Instruments, then **Points of Interest**.
