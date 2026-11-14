//! Persistent Muelle configuration stored in `%APPDATA%\Muelle\config.json`.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CURRENT_VERSION: u32 = 1;

/// Error reading or saving the configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("no se pudo acceder a `{path}`")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{path}` no es una configuración válida")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("no se pudo serializar la configuración")]
    Serialize(#[from] serde_json::Error),
}

/// User settings. Missing fields use their default values, so adding fields
/// does not break existing configuration files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub version: u32,
    pub pinned: Vec<String>,
    pub recent: Vec<String>,
    #[serde(alias = "auto_hide")]
    pub auto_hide: bool,
    /// Icon side at rest, in logical pixels.
    pub icon_size: u32,
    pub magnification: bool,
    /// Side of the icon under the cursor, in logical pixels.
    pub magnified_size: u32,
    /// Genie effect when minimizing. Off by default: while it is on, the dock
    /// turns off the system's own window transitions (see plan, "Efecto genio").
    pub genie: bool,
    /// macOS-style bar across the top with the clock and status items.
    pub menu_bar: bool,
    /// Keep the menu bar off screen until the pointer touches the top edge.
    pub menu_bar_auto_hide: bool,
    pub language: Language,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Follow the Windows display language.
    #[default]
    Auto,
    Es,
    En,
}

pub const ICON_SIZE_RANGE: (u32, u32) = (32, 96);
pub const MAX_MAGNIFIED_SIZE: u32 = 160;

impl Config {
    /// Brings out-of-range values (a hand-edited file) back into range.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.icon_size = self.icon_size.clamp(ICON_SIZE_RANGE.0, ICON_SIZE_RANGE.1);
        self.magnified_size = self
            .magnified_size
            .clamp(self.icon_size, MAX_MAGNIFIED_SIZE);
        self
    }

    /// Scale of the icon under the cursor (1.0 = no magnification).
    #[must_use]
    pub fn max_scale(&self) -> f32 {
        if self.magnification {
            self.magnified_size as f32 / self.icon_size as f32
        } else {
            1.0
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            pinned: Vec::new(),
            recent: Vec::new(),
            auto_hide: false,
            icon_size: 48,
            magnification: true,
            magnified_size: 77,
            genie: false,
            menu_bar: true,
            menu_bar_auto_hide: false,
            language: Language::Auto,
        }
    }
}

/// Moves the settings folder from before the app was renamed to Muelle.
pub fn migrate_legacy_folder() {
    let roaming = PathBuf::from(env::var_os("APPDATA").unwrap_or_default());
    let (old, new) = (roaming.join("WinDock"), roaming.join("Muelle"));
    if old.is_dir() && !new.exists() {
        let _ = fs::rename(old, new);
    }
}

#[must_use]
pub fn default_path() -> PathBuf {
    PathBuf::from(env::var_os("APPDATA").unwrap_or_default())
        .join("Muelle")
        .join("config.json")
}

/// Reads the configuration. Returns `Ok(None)` if the file does not exist yet.
pub fn load(path: &Path) -> Result<Option<Config>, ConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ConfigError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    serde_json::from_str(&text)
        .map(|config: Config| Some(config.sanitized()))
        .map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
}

/// Saves the configuration atomically by writing a temporary file and renaming
/// it, so an interrupted write cannot leave a truncated file.
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    let io_error = |source| ConfigError::Io {
        path: path.to_owned(),
        source,
    };
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(io_error)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(config)?).map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder = env::temp_dir().join(format!("muelle-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder.join("config.json")
    }

    #[test]
    fn load_should_return_none_when_file_is_missing() {
        assert_eq!(load(&scratch("missing")).ok(), Some(None));
    }

    #[test]
    fn save_then_load_should_round_trip() {
        let path = scratch("round-trip");
        let config = Config {
            pinned: vec!["explorer".into()],
            ..Config::default()
        };
        save(&path, &config).ok();
        assert_eq!(load(&path).ok(), Some(Some(config)));
    }

    #[test]
    fn load_should_fill_missing_fields_with_defaults() {
        let path = scratch("partial");
        save(&path, &Config::default()).ok();
        fs::write(&path, "{}").ok();
        assert_eq!(load(&path).ok(), Some(Some(Config::default())));
    }

    #[test]
    fn load_should_fail_on_invalid_json() {
        let path = scratch("invalid");
        save(&path, &Config::default()).ok();
        fs::write(&path, "esto no es json").ok();
        assert!(matches!(load(&path), Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn sanitized_should_clamp_icon_size() {
        let config = Config {
            icon_size: 500,
            ..Config::default()
        };
        assert_eq!(config.sanitized().icon_size, ICON_SIZE_RANGE.1);
    }

    #[test]
    fn sanitized_should_never_magnify_below_the_icon_size() {
        let config = Config {
            icon_size: 80,
            magnified_size: 40,
            ..Config::default()
        };
        assert_eq!(config.sanitized().magnified_size, 80);
    }

    #[test]
    fn max_scale_should_be_one_when_magnification_is_off() {
        let config = Config {
            magnification: false,
            ..Config::default()
        };
        assert_eq!(config.max_scale(), 1.0);
    }

    #[test]
    fn genie_should_be_off_by_default() {
        assert!(!Config::default().genie);
    }

    #[test]
    fn load_should_read_files_written_before_camel_case_keys() {
        let path = scratch("legacy-keys");
        save(&path, &Config::default()).ok();
        fs::write(&path, r#"{"version":1,"pinned":["a"],"auto_hide":true}"#).ok();
        let loaded = load(&path).ok().flatten();
        assert_eq!(loaded.map(|config| config.auto_hide), Some(true));
    }
}
