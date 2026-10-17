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
- **Usage totals** for Insights (words, dictations and time per day, and which kinds of apps
  you dictate into) are saved in `usage.json` in the same folder. They contain no text, so
  they are kept when old history is deleted.
- **Learning from corrections:** after a dictation Kudu reads the text field it typed into, for
  up to 60 seconds and only until you switch apps, to notice if you fix a word it got wrong.
  Only the corrected word is saved, to your dictionary. You can turn this off in Settings.
- **Screen context:** when you start dictating, Kudu reads the visible text of the window you
  are in through Accessibility (for at most 150 ms) and keeps only names and terms, in memory,
  so it can spell them the way they appear. Nothing is saved, and it's discarded after the
  dictation.
- **Apps built on Electron or Chromium** (Slack, VS Code, Notion, browsers and others) only
  show their text to Accessibility when asked, so Kudu turns that on for them when they become
  active. It's the same switch screen readers use; nothing is read until you dictate.
- **Settings, dictionary and snippets** are saved as JSON files in the same folder.

## Network access
Kudu connects to the internet for two things:
- **Models, once:** on first launch it downloads the models you choose: on a Mac, the speech
  model (Kudu Tiny or Kudu Base; formatting uses Apple Intelligence, already on your Mac); on
  Windows and Linux, the model pack (a speech model from the sherpa-onnx releases on GitHub and a
  writer model from Hugging Face, each checked against its published checksum).
- **An update check, once a day:** Kudu asks GitHub which version is the latest release. The
  request carries nothing about you or your dictations. Switch it off in Settings → About;
  updates are installed only when you click Install.

Dictation itself works fully offline. There is no account, no analytics, no telemetry and no crash
reporting.

## Windows and Linux
The same rules apply to Kudu for Windows and Linux: audio, transcripts and history stay on the
PC. Files live in `%APPDATA%\Kudu` and `%LOCALAPPDATA%\Kudu` on Windows, and in
`~/.config/kudu` and `~/.local/share/kudu` on Linux.
- The network is used only to download the model pack you choose (speech + writer).
- On Windows, dictated text is pasted through the clipboard with markers that keep it out of
  clipboard history and cloud sync, and your previous clipboard is put back.
- On Linux, Kudu reads key presses from `/dev/input` only to notice its shortcuts; keys are
  never recorded or stored. It types through a virtual keyboard (`/dev/uinput`).
- Reading the text field (to learn from corrections) and the window's names (to spell them)
  uses UI Automation on Windows and AT-SPI on Linux, with the same limits as on the Mac.

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
