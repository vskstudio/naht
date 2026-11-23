use serde::Deserialize;

use super::ConfigError;

const TERRAIN_VOXELS_CEILING: usize = 8 * 1024 * 1024;

const INVENTORY_BODY_CEILING: usize = 128 * 1024 * 1024;

const MASS_DELETE_WARN_CEILING: usize = 100_000;

const CONNECTIONS_CEILING: usize = 1024;

const TERRAIN_VOXELS_DEFAULT: usize = 1_048_576;

const INVENTORY_BODY_DEFAULT: usize = 64 * 1024 * 1024;

const MASS_DELETE_WARN_DEFAULT: usize = 25;

const CONNECTIONS_DEFAULT: usize = 64;

/// The validated limits the daemon enforces. Built by [`LimitsConfig::resolve`], so every field is
/// within `1..=ceiling`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most voxels one live terrain push may carry.
    pub terrain_voxels: usize,
    /// The largest `/inventory` body, in bytes.
    pub inventory_body_bytes: usize,
    /// How many deletions in one reconcile make the daemon warn that the change looks mass.
    pub mass_delete_warn: usize,
    /// How many authenticated connections the accept loop serves at once. Tokenless connections have their own pool of 16.
    pub connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            terrain_voxels: TERRAIN_VOXELS_DEFAULT,
            inventory_body_bytes: INVENTORY_BODY_DEFAULT,
            mass_delete_warn: MASS_DELETE_WARN_DEFAULT,
            connections: CONNECTIONS_DEFAULT,
        }
    }
}

impl Limits {
    /// Whether a reconcile that deletes `deletions` files is large enough to warn about.
    pub fn is_mass_deletion(&self, deletions: usize) -> bool {
        deletions >= self.mass_delete_warn
    }
}

/// The raw `[limits]` table, where each unset key keeps its built-in default.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitsConfig {
    /// The most voxels one live terrain push may carry.
    pub terrain_voxels: Option<usize>,
    /// The largest `/inventory` body, in bytes.
    pub inventory_body_bytes: Option<usize>,
    /// How many deletions in one reconcile make the daemon warn.
    pub mass_delete_warn: Option<usize>,
    /// How many authenticated connections the accept loop serves at once. Tokenless connections have their own pool of 16.
    pub connections: Option<usize>,
}

impl LimitsConfig {
    /// Overlay `other`'s set keys onto `self`.
    pub fn overlay(&mut self, other: &Self) {
        if other.terrain_voxels.is_some() {
            self.terrain_voxels = other.terrain_voxels;
        }
        if other.inventory_body_bytes.is_some() {
            self.inventory_body_bytes = other.inventory_body_bytes;
        }
        if other.mass_delete_warn.is_some() {
            self.mass_delete_warn = other.mass_delete_warn;
        }
        if other.connections.is_some() {
            self.connections = other.connections;
        }
    }

    /// Resolve the raw table into [`Limits`], filling unset keys with their defaults.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::LimitOutOfRange`] when a key is zero or above its ceiling.
    pub fn resolve(&self) -> Result<Limits, ConfigError> {
        Ok(Limits {
            terrain_voxels: bounded(
                "terrain_voxels",
                self.terrain_voxels,
                TERRAIN_VOXELS_DEFAULT,
                TERRAIN_VOXELS_CEILING,
            )?,
            inventory_body_bytes: bounded(
                "inventory_body_bytes",
                self.inventory_body_bytes,
                INVENTORY_BODY_DEFAULT,
                INVENTORY_BODY_CEILING,
            )?,
            mass_delete_warn: bounded(
                "mass_delete_warn",
                self.mass_delete_warn,
                MASS_DELETE_WARN_DEFAULT,
                MASS_DELETE_WARN_CEILING,
            )?,
            connections: bounded(
                "connections",
                self.connections,
                CONNECTIONS_DEFAULT,
                CONNECTIONS_CEILING,
            )?,
        })
    }
}

fn bounded(
    key: &'static str,
    configured: Option<usize>,
    default: usize,
    ceiling: usize,
) -> Result<usize, ConfigError> {
    let value = configured.unwrap_or(default);
    if value == 0 || value > ceiling {
        return Err(ConfigError::LimitOutOfRange {
            key,
            value,
            ceiling,
        });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured(key: &str, value: usize) -> LimitsConfig {
        let mut config = LimitsConfig::default();
        match key {
            "terrain_voxels" => config.terrain_voxels = Some(value),
            "inventory_body_bytes" => config.inventory_body_bytes = Some(value),
            "mass_delete_warn" => config.mass_delete_warn = Some(value),
            _ => config.connections = Some(value),
        }
        config
    }

    #[test]
    fn unset_limits_resolve_to_the_shipped_defaults() {
        assert_eq!(
            LimitsConfig::default().resolve().unwrap(),
            Limits::default()
        );
        assert_eq!(
            Limits::default(),
            Limits {
                terrain_voxels: 1_048_576,
                inventory_body_bytes: 64 * 1024 * 1024,
                mass_delete_warn: 25,
                connections: 64,
            }
        );
    }

    #[test]
    fn a_zero_limit_is_refused_with_its_key() {
        let error = configured("connections", 0).resolve().unwrap_err();
        assert!(matches!(
            error,
            ConfigError::LimitOutOfRange {
                key: "connections",
                value: 0,
                ceiling: CONNECTIONS_CEILING,
            }
        ));
    }

    #[test]
    fn a_limit_above_its_ceiling_is_refused() {
        let error = configured("terrain_voxels", TERRAIN_VOXELS_CEILING + 1)
            .resolve()
            .unwrap_err();
        assert!(matches!(
            error,
            ConfigError::LimitOutOfRange {
                key: "terrain_voxels",
                ceiling: TERRAIN_VOXELS_CEILING,
                ..
            }
        ));
    }

    #[test]
    fn a_limit_at_its_ceiling_is_accepted() {
        let limits = configured("inventory_body_bytes", INVENTORY_BODY_CEILING)
            .resolve()
            .unwrap();
        assert_eq!(limits.inventory_body_bytes, INVENTORY_BODY_CEILING);
    }

    #[test]
    fn a_configured_mass_delete_threshold_decides_the_warning() {
        let limits = configured("mass_delete_warn", 3).resolve().unwrap();
        assert!(!limits.is_mass_deletion(2));
        assert!(limits.is_mass_deletion(3));
        assert!(!Limits::default().is_mass_deletion(24));
        assert!(Limits::default().is_mass_deletion(25));
    }

    #[test]
    fn a_later_layer_overrides_only_the_keys_it_sets() {
        let mut base = configured("connections", 8);
        base.overlay(&configured("terrain_voxels", 2048));
        let limits = base.resolve().unwrap();
        assert_eq!(limits.connections, 8);
        assert_eq!(limits.terrain_voxels, 2048);
    }
}
