//! Converts recordings to the 16 kHz the speech models expect, with a windowed-sinc low-pass
//! so higher frequencies don't fold back into the speech band (plain decimation would).

/// Resamples mono audio from `from` Hz to `to` Hz.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((input.len() as f64) * ratio).floor() as usize;
    // Cut-off just below the lower Nyquist frequency.
    let cutoff = 0.95 * ratio.min(1.0);
    const HALF_TAPS: i64 = 24;
    let mut output = Vec::with_capacity(out_len);
    for n in 0..out_len {
        let center = n as f64 / ratio;
        let first = (center.floor() as i64 - HALF_TAPS + 1).max(0);
        let last = (center.floor() as i64 + HALF_TAPS).min(input.len() as i64 - 1);
        let mut acc = 0.0f64;
        let mut weight = 0.0f64;
        for i in first..=last {
            let x = center - i as f64;
            let w = sinc(x * cutoff) * blackman(x / (HALF_TAPS as f64 + 1.0));
            acc += input[i as usize] as f64 * w;
            weight += w;
        }
        output.push(if weight.abs() > 1e-9 { (acc / weight) as f32 } else { 0.0 });
    }
    output
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Blackman window over -1…1.
fn blackman(t: f64) -> f64 {
    if t.abs() >= 1.0 {
        return 0.0;
    }
    let x = (t + 1.0) / 2.0; // 0…1
    let tau = std::f64::consts::TAU;
    0.42 - 0.5 * (tau * x).cos() + 0.08 * (2.0 * tau * x).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, rate: u32, seconds: f64) -> Vec<f32> {
        (0..(rate as f64 * seconds) as usize)
            .map(|i| (std::f64::consts::TAU * freq * i as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn keeps_speech_frequencies_and_length() {
        let input = tone(440.0, 48_000, 1.0);
        let output = resample(&input, 48_000, 16_000);
        assert_eq!(output.len(), 16_000);
        let middle = &output[1_000..15_000];
        assert!((rms(middle) - 0.3536).abs() < 0.02, "rms {}", rms(middle));
    }

    #[test]
    fn removes_frequencies_above_the_new_nyquist() {
        let input = tone(12_000.0, 48_000, 1.0);
        let output = resample(&input, 48_000, 16_000);
        assert!(rms(&output[1_000..15_000]) < 0.02);
    }

    #[test]
    fn handles_odd_rates() {
        let input = tone(300.0, 44_100, 0.5);
        assert_eq!(resample(&input, 44_100, 16_000).len(), 8_000);
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }
}
