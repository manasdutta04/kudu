import AppKit

/// Short system sounds for start, stop and cancel. Preloaded so playing one never delays
/// the microphone starting.
@MainActor
public final class SoundCues {
    public var isEnabled = true
    public var volume: Float = 0.35

    private let start = NSSound(named: "Tink")
    private let stop = NSSound(named: "Pop")
    private let cancel = NSSound(named: "Basso")

    public init() {
        for sound in [start, stop, cancel] { sound?.volume = volume }
    }

    public func playStart() { play(start) }
    public func playStop() { play(stop) }
    public func playCancel() { play(cancel) }

    private func play(_ sound: NSSound?) {
        guard isEnabled, let sound else { return }
        sound.volume = volume
        sound.stop()
        sound.play()
    }
}
