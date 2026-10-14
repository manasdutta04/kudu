//! Microphone capture. The microphone is open only while dictating: `start` opens it, `stop`
//! closes it and returns the recording as 16 kHz mono, ready for the speech model.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample};
use parking_lot::Mutex;

use crate::resample::resample;

/// Sample rate the speech models expect.
pub const SAMPLE_RATE: u32 = 16_000;

/// Called about every 20 ms with the RMS loudness of the latest audio (0…1).
pub type LevelCallback = Box<dyn Fn(f32) + Send + 'static>;

struct Active {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

/// Records from one input device. Owned by one thread (audio streams aren't `Send` on every
/// platform); the samples are gathered on the audio thread.
pub struct Recorder {
    /// Device name to use, or `None` for the system default.
    device: Option<String>,
    active: Option<Active>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self { device: None, active: None }
    }

    /// Choose an input device by name (`None` follows the system default).
    pub fn set_device(&mut self, name: Option<String>) {
        self.device = name;
    }

    /// Names of the available microphones.
    pub fn input_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| devices.map(|d| d.to_string()).collect())
            .unwrap_or_default()
    }

    pub fn is_recording(&self) -> bool {
        self.active.is_some()
    }

    /// Opens the microphone and starts recording.
    pub fn start(&mut self, on_level: LevelCallback) -> Result<()> {
        self.cancel();
        let host = cpal::default_host();
        let device = match &self.device {
            Some(name) => host
                .input_devices()
                .ok()
                .and_then(|mut devices| devices.find(|d| &d.to_string() == name))
                .or_else(|| host.default_input_device()),
            None => host.default_input_device(),
        }
        .ok_or_else(|| anyhow!("No microphone found"))?;

        let supported = device.default_input_config().context("Couldn't read the microphone's format")?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels as usize;
        let rate = config.sample_rate;
        let samples = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));
        let meter = Arc::new(Mutex::new(Meter::new(rate, on_level)));

        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, channels, samples.clone(), meter),
            SampleFormat::I16 => build::<i16>(&device, config, channels, samples.clone(), meter),
            SampleFormat::U16 => build::<u16>(&device, config, channels, samples.clone(), meter),
            SampleFormat::I32 => build::<i32>(&device, config, channels, samples.clone(), meter),
            SampleFormat::U8 => build::<u8>(&device, config, channels, samples.clone(), meter),
            other => return Err(anyhow!("Unsupported microphone sample format {other:?}")),
        }?;
        stream.play().context("Couldn't start the microphone")?;
        self.active = Some(Active { stream, samples, rate });
        Ok(())
    }

    /// Closes the microphone and returns what was recorded, as 16 kHz mono.
    pub fn stop(&mut self) -> Vec<f32> {
        let (recorded, rate) = self.stop_raw();
        resample(&recorded, rate, SAMPLE_RATE)
    }

    /// Closes the microphone and returns the recording at the device's own rate.
    pub fn stop_raw(&mut self) -> (Vec<f32>, u32) {
        let Some(active) = self.active.take() else { return (Vec::new(), SAMPLE_RATE) };
        drop(active.stream); // closes the device: the OS microphone indicator turns off
        let recorded = std::mem::take(&mut *active.samples.lock());
        (recorded, active.rate)
    }

    /// The samples recorded so far from `offset` on (device rate, mono), and that rate, while
    /// recording continues. Used to transcribe finished phrases early.
    pub fn read_since(&self, offset: usize) -> (Vec<f32>, u32) {
        match &self.active {
            Some(active) => {
                let samples = active.samples.lock();
                (samples.get(offset..).map(<[f32]>::to_vec).unwrap_or_default(), active.rate)
            }
            None => (Vec::new(), SAMPLE_RATE),
        }
    }

    /// Closes the microphone and drops the recording.
    pub fn cancel(&mut self) {
        self.active = None;
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    samples: Arc<Mutex<Vec<f32>>>,
    meter: Arc<Mutex<Meter>>,
) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: cpal::FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Mix down to mono.
            let mut mono = Vec::with_capacity(data.len() / channels.max(1));
            for frame in data.chunks(channels.max(1)) {
                let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                mono.push(sum / frame.len() as f32);
            }
            meter.lock().feed(&mono);
            samples.lock().extend_from_slice(&mono);
        },
        |error| log::error!("Microphone error: {error}"),
        None,
    )?;
    Ok(stream)
}


/// Reports RMS loudness every ~20 ms of audio.
struct Meter {
    window: usize,
    sum: f32,
    count: usize,
    on_level: LevelCallback,
}

impl Meter {
    fn new(rate: u32, on_level: LevelCallback) -> Self {
        Self { window: (rate as usize / 50).max(1), sum: 0.0, count: 0, on_level }
    }

    fn feed(&mut self, samples: &[f32]) {
        for sample in samples {
            self.sum += sample * sample;
            self.count += 1;
            if self.count == self.window {
                (self.on_level)((self.sum / self.count as f32).sqrt());
                self.sum = 0.0;
                self.count = 0;
            }
        }
    }
}
