//! The models Kudu downloads, grouped into packs, and the downloader. The catalogue
//! (`packs.json`, from the kudu-models repository) is built into the app: every pack is a
//! speech model plus a writer model, with exact download sizes and sha256 checksums. Nothing
//! is bundled with the app; a pack is downloaded once at setup and used offline afterwards.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    packs: Vec<PackEntry>,
    components: HashMap<String, Component>,
}

#[derive(Deserialize)]
struct PackEntry {
    id: String,
    speech: String,
    writer: String,
    #[serde(default)]
    default: bool,
}

/// One downloadable model, as described in `packs.json`.
#[derive(Clone, Debug, Deserialize)]
pub struct Component {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
    #[serde(default)]
    pub disk_mb: Option<u32>,
    #[serde(default)]
    pub files: Vec<String>,
    pub license: String,
    /// The upstream model a speech component is (unmodified).
    #[serde(default)]
    pub model: Option<String>,
    /// The open model a writer was fine-tuned from.
    #[serde(default)]
    pub base: Option<String>,
    /// Prompt contracts the writer was trained on ("v1" long, "v2" short).
    #[serde(default)]
    pub prompts: Vec<String>,
}

static MANIFEST: LazyLock<Manifest> =
    LazyLock::new(|| serde_json::from_str(include_str!("packs.json")).expect("packs.json is valid"));

fn component(id: &str) -> &'static Component {
    MANIFEST.components.get(id).unwrap_or_else(|| panic!("packs.json has no component {id}"))
}

fn megabytes(bytes: u64) -> u32 {
    (bytes as f64 / 1_000_000.0).round() as u32
}

/// What the user picks: a speech model and a writer model, downloaded together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pack {
    #[serde(rename = "kudu-tiny")]
    Tiny,
    #[default]
    /// Kudu Base. Saved as "kudu-basic" by Kudu 0.1.3.
    #[serde(rename = "kudu-base", alias = "kudu-basic")]
    Base,
    #[serde(rename = "kudu-large")]
    Large,
}

impl Pack {
    pub const ALL: [Pack; 3] = [Pack::Tiny, Pack::Base, Pack::Large];

    pub fn id(self) -> &'static str {
        match self {
            Pack::Tiny => "kudu-tiny",
            Pack::Base => "kudu-base",
            Pack::Large => "kudu-large",
        }
    }

    /// The name people see: "Kudu Base".
    pub fn name(self) -> &'static str {
        match self {
            Pack::Tiny => "Kudu Tiny",
            Pack::Base => "Kudu Base",
            Pack::Large => "Kudu Large",
        }
    }

    fn entry(self) -> &'static PackEntry {
        MANIFEST.packs.iter().find(|p| p.id == self.id()).unwrap_or_else(|| panic!("packs.json has no pack {}", self.id()))
    }

    pub fn speech(self) -> SpeechModel {
        SpeechModel::from_id(&self.entry().speech)
    }

    pub fn writer(self) -> WriterModel {
        WriterModel::from_id(&self.entry().writer)
    }

    /// The pack offered first (marked as the default in the catalogue).
    pub fn is_recommended(self) -> bool {
        self.entry().default
    }

    pub fn summary(self) -> &'static str {
        match self {
            Pack::Tiny => "Smallest and fastest, for older or low-memory PCs.",
            Pack::Base => "The most accurate speech with the compact writer. Recommended.",
            Pack::Large => "The most accurate speech with the larger writer, for the best formatting.",
        }
    }

    /// Download size of the whole pack.
    pub fn download_bytes(self) -> u64 {
        self.speech().component().bytes + self.writer().component().bytes
    }

    /// What the pack takes on disk once installed.
    pub fn disk_mb(self) -> u32 {
        self.speech().disk_mb() + self.writer().download_mb()
    }

    pub fn is_installed(self, models: &Path) -> bool {
        self.speech().is_installed(models) && self.writer().is_installed(models)
    }
}

