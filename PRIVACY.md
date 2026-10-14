# Privacy

Kudu is built so that what you say stays on your Mac.

## What Kudu does
- **Audio** is captured only while you hold the dictation key (or while hands-free recording
  is locked on). It is held in memory, transcribed on your Mac, and then discarded. Audio is
  never written to disk.
- **Speech recognition** runs locally on the Apple Neural Engine.
- **Text** is inserted into the app you are using. The last few dictations are kept in
  memory so you can copy them again; they are gone when Kudu quits.
- **Settings, dictionary and snippets** are saved as JSON files in
  `~/Library/Application Support/Kudu`.

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

## Optional "Keep microphone warm"
When enabled, the microphone stays open between dictations so recording starts instantly.
macOS shows the orange microphone indicator the whole time. Audio captured while idle is
kept only as a rolling 0.3-second buffer in memory, so the first syllable isn't cut off.
