import Foundation

/// Finds places to cut a long recording into pieces for the speech model: in the middle of a
/// pause, once a piece is long enough to give the model context. Meetings are transcribed
/// piece by piece as they go, so only the last few seconds are left when they end.
///
/// The rules match the Windows and Linux app's segmenter.
public struct SpeechChunker: Sendable {
    public var rate: Int
    /// Pieces shorter than this aren't cut: the model is more accurate with context.
    public var minimumSeconds: Double = 4
    /// A pause at least this long is a place to cut.
    public var pauseSeconds: Double = 0.35
    /// After this long without a clear pause, a shorter one (0.16 s) will do.
    public var longSeconds: Double = 20
    /// Never let a piece grow past this, pause or not.
    public var maximumSeconds: Double = 28

    private static let frameSeconds = 0.02

    public init(rate: Int = 16_000) {
        self.rate = rate
    }

    /// Where to cut `samples` (an index), or nil to wait for more audio.
    public func nextCut(_ samples: [Float]) -> Int? {
        let frame = max(1, Int(Double(rate) * Self.frameSeconds))
        let frames = samples.count / frame
        guard frames >= 10 else { return nil }
        var energies = [Float](repeating: 0, count: frames)
        for index in 0..<frames {
            var sum: Float = 0
            for sample in samples[(index * frame)..<((index + 1) * frame)] { sum += sample * sample }
            energies[index] = (sum / Float(frame)).squareRoot()
        }
        let threshold = Self.speechThreshold(energies)
        let minimumFrames = Int(minimumSeconds / Self.frameSeconds)
        let longFrames = Int(longSeconds / Self.frameSeconds)
        var quietRun = 0
        var heardSpeech = false
        for (index, energy) in energies.enumerated() {
            let pause = index >= longFrames ? 0.16 : pauseSeconds
            let pauseFrames = max(1, Int(pause / Self.frameSeconds))
            if energy < threshold {
                quietRun += 1
            } else {
                quietRun = 0
                heardSpeech = true
            }
            let pauseStart = index + 1 - quietRun
            if quietRun >= pauseFrames, pauseStart >= minimumFrames, heardSpeech {
                return (pauseStart + quietRun / 2) * frame
            }
        }
        let maximum = Int(maximumSeconds * Double(rate))
        return samples.count >= maximum ? maximum : nil
    }

    /// Quiet means close to the room's noise floor (the 10th percentile of frame loudness) and
    /// well below the speech level (the 90th percentile), with a minimum so a perfectly silent
    /// input still has a threshold.
    static func speechThreshold(_ energies: [Float]) -> Float {
        let sorted = energies.sorted()
        let floor = sorted[sorted.count / 10]
        let loud = sorted[sorted.count * 9 / 10]
        return max(min(floor * 3, loud * 0.25), 0.004)
    }

    /// Whether a piece is worth sending to the speech model: it has some sound above the
    /// noise floor, not just a quiet room or a muted line.
    public static func hasSpeech(_ samples: [Float]) -> Bool {
        var peak: Float = 0
        for sample in samples { peak = max(peak, abs(sample)) }
        return peak > 0.01
    }
}
