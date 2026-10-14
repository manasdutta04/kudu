# Roadmap

## v0.5 (current)
- [x] Learn spellings from the corrections you make after dictating
- [x] Insights: speed, cleanups, streak heat map, where you dictate

## v0.4
- [x] Command mode on fn + ⌃: edit the selection or draft new text, on device
- [x] Formatting while you speak: ~100–700 ms from release to text
- [x] Styles per app category and editing strength
- [x] Redesigned window; setup inside the app

## v0.3
- [x] Desktop app: Dock icon, main window with Home dashboard (words, words per minute, streak, history), Dictionary, Snippets, Style, Settings
- [x] Kudu logo as the app icon and an animated menu-bar icon
- [x] Smart formatting with Apple's on-device model: punctuation, paragraphs, lists, written numbers
- [x] History saved on device, with retention and clear
- [x] Fewer clipped words: silence padding around each recording
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
- [x] Microphone picker
- [x] Custom shortcut recorder
- [ ] Vocabulary biasing in the speech model from the personal dictionary
- [ ] Undo the last dictation
- [ ] Optionally pause media playback while dictating

## v0.5: on-device intelligence (macOS 26+)
- [ ] Context from the page URL and more of the focused document
- [ ] Spoken "press enter" and other editing commands

## v0.6: learning
- [ ] Import and export the dictionary as CSV

## v1.0: release
- [ ] Notarized DMG, Sparkle auto-updates, Homebrew cask
- [ ] Published latency and accuracy benchmarks

## Windows and Linux (Rust, `desktop/`)
- [x] Shared core: the same text pipeline, shortcuts and learning as the Mac app
- [x] Speech on the CPU (Parakeet or Moonshine via sherpa-onnx), transcribed at pauses while you speak
- [x] Optional small language model (Qwen2.5 0.5B / 1.5B via llama.cpp) for smart formatting and Command mode
- [x] Windows: keyboard hook, clipboard paste that stays out of clipboard history, UI Automation, overlay, tray
- [x] Linux: evdev + uinput (X11 and Wayland), AT-SPI, layer-shell / X11 overlay, tray
- [x] Settings window: Home, Insights, Dictionary, Snippets, Style, Settings
- [ ] Installers: MSI / winget, AppImage / Flatpak
- [ ] Use Windows' built-in on-device model on Copilot+ PCs

## Later
- [ ] More languages