/// Speech recognition models (sherpa-onnx exports, int8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeechModel {
    /// kudu-speech-base: NVIDIA Parakeet TDT 0.6B v2, the most accurate English model; the
    /// same one the Mac app uses.
    Parakeet,
    /// kudu-speech-tiny: Moonshine Base, much smaller and faster, a little less accurate.
    MoonshineBase,
}

impl SpeechModel {
    pub const ALL: [SpeechModel; 2] = [SpeechModel::Parakeet, SpeechModel::MoonshineBase];

    pub fn id(self) -> &'static str {
        match self {
            SpeechModel::Parakeet => "kudu-speech-base",
            SpeechModel::MoonshineBase => "kudu-speech-tiny",
        }
    }

    fn from_id(id: &str) -> Self {
        Self::ALL.into_iter().find(|m| m.id() == id).unwrap_or_else(|| panic!("unknown speech model {id}"))
    }

    pub fn component(self) -> &'static Component {
        component(self.id())
    }

    /// "NVIDIA Parakeet TDT 0.6B v2, CC-BY-4.0": what it is and its license, for credits.
    pub fn credit(self) -> String {
        let c = self.component();
        format!("{}, {}", c.model.as_deref().unwrap_or(self.id()), c.license)
    }

    pub fn download_mb(self) -> u32 {
        megabytes(self.component().bytes)
    }

    pub fn disk_mb(self) -> u32 {
        self.component().disk_mb.unwrap_or_else(|| self.download_mb())
    }

    /// The archive's name, which is also the folder it is unpacked into.
    fn archive(self) -> &'static str {
        let url = &self.component().url;
        let name = url.rsplit('/').next().unwrap_or(url);
        name.strip_suffix(".tar.bz2").unwrap_or(name)
    }

    pub fn url(self) -> &'static str {
        &self.component().url
    }

    /// Files that must be present after extraction.
    pub fn files(self) -> &'static [String] {
        &self.component().files
    }

    pub fn dir(self, models: &Path) -> PathBuf {
        models.join(self.archive())
    }

    pub fn is_installed(self, models: &Path) -> bool {
        let dir = self.dir(models);
        self.files().iter().all(|f| dir.join(f).is_file())
    }
}

/// Kudu's writer models: small language models fine-tuned for formatting dictation and for
/// Command mode (GGUF, run by llama.cpp).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WriterModel {
    /// Fine-tuned from SmolLM2-360M-Instruct.
    #[serde(rename = "kudu-writer-tiny")]
    Tiny,
    /// Fine-tuned from Qwen2.5-0.5B-Instruct.
    #[serde(rename = "kudu-writer-base")]
    Base,
}

impl WriterModel {
    pub const ALL: [WriterModel; 2] = [WriterModel::Tiny, WriterModel::Base];

    pub fn id(self) -> &'static str {
        match self {
            WriterModel::Tiny => "kudu-writer-tiny",
            WriterModel::Base => "kudu-writer-base",
        }
    }

    fn from_id(id: &str) -> Self {
        Self::ALL.into_iter().find(|m| m.id() == id).unwrap_or_else(|| panic!("unknown writer model {id}"))
    }

    pub fn component(self) -> &'static Component {
        component(self.id())
    }

    /// "fine-tuned from SmolLM2-360M-Instruct, Apache-2.0", for credits.
    pub fn credit(self) -> String {
        let c = self.component();
        let base = c.base.as_deref().map(|b| b.rsplit('/').next().unwrap_or(b)).unwrap_or("an open model");
        format!("fine-tuned from {base}, {}", c.license)
    }

    pub fn file_name(self) -> &'static str {
        let url = &self.component().url;
        url.rsplit('/').next().unwrap_or(url)
    }

    pub fn url(self) -> &'static str {
        &self.component().url
    }

    pub fn download_mb(self) -> u32 {
        megabytes(self.component().bytes)
    }

    /// Whether the writer was trained on the short formatting prompt (contract v2).
    pub fn short_prompt(self) -> bool {
        self.component().prompts.iter().any(|p| p == "v2")
    }

    pub fn path(self, models: &Path) -> PathBuf {
        models.join(self.file_name())
    }

    pub fn is_installed(self, models: &Path) -> bool {
        self.path(models).is_file()
    }
}

