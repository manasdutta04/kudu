//! The audio of a meeting as two streams: the microphone ("You") and, when available,
//! everything the computer plays ("Others"). Each stream is cut into pieces at pauses and
//! handed out as 16 kHz mono with its start time; audio is kept only until its piece is taken,
//! and never written to disk.
//!
//! The computer's sound comes from WASAPI loopback on Windows (an input stream on the output
//! device) and from the PulseAudio / PipeWire monitor source on Linux (`parec`). Either may be
//! unavailable; the meeting is then recorded from the microphone alone.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample};
use kudu_core::meeting::AudioSource;
use parking_lot::Mutex;

use crate::resample::resample;
use crate::segmenter::Segmenter;
use crate::SAMPLE_RATE;

/// A piece of meeting audio, ready for the speech model.
pub struct Piece {
    pub source: AudioSource,
    /// Seconds from the start of the meeting.
    pub start: f64,
    /// 16 kHz mono.
    pub samples: Vec<f32>,
}

impl Piece {
    pub fn end(&self) -> f64 {
        self.start + self.samples.len() as f64 / f64::from(SAMPLE_RATE)
    }

    /// Whether it's worth sending to the speech model: some sound above a quiet room.
    pub fn has_speech(&self) -> bool {
        self.samples.iter().any(|s| s.abs() > 0.01)
    }
}

struct Buffer {
    rate: u32,
    /// Samples received (or silence filled in) so far, at the device rate, including those
    /// already handed out.
    received: usize,
    /// Waiting to be cut into a piece; starts at sample `received - samples.len()`.
    samples: Vec<f32>,
    /// Microphone audio is replaced with silence while the user dictates.
    muted: bool,
    /// Audio actually delivered (not filled in), and its loudest sample.
    delivered: usize,
    peak: f32,
}

impl Buffer {
    fn new(rate: u32) -> Self {
        Self { rate, received: 0, samples: Vec::new(), muted: false, delivered: 0, peak: 0.0 }
    }
}

type Buffers = Arc<Mutex<Vec<(AudioSource, Buffer)>>>;

enum SystemSource {
    #[cfg_attr(not(windows), allow(dead_code))]
    Stream(cpal::Stream),
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Process(std::process::Child),
}

/// Records a meeting. Owned by one thread (audio streams aren't `Send` everywhere).
pub struct MeetingCapture {
    mic: Option<cpal::Stream>,
    system: Option<SystemSource>,
    buffers: Buffers,
    #[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
    started: Instant,
}

impl MeetingCapture {
    /// Opens the microphone (by name, or the default) and, if `include_system_audio`, the
    /// computer's sound. Fails only if the microphone can't be opened.
    pub fn start(microphone: Option<&str>, include_system_audio: bool) -> Result<Self> {
        let started = Instant::now();
        let buffers: Buffers = Arc::new(Mutex::new(Vec::new()));
        let host = cpal::default_host();
        let device = match microphone {
            Some(name) => host
                .input_devices()
                .ok()
                .and_then(|mut devices| devices.find(|d| d.to_string() == name))
                .or_else(|| host.default_input_device()),
            None => host.default_input_device(),
        }
        .ok_or_else(|| anyhow!("No microphone found"))?;
        let config = device.default_input_config().context("Couldn't read the microphone's format")?;
        let mic = open(&device, config, AudioSource::Mic, buffers.clone(), started).context("Couldn't open the microphone")?;
        let mut capture = Self { mic: Some(mic), system: None, buffers, started };
        if include_system_audio {
            match capture.open_system() {
                Ok(source) => capture.system = Some(source),
                Err(error) => log::warn!("Recording the microphone only; the computer's sound isn't available: {error:#}"),
            }
        }
        Ok(capture)
    }

    /// Whether the computer's sound is being recorded.
    pub fn has_system_audio(&self) -> bool {
        self.system.is_some()
    }

