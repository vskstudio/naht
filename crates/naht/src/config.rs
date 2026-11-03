//! Layered configuration (architecture §6): defaults → `~/.naht/config.toml` (per-machine) →
//! project `naht.toml`. Later layers override earlier ones field by field, so a machine-wide setting
//! holds unless a project opts out of it.
//!
//! The schema is deliberately small, Naht is convention-first, so config carries only what cannot
//! be inferred: the project name, the serve port, and the place-id guard.

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use thiserror::Error;
use toml::de;

/// The project's config file name, read from the project root.
pub const PROJECT_FILE: &str = "naht.toml";
pub const INTERNAL_DIR: &str = ".naht";

/// The default localhost port the daemon listens on when none is configured.
pub const DEFAULT_PORT: u16 = 34872;

/// Why a config layer could not be loaded or a port could not be resolved.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// A config file exists but cannot be read.
    #[error("cannot read `{}`", path.display())]
    Read {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// A config file is not valid TOML for this schema.
    #[error("cannot parse `{}`", path.display())]
    Parse {
        /// The file that does not parse.
        path: PathBuf,
        /// The TOML error, with its position.
        #[source]
        source: de::Error,
    },
    /// Port 0 asks the OS for an arbitrary port, which a daemon cannot advertise.
    #[error("port must be between 1 and 65535, got 0")]
    PortZero,
}

/// A single config layer, as parsed from one TOML file. Every field is optional so layers can be
/// merged without a partial layer wiping a value set by an earlier one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// `[project]` settings.
    pub project: ProjectConfig,
    /// `[serve]` settings.
    pub serve: ServeConfig,
    /// `[assets]` settings.
    pub assets: AssetsConfig,
    /// `[[tree]]` entries: instance-tree mappings that convention alone cannot express, recorded by
    /// `init --from-rojo` when migrating a Rojo project. Empty for a convention-first project.
    pub tree: Vec<TreeNode>,
}

/// One `[[tree]]` entry: an instance whose source path, class, or properties convention cannot infer.
///
/// `instance` is a slash-separated path from the place root (e.g. `ReplicatedStorage/Common`). `path`
/// (when set) is the filesystem directory, relative to the project root, whose contents source the
/// instance, the Rojo `$path` that Naht's directory convention does not already place. `class` and
/// `properties` carry a Rojo `$className` / `$properties` the convention cannot express.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeNode {
    /// The instance path from the place root, slash-separated.
    pub instance: String,
    /// The filesystem source directory (relative to the project root), if any.
    pub path: Option<String>,
    /// A class override, when not inferable from the convention.
    pub class: Option<String>,
    /// Property overrides, as TOML primitives (bool, integer, float, string).
    #[serde(default)]
    pub properties: BTreeMap<String, toml::Value>,
}

/// `[project]`, identity that cannot be inferred from the directory alone.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    /// The project name; defaults to the directory name when unset.
    pub name: Option<String>,
}

/// `[serve]`, daemon settings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServeConfig {
    /// The localhost port to listen on; defaults to [`DEFAULT_PORT`].
    pub port: Option<u16>,
    /// The Studio place id the daemon will sync into; `None` leaves the handshake unguarded.
    pub place_id: Option<u64>,
    /// Whether terrain syncs live as an opaque blob (Stage 11/13). Off by default, so terrain is
    /// flagged unsyncable until a project opts in; when on, the daemon drives the blob channel and
    /// suppresses the terrain warning.
    pub terrain_sync: Option<bool>,
}

/// `[assets]`, Open Cloud asset upload (Stage 12). Disabled by default, so the unchanged behavior is
/// reference-only: properties keep their values and nothing is uploaded.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AssetsConfig {
    /// Whether to upload local assets and rewrite their references. Off unless set.
    pub enabled: Option<bool>,
    /// The name of the environment variable holding the Open Cloud API key, never the key itself.
    pub api_key_env: Option<String>,
}

impl AssetsConfig {
    /// Whether asset upload is enabled (off by default).
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
}

