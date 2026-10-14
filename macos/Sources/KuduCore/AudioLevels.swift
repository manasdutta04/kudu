import Foundation

public enum AudioLevels {
    /// True when a recording carries no signal at all: a muted, disconnected or blocked input
    /// delivers exact zeros (or near enough), while even a quiet room is well above this.
    public static func isSilent(_ samples: [Float], threshold: Float = 1e-4) -> Bool {
        var peak: Float = 0
        for sample in samples {
            peak = max(peak, abs(sample))
            if peak > threshold { return false }
        }
        return true
    }
}