    #[cfg(windows)]
    fn open_system(&self) -> Result<SystemSource> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| anyhow!("No speakers or headphones found"))?;
        let config = device.default_output_config().context("Couldn't read the speakers' format")?;
        // An input stream on an output device records what it plays (WASAPI loopback).
        let stream = open(&device, config, AudioSource::System, self.buffers.clone(), self.started)?;
        Ok(SystemSource::Stream(stream))
    }

    #[cfg(target_os = "linux")]
    fn open_system(&self) -> Result<SystemSource> {
        use std::io::Read;
        use std::process::{Command, Stdio};
        // The monitor of the default output: what the computer plays (PulseAudio and
        // PipeWire's Pulse server both provide it).
        let mut child = Command::new("parec")
            .args(["--device=@DEFAULT_MONITOR@", "--format=float32le", "--rate=16000", "--channels=1", "--latency-msec=100", "--client-name=Kudu meeting notes"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("parec (pulseaudio-utils) isn't installed")?;
        let mut stdout = child.stdout.take().ok_or_else(|| anyhow!("No output from parec"))?;
        self.buffers.lock().push((AudioSource::System, Buffer::new(SAMPLE_RATE)));
        let (buffers, started) = (self.buffers.clone(), self.started);
        std::thread::Builder::new().name("kudu-meeting-monitor".into()).spawn(move || {
            let mut bytes = vec![0u8; 3200 * 4];
            let mut carry: Vec<u8> = Vec::new();
            loop {
                match stdout.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        carry.extend_from_slice(&bytes[..read]);
                        let whole = carry.len() / 4 * 4;
                        let samples: Vec<f32> =
                            carry[..whole].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
                        carry.drain(..whole);
                        receive(&buffers, AudioSource::System, &samples, started);
                    }
                }
            }
        })?;
        Ok(SystemSource::Process(child))
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn open_system(&self) -> Result<SystemSource> {
        Err(anyhow!("Not supported on this system"))
    }

    /// Stops recording. The audio not yet taken stays available to `take_pieces(true)`.
    pub fn stop(&mut self) {
        self.mic = None;
        match self.system.take() {
            Some(SystemSource::Process(mut child)) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            Some(SystemSource::Stream(stream)) => drop(stream),
            None => {}
        }
    }

    /// Silences the microphone (while the user dictates into another app).
    pub fn set_mic_muted(&self, muted: bool) {
        for (source, buffer) in self.buffers.lock().iter_mut() {
            if *source == AudioSource::Mic {
                buffer.muted = muted;
            }
        }
    }

    /// The pieces ready to transcribe: each stream's audio up to its next pause (or 28 s at
    /// most). With `last`, everything that's left.
    pub fn take_pieces(&self, last: bool) -> Vec<Piece> {
        let mut pieces = Vec::new();
        for (source, buffer) in self.buffers.lock().iter_mut() {
            let mut segmenter = Segmenter::new(buffer.rate);
            segmenter.max_segment = 20.0;
            let maximum = buffer.rate as usize * 28;
            while !buffer.samples.is_empty() {
                let cut = if last {
                    Some(buffer.samples.len())
                } else {
                    segmenter.next_cut(&buffer.samples, 0).or((buffer.samples.len() >= maximum).then_some(maximum))
                };
                let Some(cut) = cut else { break };
                let start_index = buffer.received - buffer.samples.len();
                let raw: Vec<f32> = buffer.samples.drain(..cut).collect();
                pieces.push(Piece {
                    source: *source,
                    start: start_index as f64 / f64::from(buffer.rate),
                    samples: resample(&raw, buffer.rate, SAMPLE_RATE),
                });
            }
        }
        pieces
    }

    /// How far the computer's sound has been listened to with nothing left to transcribe: the
    /// end of its buffer when what's waiting there is only quiet. `None` if something is.
    pub fn system_quiet_until(&self) -> Option<f64> {
        let buffers = self.buffers.lock();
        let (_, buffer) = buffers.iter().find(|(source, _)| *source == AudioSource::System)?;
        let quiet = !buffer.samples.iter().any(|s| s.abs() > 0.01);
        quiet.then(|| buffer.received as f64 / f64::from(buffer.rate))
    }

    /// A blocked or broken loopback gives exact silence: if a long stretch of the computer's
    /// sound was nothing but zeros, it most likely isn't being recorded.
    pub fn system_audio_seems_blocked(&self) -> bool {
        let buffers = self.buffers.lock();
        buffers
            .iter()
            .find(|(source, _)| *source == AudioSource::System)
            .is_some_and(|(_, buffer)| buffer.delivered > buffer.rate as usize * 20 && buffer.peak == 0.0)
    }
}

impl Drop for MeetingCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn open(device: &cpal::Device, config: cpal::SupportedStreamConfig, source: AudioSource, buffers: Buffers, started: Instant) -> Result<cpal::Stream> {
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    buffers.lock().push((source, Buffer::new(config.sample_rate)));
    let stream = match format {
        SampleFormat::F32 => build::<f32>(device, config, source, buffers, started),
        SampleFormat::I16 => build::<i16>(device, config, source, buffers, started),
        SampleFormat::U16 => build::<u16>(device, config, source, buffers, started),
        SampleFormat::I32 => build::<i32>(device, config, source, buffers, started),
        SampleFormat::U8 => build::<u8>(device, config, source, buffers, started),
        other => return Err(anyhow!("Unsupported sample format {other:?}")),
    }?;
    stream.play()?;
    Ok(stream)
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, source: AudioSource, buffers: Buffers, started: Instant) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mono: Vec<f32> = data
                .chunks(channels)
                .map(|frame| frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / frame.len() as f32)
                .collect();
            receive(&buffers, source, &mono, started);
        },
        move |error| log::error!("Meeting audio error ({source:?}): {error}"),
        None,
    )?;
    Ok(stream)
}

fn receive(buffers: &Buffers, source: AudioSource, samples: &[f32], started: Instant) {
    let mut buffers = buffers.lock();
    let Some((_, buffer)) = buffers.iter_mut().find(|(s, _)| *s == source) else { return };
    // The computer's sound arrives only while something is playing. Fill the gaps with
    // silence, so each stream's sample count stays its clock and "You" and "Others" line up.
    let expected = (started.elapsed().as_secs_f64() * f64::from(buffer.rate)) as usize;
    let gap = expected.saturating_sub(samples.len()).saturating_sub(buffer.received);
    if gap > buffer.rate as usize / 4 {
        buffer.samples.extend(std::iter::repeat_n(0.0, gap));
        buffer.received += gap;
    }
    buffer.received += samples.len();
    buffer.delivered += samples.len();
    buffer.peak = samples.iter().fold(buffer.peak, |peak, s| peak.max(s.abs()));
    if buffer.muted {
        buffer.samples.extend(std::iter::repeat_n(0.0, samples.len()));
    } else {
        buffer.samples.extend_from_slice(samples);
    }
}
