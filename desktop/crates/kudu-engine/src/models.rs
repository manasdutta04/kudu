//! The models Kudu can download, and the downloader. Nothing is bundled with the app: the
//! speech model is fetched once at setup, the small language model only if the user wants
//! smart formatting. Everything is stored in Kudu's data folder and used offline afterwards.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Speech recognition models (sherpa-onnx exports, int8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeechModel {
    /// NVIDIA Parakeet TDT 0.6B v2: the most accurate English model, the same one the Mac app
    /// uses. ~630 MB.
    Parakeet,
    /// Moonshine Base: much smaller and faster, a little less accurate. ~270 MB on disk.
    MoonshineBase,
}

impl SpeechModel {
    pub const ALL: [SpeechModel; 2] = [SpeechModel::Parakeet, SpeechModel::MoonshineBase];

    pub fn title(self) -> &'static str {
        match self {
            SpeechModel::Parakeet => "Accurate",
            SpeechModel::MoonshineBase => "Compact",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            SpeechModel::Parakeet => "Parakeet 0.6B: the best accuracy for English. Recommended.",
            SpeechModel::MoonshineBase => "Moonshine Base: smaller and faster, for older or low-memory PCs.",
        }
    }

    pub fn download_mb(self) -> u32 {
        match self {
            SpeechModel::Parakeet => 482,
            SpeechModel::MoonshineBase => 251,
        }
    }

    pub fn disk_mb(self) -> u32 {
        match self {
            SpeechModel::Parakeet => 631,
            SpeechModel::MoonshineBase => 272,
        }
    }

    fn archive(self) -> &'static str {
        match self {
            SpeechModel::Parakeet => "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
            SpeechModel::MoonshineBase => "sherpa-onnx-moonshine-base-en-int8",
        }
    }

    pub fn url(self) -> String {
        format!("https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{}.tar.bz2", self.archive())
    }

    /// Files that must be present after extraction.
    pub fn files(self) -> &'static [&'static str] {
        match self {
            SpeechModel::Parakeet => &["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt"],
            SpeechModel::MoonshineBase => &[
                "preprocess.onnx",
                "encode.int8.onnx",
                "uncached_decode.int8.onnx",
                "cached_decode.int8.onnx",
                "tokens.txt",
            ],
        }
    }

    pub fn dir(self, models: &Path) -> PathBuf {
        models.join(self.archive())
    }

    pub fn is_installed(self, models: &Path) -> bool {
        let dir = self.dir(models);
        self.files().iter().all(|f| dir.join(f).is_file())
    }
}

/// Small language models for smart formatting and Command mode (GGUF, llama.cpp).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmartModel {
    /// Qwen2.5 0.5B Instruct, 4-bit. ~490 MB. Fast on any CPU.
    Small,
    /// Qwen2.5 1.5B Instruct, 4-bit. ~1.1 GB. Better rewriting, slower without a fast CPU.
    Better,
}

impl SmartModel {
    pub const ALL: [SmartModel; 2] = [SmartModel::Small, SmartModel::Better];

    pub fn title(self) -> &'static str {
        match self {
            SmartModel::Small => "Small",
            SmartModel::Better => "Better",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            SmartModel::Small => "Qwen2.5 0.5B: lists, numbers and punctuation, fast on any PC.",
            SmartModel::Better => "Qwen2.5 1.5B: also good at Command mode rewrites; best with a recent CPU.",
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            SmartModel::Small => "qwen2.5-0.5b-instruct-q4_k_m.gguf",
            SmartModel::Better => "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        }
    }

    pub fn download_mb(self) -> u32 {
        match self {
            SmartModel::Small => 491,
            SmartModel::Better => 1117,
        }
    }

    pub fn url(self) -> String {
        let repo = match self {
            SmartModel::Small => "Qwen/Qwen2.5-0.5B-Instruct-GGUF",
            SmartModel::Better => "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
        };
        format!("https://huggingface.co/{repo}/resolve/main/{}", self.file_name())
    }

    pub fn path(self, models: &Path) -> PathBuf {
        models.join(self.file_name())
    }

