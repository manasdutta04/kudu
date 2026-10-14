# Roadmap

## v0.2 (current)
- [x] Hold-to-dictate on fn / Right ⌘ / Right ⌥, Escape to cancel
- [x] Double-tap for hands-free recording
- [x] On-device Parakeet speech recognition (~50–80 ms after release on M-series)
- [x] Accessibility insertion with clipboard-paste fallback and restore
- [x] Paste key that follows the keyboard layout
- [x] Cleanup: fillers, spoken corrections, new line and new paragraph, clock times
- [x] Personal dictionary and snippets
- [x] History with copy and copy-raw
- [x] Sounds, open at login, warning when the mic is silent

## v0.3: polish
- [ ] Confirm each paste actually landed; otherwise leave the text on the clipboard and say so
- [ ] Microphone picker
- [ ] Custom shortcut recorder
- [ ] Vocabulary biasing in the speech model from the personal dictionary
- [ ] Undo the last dictation
- [ ] Optionally pause media playback while dictating

## v0.4: on-device intelligence (macOS 26+)
- [ ] Optional rewrite in a per-app tone with Apple's on-device Foundation Models
- [ ] Select text, hold fn+⌃ and say what to do with it ("make this more formal", "reply yes")
- [ ] Context from the focused app (name, page URL, text around the cursor), never leaving the Mac

## v0.5: learning
- [ ] Notice corrections made right after a dictation and suggest dictionary entries
- [ ] Import and export the dictionary as CSV

## v1.0: release
- [ ] Notarized DMG, Sparkle auto-updates, Homebrew cask
- [ ] Published latency and accuracy benchmarks

## Later
- [ ] Windows and Linux (Rust)
- [ ] More languages
