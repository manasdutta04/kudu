# Roadmap

## v0.3 (current)
- [x] Desktop app: Dock icon, main window with Home dashboard (words, words per minute, streak, history), Dictionary, Snippets, Style, Settings
- [x] Kudu logo as the app icon and an animated menu-bar icon
- [x] Smart formatting with Apple's on-device model: punctuation, paragraphs, lists, written numbers
- [x] History saved on device, with retention and clear
- [x] Fewer clipped words: audio padding and a warm microphone after each dictation
- [x] fn never opens the emoji picker while Kudu runs; overlay follows the Dock and full-screen apps

## v0.2
- [x] Hold-to-dictate on fn / Right ⌘ / Right ⌥, Escape to cancel
- [x] Double-tap for hands-free recording
- [x] On-device Parakeet speech recognition (~50–80 ms after release on M-series)
- [x] Accessibility insertion with clipboard-paste fallback and restore
- [x] Paste key that follows the keyboard layout
- [x] Cleanup: fillers, spoken corrections, new line and new paragraph, clock times
- [x] Personal dictionary and snippets
- [x] History with copy and copy-raw
- [x] Sounds, open at login, warning when the mic is silent

## v0.4: polish
- [ ] Confirm each paste actually landed; otherwise leave the text on the clipboard and say so
- [ ] Microphone picker
- [ ] Custom shortcut recorder
- [ ] Vocabulary biasing in the speech model from the personal dictionary
- [ ] Undo the last dictation
- [ ] Optionally pause media playback while dictating

## v0.5: on-device intelligence (macOS 26+)
- [ ] Per-app tone presets on top of smart formatting
- [ ] Select text, hold fn+⌃ and say what to do with it ("make this more formal", "reply yes")
- [ ] Context from the focused app (name, page URL, text around the cursor), never leaving the Mac

## v0.6: learning
- [ ] Notice corrections made right after a dictation and suggest dictionary entries
- [ ] Import and export the dictionary as CSV

## v1.0: release
- [ ] Notarized DMG, Sparkle auto-updates, Homebrew cask
- [ ] Published latency and accuracy benchmarks

## Later
- [ ] Windows and Linux (Rust)
- [ ] More languages