impl Config {
    /// Load the layered config for the project rooted at `root`: per-machine config first, then the
    /// project's `naht.toml`, each overriding the last.
    pub fn load(root: &Path) -> Result<Self, ConfigError> {
        Self::load_from(home_config_path().as_deref(), root)
    }

    /// The layered load with an explicit per-machine config path, so tests need not touch the real
    /// home directory.
    fn load_from(home_config: Option<&Path>, root: &Path) -> Result<Self, ConfigError> {
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
        if other.serve.terrain_sync.is_some() {
            self.serve.terrain_sync = other.serve.terrain_sync;
        }
        if other.assets.enabled.is_some() {
            self.assets.enabled = other.assets.enabled;
        }
        if other.assets.api_key_env.is_some() {
            self.assets.api_key_env = other.assets.api_key_env;
        }
        if !other.tree.is_empty() {
            self.tree = other.tree;
        }
    }

    /// The resolved project name: the configured name, or the directory name as a fallback.
    pub fn project_name(&self, root: &Path) -> String {
        self.project.name.clone().unwrap_or_else(|| {
            root.file_name()
                .and_then(|os_name| os_name.to_str())
                .unwrap_or("naht")
                .to_owned()
        })
    }

    /// The resolved serve port.
    pub fn port(&self) -> u16 {
        self.serve.port.unwrap_or(DEFAULT_PORT)
    }

    /// Whether live terrain blob sync is enabled (off by default).
    pub fn terrain_sync(&self) -> bool {
        self.serve.terrain_sync.unwrap_or(false)
    }

    /// Resolve the port to bind: a `--port` flag beats `naht.toml`, which beats the default. Port 0
    /// (no fixed port) is rejected here rather than panicking at bind time.
    pub fn resolve_port(&self, cli_port: Option<u16>) -> Result<u16, ConfigError> {
        let port = cli_port.unwrap_or_else(|| self.port());
        if port == 0 {
            return Err(ConfigError::PortZero);
        }
        Ok(port)
    }
}

/// Parse one config file; an absent file is an empty layer, not an error.
fn read_layer(path: &Path) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(source) => Err(ConfigError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// The per-machine config path, `~/.naht/config.toml`, if a home directory is known.
fn home_config_path() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".naht").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_layer_overrides_the_machine_layer_field_by_field() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home-config.toml");
        fs::write(
            &home,
            "[project]\nname = \"machine\"\n[serve]\nport = 9000\n",
        )
        .unwrap();
        fs::write(
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
        let expected = dir.path().file_name().unwrap().to_str().unwrap();
        assert_eq!(config.project_name(dir.path()), expected);
        assert_eq!(config.port(), DEFAULT_PORT);
    }

    #[test]
    fn resolve_port_prefers_cli_then_config_then_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(PROJECT_FILE), "[serve]\nport = 5000\n").unwrap();
        let config = Config::load_from(None, dir.path()).unwrap();

        assert_eq!(config.resolve_port(Some(6000)).unwrap(), 6000);
        assert_eq!(config.resolve_port(None).unwrap(), 5000);

        let bare = Config::default();
        assert_eq!(bare.resolve_port(None).unwrap(), DEFAULT_PORT);
    }

    #[test]
    fn resolve_port_rejects_zero() {
        assert!(Config::default().resolve_port(Some(0)).is_err());
    }

    #[test]
    fn assets_are_disabled_by_default_and_opt_in_via_config() {
        let bare = Config::default();
        assert!(!bare.assets.is_enabled());

        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(PROJECT_FILE),
            "[assets]\nenabled = true\napi_key_env = \"NAHT_OPENCLOUD_KEY\"\n",
        )
        .unwrap();
        let config = Config::load_from(None, dir.path()).unwrap();
        assert!(config.assets.is_enabled());
        assert_eq!(
            config.assets.api_key_env.as_deref(),
            Some("NAHT_OPENCLOUD_KEY")
        );
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(PROJECT_FILE), "[project]\nbogus = 1\n").unwrap();
        assert!(Config::load_from(None, dir.path()).is_err());
    }
}
