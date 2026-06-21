//! Detect what cannot round-trip live, and say so explicitly (architecture §9).
//!
//! We cannot beat the Studio API ceiling — engine-generated geometry (CSG unions), mesh binaries,
//! terrain voxels, and security-locked properties such as `HttpService.HttpEnabled` do not survive a
//! live text sync. What Naht does better than Rojo/Argon is **detect and warn**, with guidance,
//! instead of dropping silently. This module scans a [`Snapshot`] tree and returns those warnings;
//! callers surface them at session start and on build.

use crate::snapshot::Snapshot;

/// Engine-generated binary geometry — round-trips opaquely inside an `rbxm`, never as live text.
const BINARY_GEOMETRY_CLASSES: &[&str] =
    &["UnionOperation", "NegateOperation", "IntersectOperation"];

/// Properties that scripts and plugins cannot set, by design.
const LOCKED_PROPERTIES: &[&str] = &["HttpEnabled"];

/// Why an instance or property can't be synced live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// CSG geometry: a `UnionOperation` and friends.
    BinaryGeometry,
    /// A `MeshPart`'s mesh binary (the `MeshId` string still syncs; the mesh itself is a cloud asset).
    MeshBinary,
    /// Terrain voxels.
    Terrain,
    /// A security-locked property the plugin cannot set.
    LockedProperty(String),
}

/// One thing that won't round-trip live, with where it is and what to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The instance path, by name, from the scanned root.
    pub path: String,
    /// The Roblox class name.
    pub class: String,
    /// Why it can't sync.
    pub reason: Reason,
}

impl Warning {
    /// A human-facing explanation with guidance — never a silent drop.
    #[must_use]
    pub fn message(&self) -> String {
        match &self.reason {
            Reason::BinaryGeometry => format!(
                "{} ({}): CSG geometry can't live-sync; round-trip it with `naht build` to an rbxm \
                 instead.",
                self.path, self.class
            ),
            Reason::MeshBinary => format!(
                "{} ({}): the mesh binary can't sync (only the MeshId reference does); upload the \
                 mesh as a cloud asset.",
                self.path, self.class
            ),
            Reason::Terrain => format!(
                "{} ({}): terrain voxels can't live-sync; use the place-file fallback (`naht build`).",
                self.path, self.class
            ),
            Reason::LockedProperty(property) => format!(
                "{} ({}): `{}` is security-locked and can't be synced; set it in Studio's Game \
                 Settings.",
                self.path, self.class, property
            ),
        }
    }
}

/// What capabilities are enabled, so a now-syncable case stops being warned about.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Terrain is synced as an opaque blob (Stage 11), so it is no longer flagged unsyncable.
    pub terrain_sync: bool,
}

/// Scan a snapshot tree for everything that can't round-trip live, with default capabilities.
#[must_use]
pub fn scan(root: &Snapshot) -> Vec<Warning> {
    scan_with(root, Options::default())
}

/// Scan a snapshot tree for everything that can't round-trip live, given enabled capabilities.
#[must_use]
pub fn scan_with(root: &Snapshot, options: Options) -> Vec<Warning> {
    let mut warnings = Vec::new();
    scan_into(root, root.name.clone(), options, &mut warnings);
    warnings
}

fn scan_into(node: &Snapshot, path: String, options: Options, warnings: &mut Vec<Warning>) {
    let class_reason = if BINARY_GEOMETRY_CLASSES.contains(&node.class.as_str()) {
        Some(Reason::BinaryGeometry)
    } else if node.class == "MeshPart" {
        Some(Reason::MeshBinary)
    } else if node.class == "Terrain" && !options.terrain_sync {
        Some(Reason::Terrain)
    } else {
        None
    };
    if let Some(reason) = class_reason {
        warnings.push(Warning {
            path: path.clone(),
            class: node.class.clone(),
            reason,
        });
    }

    for property in node.properties.keys() {
        if LOCKED_PROPERTIES.contains(&property.as_str()) {
            warnings.push(Warning {
                path: path.clone(),
                class: node.class.clone(),
                reason: Reason::LockedProperty(property.clone()),
            });
        }
    }

    for child in &node.children {
        scan_into(child, format!("{path}/{}", child.name), options, warnings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbx_dom_weak::types::Variant;

    #[test]
    fn flags_a_union_as_binary_geometry() {
        let tree =
            Snapshot::new("Folder", "proj").with_child(Snapshot::new("UnionOperation", "Bridge"));
        let warnings = scan(&tree);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].path, "proj/Bridge");
        assert_eq!(warnings[0].reason, Reason::BinaryGeometry);
        assert!(warnings[0].message().contains("naht build"));
    }

    #[test]
    fn flags_a_locked_property() {
        let tree = Snapshot::new("Folder", "proj").with_child(
            Snapshot::new("HttpService", "HttpService")
                .with_property("HttpEnabled", Variant::Bool(true)),
        );
        let warnings = scan(&tree);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].reason,
            Reason::LockedProperty("HttpEnabled".to_string())
        );
        assert!(warnings[0].message().contains("Game Settings"));
    }

    #[test]
    fn flags_terrain_and_meshparts() {
        let tree = Snapshot::new("Folder", "proj")
            .with_child(Snapshot::new("Terrain", "Terrain"))
            .with_child(Snapshot::new("MeshPart", "Rock"));
        let reasons: Vec<_> = scan(&tree).into_iter().map(|w| w.reason).collect();
        assert!(reasons.contains(&Reason::Terrain));
        assert!(reasons.contains(&Reason::MeshBinary));
    }

    #[test]
    fn terrain_is_not_flagged_when_terrain_sync_is_enabled() {
        let tree = Snapshot::new("Folder", "proj")
            .with_child(Snapshot::new("Terrain", "Terrain"))
            .with_child(Snapshot::new("MeshPart", "Rock"));
        let options = Options { terrain_sync: true };
        let reasons: Vec<_> = scan_with(&tree, options)
            .into_iter()
            .map(|w| w.reason)
            .collect();
        // Terrain now syncs as a blob, but the mesh binary is still flagged.
        assert!(!reasons.contains(&Reason::Terrain));
        assert!(reasons.contains(&Reason::MeshBinary));
    }

    #[test]
    fn a_plain_script_project_has_no_warnings() {
        let tree = Snapshot::new("Folder", "proj")
            .with_child(Snapshot::new("ModuleScript", "Greeter"))
            .with_child(Snapshot::new("Script", "Main"));
        assert!(scan(&tree).is_empty());
    }
}