/// A running sha256 over a download.
struct Checksum(Sha256);

impl Checksum {
    fn new() -> Self {
        Self(Sha256::new())
    }

    fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn hex(self) -> String {
        format!("{:x}", self.0.finalize())
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

/// The address of Kudu's latest release; it redirects to the release's tag.
const LATEST_RELEASE: &str = "https://github.com/manasdutta04/kudu/releases/latest";

/// The latest released version of Kudu ("0.1.4"), read from where GitHub's "latest release"
/// address redirects to. Sends nothing but the request itself.
pub fn latest_release() -> Result<String> {
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
    let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(concat!("Kudu/", env!("CARGO_PKG_VERSION")))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .build()
        .into();
    let response = agent.head(LATEST_RELEASE).call().context("Couldn't reach GitHub")?;
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow!("GitHub didn't say which release is the latest"))?;
    let tag = location.trim_end_matches('/').rsplit('/').next().unwrap_or_default();
    kudu_core::release_version::from_tag(tag).ok_or_else(|| anyhow!("Unexpected release tag: {tag}"))
}

/// The address of a file in the latest release ("Kudu-Windows-x64-Setup.exe").
pub fn latest_release_file(name: &str) -> String {
    format!("{LATEST_RELEASE}/download/{name}")
}

/// Downloads `url` to `dest` (atomically: a partial file never looks complete), reporting
/// progress as 0…1. `cancelled` is polled between chunks. With `sha256`, the file is checked
/// as it arrives and a damaged download is deleted instead of used.
pub fn download_file(
    url: &str,
    dest: &Path,
    sha256: Option<&str>,
    progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
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
    let mut checksum = Checksum::new();
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
        checksum.update(&buffer[..n]);
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
    if let Some(expected) = sha256 {
        let actual = checksum.hex();
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&partial);
            log::warn!("Checksum mismatch for {url}: expected {expected}, got {actual}");
            bail!("The download was damaged; please try again");
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
    let sha256 = &model.component().sha256;
    download_file(model.url(), &archive, Some(sha256), &mut |f| progress(f * 0.95), cancelled)?;
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
        if model.files().contains(&name) {
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

/// Downloads a writer model into `models`.
pub fn install_writer(
    model: WriterModel,
    models: &Path,
    progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    download_file(model.url(), &model.path(models), Some(&model.component().sha256), progress, cancelled)
}

/// Deletes a downloaded model to free space.
pub fn remove_speech_model(model: SpeechModel, models: &Path) -> Result<()> {
    let dir = model.dir(models);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| anyhow!("Couldn't remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

pub fn remove_writer(model: WriterModel, models: &Path) -> Result<()> {
    let path = model.path(models);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Deletes the general-purpose models earlier versions of Kudu downloaded for smart
/// formatting (Qwen2.5 GGUF files); Kudu's writers replace them. Returns how many were removed.
pub fn remove_legacy_models(models: &Path) -> usize {
    let Ok(entries) = fs::read_dir(models) else { return 0 };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            name.starts_with("qwen2.5-") && (name.ends_with(".gguf") || name.ends_with(".partial"))
        })
        .filter(|entry| fs::remove_file(entry.path()).is_ok())
        .count()
}

/// Deletes every downloaded model the chosen pack doesn't use, to free space.
pub fn remove_unused(pack: Pack, models: &Path) -> Result<()> {
    for speech in SpeechModel::ALL.into_iter().filter(|m| *m != pack.speech()) {
        remove_speech_model(speech, models)?;
    }
    for writer in WriterModel::ALL.into_iter().filter(|m| *m != pack.writer()) {
        remove_writer(writer, models)?;
    }
    remove_legacy_models(models);
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
            None,
            &mut |f| last = f,
            &|| false,
        )
        .expect("download");
        let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        let _ = fs::remove_file(&dest);
        assert!(size > 1000, "got {size} bytes");
        assert_eq!(last, 1.0);
    }

    /// Asks GitHub which release is the latest. Needs the network, so it runs in CI.
    #[test]
    #[ignore = "needs the network"]
    fn reads_the_latest_release() {
        let version = latest_release().expect("latest release");
        assert!(kudu_core::release_version::from_tag(&version).is_some(), "{version}");
    }

    #[test]
    fn every_pack_has_a_speech_model_and_a_writer() {
        for pack in Pack::ALL {
            for c in [pack.speech().component(), pack.writer().component()] {
                assert!(c.url.starts_with("https://"), "{}", c.url);
                assert_eq!(c.sha256.len(), 64);
                assert!(c.bytes > 100_000_000);
            }
            assert!(pack.disk_mb() <= 1000, "{} is {} MB on disk", pack.id(), pack.disk_mb());
        }
        assert_eq!(Pack::ALL.into_iter().filter(|p| p.is_recommended()).collect::<Vec<_>>(), [Pack::default()]);
        assert_eq!(Pack::Base.speech(), SpeechModel::Parakeet);
        assert_eq!(Pack::Tiny.speech(), SpeechModel::MoonshineBase);
        assert_eq!(Pack::Large.writer(), WriterModel::Base);
    }

    #[test]
    fn models_come_from_their_sources_and_keep_their_folders() {
        for model in SpeechModel::ALL {
            assert!(model.url().starts_with("https://github.com/k2-fsa/sherpa-onnx/releases/download/"));
            assert!(!model.files().is_empty());
        }
        // Earlier versions unpacked into these folders; keeping them avoids a second download.
        assert_eq!(SpeechModel::Parakeet.archive(), "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8");
        assert_eq!(SpeechModel::MoonshineBase.archive(), "sherpa-onnx-moonshine-base-en-int8");
        for model in WriterModel::ALL {
            assert!(model.url().starts_with("https://huggingface.co/manasdutta04/kudu-writer-"));
            assert!(model.file_name().ends_with(".gguf"));
            assert!(model.short_prompt());
        }
        assert_eq!(WriterModel::Tiny.credit(), "fine-tuned from SmolLM2-360M-Instruct, Apache-2.0");
        assert_eq!(SpeechModel::Parakeet.credit(), "NVIDIA Parakeet TDT 0.6B v2, CC-BY-4.0");
    }

    #[test]
    fn packs_are_named_as_in_the_catalogue() {
        assert_eq!(serde_json::to_string(&Pack::Base).unwrap(), "\"kudu-base\"");
        assert_eq!(serde_json::from_str::<Pack>("\"kudu-basic\"").unwrap(), Pack::Base);
        assert_eq!(serde_json::from_str::<WriterModel>("\"kudu-writer-base\"").unwrap(), WriterModel::Base);
        assert_eq!(serde_json::from_str::<SpeechModel>("\"moonshine-base\"").unwrap(), SpeechModel::MoonshineBase);
    }

    #[test]
    fn checksums_are_sha256() {
        let mut sum = Checksum::new();
        sum.update(b"a");
        sum.update(b"bc");
        assert_eq!(sum.hex(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn removes_old_general_purpose_models_only() {
        let dir = std::env::temp_dir().join(format!("kudu-legacy-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for name in ["qwen2.5-0.5b-instruct-q4_k_m.gguf", "qwen2.5-1.5b-instruct-q4_k_m.partial", "kudu-writer-tiny-v1-iq4_xs.gguf"] {
            fs::write(dir.join(name), b"x").unwrap();
        }
        assert_eq!(remove_legacy_models(&dir), 2);
        assert!(dir.join("kudu-writer-tiny-v1-iq4_xs.gguf").is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
