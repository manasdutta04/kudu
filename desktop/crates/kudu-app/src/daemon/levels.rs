//! Turns microphone loudness into the overlay's waveform, like the Mac app's pill.

/// Smoothed loudness history (0…1), newest first.
#[derive(Clone, Debug, Default)]
pub struct Levels {
    smoothed: f32,
    history: [f32; 5],
}

impl Levels {
    /// Feed the RMS of ~20 ms of audio.
    pub fn push(&mut self, rms: f32) -> [f32; 5] {
        // Perceived loudness is logarithmic: map −55 dBFS (room tone) … −12 dBFS (loud speech)
        // onto 0…1, so quiet speech still moves the bars and loud speech doesn't pin them.
        let decibels = 20.0 * rms.max(1e-6).log10();
        let target = ((decibels + 55.0) / 43.0).clamp(0.0, 1.0);
        // Fast attack, slow release: jumps with each syllable, settles smoothly.
        let rate = if target > self.smoothed { 0.55 } else { 0.18 };
        self.smoothed += (target - self.smoothed) * rate;
        self.history.rotate_right(1);
        self.history[0] = self.smoothed;
        self.history
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_moves_the_bars_and_silence_settles() {
        let mut levels = Levels::default();
        let loud = (0..10).map(|_| levels.push(0.1)).last().unwrap();
        assert!(loud[0] > 0.6);
        let quiet = (0..40).map(|_| levels.push(0.0005)).last().unwrap();
        assert!(quiet[0] < 0.1);
    }
}