    pub fn is_installed(self, models: &Path) -> bool {
        self.path(models).is_file()
    }
}

/// An HTTP client using the operating system's TLS and certificate store.
fn agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(concat!("Kudu/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Downloads `url` to `dest` (atomically: a partial file never looks complete), reporting
/// progress as 0…1. `cancelled` is polled between chunks.
pub fn download_file(url: &str, dest: &Path, progress: &mut dyn FnMut(f64), cancelled: &dyn Fn() -> bool) -> Result<()> {
    let partial = dest.with_extension("partial");
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut response = agent().get(url).call().with_context(|| format!("Couldn't reach {url}"))?;
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = response.body_mut().as_reader();
    let mut file = fs::File::create(&partial)?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut done: u64 = 0;
    let mut last_report = 0.0;
    loop {
        if cancelled() {
            drop(file);
            let _ = fs::remove_file(&partial);
            bail!("Download cancelled");
        }
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        done += n as u64;
        if let Some(total) = total {
            let fraction = done as f64 / total as f64;
            if fraction - last_report >= 0.005 {
                last_report = fraction;
                progress(fraction.min(1.0));
            }
        }
    }
    file.sync_all()?;
    drop(file);
    if let Some(total) = total {
        if done != total {
            let _ = fs::remove_file(&partial);
            bail!("The download was interrupted ({done} of {total} bytes)");
        }
    }
    fs::rename(&partial, dest)?;
    progress(1.0);
    Ok(())
}

/// Downloads and installs a speech model into `models`.
pub fn install_speech_model(
    model: SpeechModel,
    models: &Path,
    progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    fs::create_dir_all(models)?;
    let archive = models.join(format!("{}.tar.bz2", model.archive()));
    // Downloading is ~95% of the time; extracting the rest.
    download_file(&model.url(), &archive, &mut |f| progress(f * 0.95), cancelled)?;
    let staging = models.join(format!("{}.extracting", model.archive()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    let file = fs::File::open(&archive)?;
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(std::io::BufReader::new(file)));
    // Extract only the files the model needs (the archives also carry test recordings).
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else { continue };
        if model.files().contains(&name.as_str()) {
            entry.unpack(staging.join(&name))?;
        }
    }
    for file in model.files() {
        if !staging.join(file).is_file() {
            bail!("The model archive is missing {file}");
        }
    }
    let dir = model.dir(models);
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&staging, &dir)?;
    let _ = fs::remove_file(&archive);
    progress(1.0);
    Ok(())
}

/// Downloads a small language model into `models`.
pub fn install_smart_model(
    model: SmartModel,
    models: &Path,
    progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    download_file(&model.url(), &model.path(models), progress, cancelled)
}

/// Deletes a downloaded model to free space.
pub fn remove_speech_model(model: SpeechModel, models: &Path) -> Result<()> {
    let dir = model.dir(models);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| anyhow!("Couldn't remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

pub fn remove_smart_model(model: SmartModel, models: &Path) -> Result<()> {
    let path = model.path(models);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real HTTPS download through GitHub's redirect, with the TLS the app ships. Needs the
    /// network, so it runs in CI (`--include-ignored`), not by default.
    #[test]
    #[ignore = "needs the network"]
    fn downloads_over_https() {
        let dest = std::env::temp_dir().join(format!("kudu-download-test-{}", std::process::id()));
        let mut last = 0.0;
        download_file(
            "https://github.com/manasdutta04/kudu/releases/download/v0.1.0/install.sh",
            &dest,
            &mut |f| last = f,
            &|| false,
        )
        .expect("download");
        let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        let _ = fs::remove_file(&dest);
        assert!(size > 1000, "got {size} bytes");
        assert_eq!(last, 1.0);
    }

    #[test]
    fn model_urls_point_at_releases() {
        for model in SpeechModel::ALL {
            assert!(model.url().starts_with("https://github.com/k2-fsa/sherpa-onnx/releases/download/"));
        }
        for model in SmartModel::ALL {
            assert!(model.url().starts_with("https://huggingface.co/Qwen/"));
        }
    }
}
