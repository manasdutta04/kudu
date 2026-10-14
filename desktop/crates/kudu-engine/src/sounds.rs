//! Short, soft start/stop cues, synthesized (no sound files to ship).

use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// Recording started: two rising notes.
    Start,
    /// Recording stopped: two falling notes.
    Stop,
    /// Something went wrong (no speech, error): one low note.
    Error,
}

/// Plays a cue without blocking (on its own short-lived thread).
pub fn play(cue: Cue) {
    std::thread::Builder::new()
        .name("kudu-sound".into())
        .spawn(move || {
            if let Err(error) = play_blocking(cue) {
                log::debug!("Couldn't play the sound cue: {error}");
            }
        })
        .ok();
}

fn play_blocking(cue: Cue) -> anyhow::Result<()> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let supported = device.default_output_config()?;
    let rate = supported.sample_rate() as f32;
    let channels = supported.channels() as usize;
    let config: cpal::StreamConfig = supported.into();
    let samples = Arc::new(synthesize(cue, rate));
    let length = samples.len();
    let position = Arc::new(Mutex::new(0usize));

    let (samples_cb, position_cb) = (samples.clone(), position.clone());
    let stream = device.build_output_stream(
        config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut pos = position_cb.lock();
            for frame in data.chunks_mut(channels) {
                let value = samples_cb.get(*pos).copied().unwrap_or(0.0);
                frame.iter_mut().for_each(|s| *s = value);
                *pos += 1;
            }
        },
        |error| log::debug!("Sound output error: {error}"),
        None,
    )?;
    stream.play()?;
    std::thread::sleep(Duration::from_secs_f32(length as f32 / rate + 0.05));
    Ok(())
}

/// Sine notes with a quick attack and an exponential decay, quiet (peak ~0.12).
fn synthesize(cue: Cue, rate: f32) -> Vec<f32> {
    let notes: &[(f32, f32)] = match cue {
        Cue::Start => &[(660.0, 0.07), (880.0, 0.09)],
        Cue::Stop => &[(880.0, 0.07), (587.0, 0.09)],
        Cue::Error => &[(330.0, 0.16)],
    };
    let mut out = Vec::new();
    for &(freq, seconds) in notes {
        let n = (seconds * rate) as usize;
        for i in 0..n {
            let t = i as f32 / rate;
            let attack = (t / 0.006).min(1.0);
            let decay = (-t / (seconds * 0.45)).exp();
            out.push((std::f32::consts::TAU * freq * t).sin() * 0.12 * attack * decay);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_are_short_and_quiet() {
        for cue in [Cue::Start, Cue::Stop, Cue::Error] {
            let samples = synthesize(cue, 48_000.0);
            assert!(samples.len() < 48_000 / 4, "under a quarter second");
            assert!(samples.iter().all(|s| s.abs() <= 0.12));
        }
    }
}
