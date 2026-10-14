//! Where Kudu keeps its files.
//!
//! - Windows: `%APPDATA%\Kudu` for settings and history, `%LOCALAPPDATA%\Kudu\models` for models.
//! - Linux: `~/.config/kudu`, `~/.local/share/kudu` and `~/.local/share/kudu/models`
//!   (following `XDG_CONFIG_HOME` / `XDG_DATA_HOME`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub struct Paths {
    /// Settings, dictionary, snippets.
    pub config: PathBuf,
    /// History and usage totals.
    pub data: PathBuf,
    /// Downloaded models (large; not roamed on Windows).
    pub models: PathBuf,
    /// Log file.
    pub log: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", "Kudu").context("No home folder")?;
        let paths = Self {
            config: dirs.config_dir().to_path_buf(),
            data: dirs.data_dir().to_path_buf(),
            models: dirs.data_local_dir().join("models"),
            log: dirs.data_local_dir().join("kudu.log"),
        };
        for dir in [&paths.config, &paths.data, &paths.models] {
            fs::create_dir_all(dir).with_context(|| format!("Couldn't create {}", dir.display()))?;
        }
        Ok(paths)
    }

    pub fn settings(&self) -> PathBuf {
        self.config.join("settings.json")
    }
    pub fn cleanup(&self) -> PathBuf {
        self.config.join("cleanup.json")
    }
    pub fn dictionary(&self) -> PathBuf {
        self.config.join("dictionary.json")
    }
    pub fn snippets(&self) -> PathBuf {
        self.config.join("snippets.json")
    }
    pub fn history(&self) -> PathBuf {
        self.data.join("history.jsonl")
    }
    pub fn usage(&self) -> PathBuf {
        self.data.join("usage.json")
    }
}

/// Reads a JSON file, or the default when it's missing or unreadable (a broken file must
/// never stop Kudu from starting; it's logged and replaced on the next save).
pub fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            log::warn!("Couldn't read {}: {error}", path.display());
            T::default()
        }),
        Err(_) => T::default(),
    }
}

/// Writes JSON atomically: to a temporary file, then renamed over the old one.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// The modification time of a file, to notice edits made by the other Kudu process.
pub fn modified(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}
