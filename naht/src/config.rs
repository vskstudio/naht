//! Layered configuration (architecture §6): defaults → `~/.naht/config.toml` (per-machine) →
//! project `naht.toml`. Later layers override earlier ones field by field, so a machine-wide setting
//! holds unless a project opts out of it.
//!
//! The schema is deliberately small — Naht is convention-first, so config carries only what cannot
//! be inferred: the project name, the serve port, and the place-id guard.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// The project's config file name, read from the project root.
pub const PROJECT_FILE: &str = "naht.toml";

/// The default localhost port the daemon listens on when none is configured.
pub const DEFAULT_PORT: u16 = 34872;

/// A single config layer, as parsed from one TOML file. Every field is optional so layers can be
/// merged without a partial layer wiping a value set by an earlier one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[project]` settings.
    pub project: ProjectConfig,
    /// `[serve]` settings.
    pub serve: ServeConfig,
}

/// `[project]` — identity that cannot be inferred from the directory alone.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    /// The project name; defaults to the directory name when unset.
    pub name: Option<String>,
}

/// `[serve]` — daemon settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServeConfig {
    /// The localhost port to listen on; defaults to [`DEFAULT_PORT`].
    pub port: Option<u16>,
    /// The Studio place id the daemon will sync into; `None` leaves the handshake unguarded.
    pub place_id: Option<u64>,
}

impl Config {
    /// Load the layered config for the project rooted at `root`: per-machine config first, then the
    /// project's `naht.toml`, each overriding the last.
    pub fn load(root: &Path) -> Result<Self> {
        Self::load_from(home_config_path().as_deref(), root)
    }

    /// The layered load with an explicit per-machine config path, so tests need not touch the real
    /// home directory.
    fn load_from(home_config: Option<&Path>, root: &Path) -> Result<Self> {
        let mut config = Config::default();
        if let Some(home) = home_config {
            config.overlay(read_layer(home)?);
        }
        config.overlay(read_layer(&root.join(PROJECT_FILE))?);
        Ok(config)
    }

    /// Overlay `other` onto `self`, with `other`'s present fields winning.
    fn overlay(&mut self, other: Config) {
        if other.project.name.is_some() {
            self.project.name = other.project.name;
        }
        if other.serve.port.is_some() {
            self.serve.port = other.serve.port;
        }
        if other.serve.place_id.is_some() {
            self.serve.place_id = other.serve.place_id;
        }
    }

    /// The resolved project name: the configured name, or the directory name as a fallback.
    pub fn project_name(&self, root: &Path) -> String {
        self.project.name.clone().unwrap_or_else(|| {
            root.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("naht")
                .to_string()
        })
    }

    /// The resolved serve port.
    pub fn port(&self) -> u16 {
        self.serve.port.unwrap_or(DEFAULT_PORT)
    }
}

/// Parse one config file; an absent file is an empty layer, not an error.
fn read_layer(path: &Path) -> Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("parsing {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

/// The per-machine config path, `~/.naht/config.toml`, if a home directory is known.
fn home_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".naht").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_layer_overrides_the_machine_layer_field_by_field() {
        let dir = tempfile::tempdir().unwrap();
        // Per-machine layer: a name and a port.
        let home = dir.path().join("home-config.toml");
        std::fs::write(
            &home,
            "[project]\nname = \"machine\"\n[serve]\nport = 9000\n",
        )
        .unwrap();
        // Project layer: overrides the name, leaves the port to fall through from the machine layer.
        std::fs::write(
            dir.path().join(PROJECT_FILE),
            "[project]\nname = \"project\"\n",
        )
        .unwrap();

        let config = Config::load_from(Some(&home), dir.path()).unwrap();
        assert_eq!(config.project_name(dir.path()), "project");
        assert_eq!(config.port(), 9000);
    }

    #[test]
    fn name_falls_back_to_directory_when_unset() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load_from(None, dir.path()).unwrap();
        // No file: the name is the directory's, and the port is the default.
        let expected = dir.path().file_name().unwrap().to_str().unwrap();
        assert_eq!(config.project_name(dir.path()), expected);
        assert_eq!(config.port(), DEFAULT_PORT);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PROJECT_FILE), "[project]\nbogus = 1\n").unwrap();
        assert!(Config::load_from(None, dir.path()).is_err());
    }
}
