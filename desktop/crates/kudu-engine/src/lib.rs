//! Kudu's engine for Windows and Linux: microphone capture, on-device speech recognition
//! (sherpa-onnx), and an optional small language model (llama.cpp) for smart formatting and
//! Command mode. Everything runs on the user's computer.

pub mod asr;
pub mod audio;
pub mod models;
pub mod resample;
pub mod segmenter;
pub mod sounds;

#[cfg(feature = "smart")]
pub mod llm;
#[cfg(feature = "smart")]
pub mod smart;

pub use asr::SpeechEngine;
pub use audio::{Recorder, SAMPLE_RATE};
pub use models::{SmartModel, SpeechModel};
