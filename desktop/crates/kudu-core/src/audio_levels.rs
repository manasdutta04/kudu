//! Checks on recorded audio.

/// Peak level at or below which a recording counts as silent.
pub const SILENCE_THRESHOLD: f32 = 1e-4;

/// True when a recording carries no signal at all: a muted, disconnected or blocked input
/// delivers exact zeros (or near enough), while even a quiet room is well above this.
pub fn is_silent(samples: &[f32]) -> bool {
    is_silent_below(samples, SILENCE_THRESHOLD)
}

/// [`is_silent`] with a custom peak threshold.
pub fn is_silent_below(samples: &[f32], threshold: f32) -> bool {
    !samples.iter().any(|sample| sample.abs() > threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeros_are_silent() {
        assert!(is_silent(&[0.0; 16_000]));
        assert!(is_silent(&[]));
    }

    #[test]
    fn quiet_speech_is_not_silent() {
        let mut samples = vec![0.0; 16_000];
        samples[8_000] = 0.002;
        assert!(!is_silent(&samples));
    }
}
