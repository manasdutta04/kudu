# Privacy

Kudu is built so that what you say stays on your Mac.

## What Kudu does
- **Audio** is captured only while you hold the dictation key (or while hands-free recording
  is locked on). It is held in memory, transcribed on your Mac, and then discarded. Audio is
  never written to disk.
- **Speech recognition** runs locally on the Apple Neural Engine.
- **Smart formatting** (punctuation, paragraphs, lists) uses Apple's on-device language model
  on macOS 26 and later. The text never leaves your Mac.
- **Text** is inserted into the app you are using.
- **History:** each dictation (the transcript, the final text, its length and timing, and
  which app it went to) is saved on this Mac in
  `~/Library/Application Support/Kudu/history.jsonl`, so the Home page can show it with your
  stats. It is kept for 90 days by default; choose 30 days, 90 days or forever, or clear it,
  in Settings.
- **Settings, dictionary and snippets** are saved as JSON files in the same folder.

## Network access
Kudu connects to the internet once, to download the speech model on first launch. After
that it works fully offline. There is no account, no analytics, no telemetry and no crash
reporting.

## Permissions
| Permission | Why |
|---|---|
| Microphone | To hear you while you dictate |
| Accessibility | To notice the dictation key and to type text into other apps |

## The 🌐 / fn key
When fn is the dictation key, Kudu sets **System Settings → Keyboard → "Press 🌐 key to"** to
*Do Nothing* while it runs, so tapping fn doesn't open the emoji picker. Your own choice is
saved first and put back the moment Kudu quits (or the next time it starts, if it crashed).
You can turn this off in Kudu's menu.

## Microphone
The microphone is open only while you are dictating: it turns on when you press the dictation
key and off the moment you stop. macOS's orange microphone indicator is never on otherwise.
