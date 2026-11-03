//! Serialize a [`Snapshot`] tree to a Roblox model file via the `rbx-dom` ecosystem.
//!
//! The reconciler works on the lightweight, comparable [`Snapshot`] tree; building a distributable
//! artifact is a separate edge concern (architecture §6: build config is independent of sync
//! mapping). A snapshot is converted into a [`WeakDom`] and written as either a **model** (binary
//! `rbxm` / XML `rbxmx` — a bare instance list) or a **place** (`rbxl`/`rbxlx` — a `DataModel` with
//! convention-mapped services). We depend on `rbx_binary`/`rbx_xml` rather than hand-rolling either.

use std::collections::BTreeMap;
use std::io::Write;

use rbx_dom_weak::types::Ref;
use rbx_dom_weak::{InstanceBuilder, WeakDom};
use rbx_reflection::ClassTag;

use crate::snapshot::Snapshot;

/// The service every loose top-level entry (and any unknown service-shaped directory) falls into.
const DEFAULT_SERVICE: &str = "Workspace";

/// Suffixes that make a top-level directory *look* like a service. Used only to warn when such a
/// directory isn't a real service — a likely typo, not a silent reparent.
const SERVICE_SUFFIXES: &[&str] = &["Service", "Storage", "Gui", "Lighting", "Players"];

/// The model serialization format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFormat {
    /// Binary `.rbxm` — compact, the usual choice.
    Binary,
    /// XML `.rbxmx` — text, diff-friendly.
    Xml,
}

/// Errors from building a model file.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The binary encoder failed.
    #[error("binary model encode failed: {0}")]
    Binary(#[from] rbx_binary::EncodeError),
    /// The XML encoder failed.
    #[error("xml model encode failed: {0}")]
    Xml(#[from] rbx_xml::EncodeError),
}

/// Write `root`'s children as a model into `writer`.
///
/// The project root itself is the conventional `Folder` named after the directory; its children are
/// the instances a consumer inserts, so the wrapping folder is dropped from the artifact.
pub fn write_model<W: Write>(
    writer: W,
    root: &Snapshot,
    format: ModelFormat,
) -> Result<(), BuildError> {
    let mut dom = WeakDom::new(InstanceBuilder::new("Folder"));
    let dom_root = dom.root_ref();
    let mut refs = Vec::with_capacity(root.children.len());
    for child in &root.children {
        refs.push(dom.insert(dom_root, build_instance(child)));
    }

    match format {
        ModelFormat::Binary => rbx_binary::to_writer(writer, &dom, &refs)?,
        ModelFormat::Xml => rbx_xml::to_writer_default(writer, &dom, &refs)?,
    }
    Ok(())
}

/// Write `root` as a **place**: a `DataModel` whose top-level directories are convention-mapped to
/// services (architecture §6 extension). A top-level directory named after a real Roblox service
/// (validated against `rbx_reflection`) becomes that service, populated with its contents; anything
/// else — loose files, ordinary directories — lands under `Workspace`. A directory that *looks* like
/// a service but isn't returns an explicit warning, never a silent reparent.
///
/// Returns the warnings, for the caller to surface.
pub fn write_place<W: Write>(
    writer: W,
    root: &Snapshot,
    format: ModelFormat,
) -> Result<Vec<String>, BuildError> {
    let (dom, warnings) = build_place_dom(root);
    let refs: Vec<Ref> = dom.root().children().to_vec();
    match format {
        ModelFormat::Binary => rbx_binary::to_writer(writer, &dom, &refs)?,
        ModelFormat::Xml => rbx_xml::to_writer_default(writer, &dom, &refs)?,
    }
    Ok(warnings)
}

/// Build the place [`WeakDom`]: a `DataModel` root whose top-level directories are convention-mapped
/// to services. Exposed alongside [`write_place`] so a caller (or a test) can inspect the built tree —
/// notably that the root instance is a `DataModel` — without re-serializing and reloading it, which
/// would lose the distinction (a reloaded model and a reloaded place both get a synthetic `DataModel`
/// root). Returns the dom and the same warnings [`write_place`] surfaces.
pub fn build_place_dom(root: &Snapshot) -> (WeakDom, Vec<String>) {
    let mut dom = WeakDom::new(InstanceBuilder::new("DataModel"));
    let data_model = dom.root_ref();
    let mut services: BTreeMap<String, Ref> = BTreeMap::new();
    let mut warnings = Vec::new();

    for child in &root.children {
        // Only a top-level *directory* maps to a service. A script that merely shares a service's
        // name (e.g. `Lighting.luau`) is ordinary source and must not be mistaken for one — it would
        // be dropped, since a service takes its directory's *children*, not the entry itself.
        let is_directory = child.class == "Folder";
        if is_directory && is_service(&child.name) {
            let service = service_ref(&mut dom, data_model, &mut services, &child.name);
            for grandchild in &child.children {
                dom.insert(service, build_instance(grandchild));
            }
        } else {
            if is_directory && looks_service_like(&child.name) {
                warnings.push(format!(
                    "'{}' looks like a service but isn't a known one; placed under {DEFAULT_SERVICE}",
                    child.name
                ));
            }
            let workspace = service_ref(&mut dom, data_model, &mut services, DEFAULT_SERVICE);
            dom.insert(workspace, build_instance(child));
        }
    }
    (dom, warnings)
}

/// Get or create the service of `class` under the `DataModel`, deduplicated by class name.
fn service_ref(
    dom: &mut WeakDom,
    data_model: Ref,
    services: &mut BTreeMap<String, Ref>,
    class: &str,
) -> Ref {
    if let Some(existing) = services.get(class) {
        return *existing;
    }
    let reference = dom.insert(data_model, InstanceBuilder::new(class));
    services.insert(class.to_string(), reference);
    reference
}

/// Whether `name` is a real Roblox service, per the reflection database.
fn is_service(name: &str) -> bool {
    rbx_reflection_database::get()
        .ok()
        .and_then(|db| {
            db.classes
                .get(name)
                .map(|class| class.tags.contains(&ClassTag::Service))
        })
        .unwrap_or(false)
}

/// Whether `name` resembles a service (by suffix) — used only to warn about a likely typo.
fn looks_service_like(name: &str) -> bool {
    SERVICE_SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

fn build_instance(snapshot: &Snapshot) -> InstanceBuilder {
    let mut builder = InstanceBuilder::new(snapshot.class.clone()).with_name(snapshot.name.clone());
    for (key, value) in &snapshot.properties {
        builder = builder.with_property(key.clone(), value.clone());
    }
    for child in &snapshot.children {
        builder = builder.with_child(build_instance(child));
    }
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbx_dom_weak::types::Variant;

    fn fixture() -> Snapshot {
        Snapshot::new("Folder", "proj")
            .with_child(
                Snapshot::new("ModuleScript", "Greeter")
                    .with_property("Source", Variant::String("return 1".to_string())),
            )
            .with_child(Snapshot::new("Folder", "sub").with_child(Snapshot::new("Script", "main")))
    }

    #[test]
    fn binary_model_round_trips_classes_names_and_source() {
        let mut bytes = Vec::new();
        write_model(&mut bytes, &fixture(), ModelFormat::Binary).unwrap();

        let dom = rbx_binary::from_reader(&bytes[..]).unwrap();
        let roots = dom.root().children();
        let mut top: Vec<_> = roots
            .iter()
            .map(|r| dom.get_by_ref(*r).unwrap())
            .map(|i| (i.name.as_str(), i.class.as_str()))
            .collect();
        top.sort();
        assert_eq!(top, vec![("Greeter", "ModuleScript"), ("sub", "Folder")]);

        let greeter = roots
            .iter()
            .map(|r| dom.get_by_ref(*r).unwrap())
            .find(|i| i.name == "Greeter")
            .unwrap();
        let source = greeter
            .properties
            .iter()
            .find(|(key, _)| key.as_str() == "Source")
            .map(|(_, value)| value);
        assert_eq!(source, Some(&Variant::String("return 1".to_string())));
    }

    #[test]
    fn xml_model_is_produced_and_reloadable() {
        let mut bytes = Vec::new();
        write_model(&mut bytes, &fixture(), ModelFormat::Xml).unwrap();
        let dom = rbx_xml::from_reader_default(&bytes[..]).unwrap();
        assert_eq!(dom.root().children().len(), 2);
    }

    #[test]
    fn a_union_round_trips_through_rbxm_without_losing_its_subtree() {
        // Naht never text-ifies binary geometry; it passes the instance through verbatim, so a
        // Union (and anything under it) round-trips opaquely inside an rbxm.
        let project = Snapshot::new("Folder", "proj").with_child(
            Snapshot::new("UnionOperation", "Bridge").with_child(
                Snapshot::new("ModuleScript", "Meta")
                    .with_property("Source", Variant::String("return 'kept'".to_string())),
            ),
        );

        let mut bytes = Vec::new();
        write_model(&mut bytes, &project, ModelFormat::Binary).unwrap();
        let dom = rbx_binary::from_reader(&bytes[..]).unwrap();

        let union_ref = dom.root().children()[0];
        let union = dom.get_by_ref(union_ref).unwrap();
        assert_eq!(union.class, "UnionOperation");
        assert_eq!(union.name, "Bridge");

        let meta = dom.get_by_ref(union.children()[0]).unwrap();
        assert_eq!(meta.name, "Meta");
        let source = meta
            .properties
            .iter()
            .find(|(key, _)| key.as_str() == "Source")
            .map(|(_, value)| value);
        assert_eq!(source, Some(&Variant::String("return 'kept'".to_string())));
    }

    /// Find a `DataModel` child instance of the given class in a reloaded place.
    fn service<'a>(dom: &'a WeakDom, class: &str) -> Option<&'a rbx_dom_weak::Instance> {
        dom.root()
            .children()
            .iter()
            .map(|r| dom.get_by_ref(*r).unwrap())
            .find(|instance| instance.class == class)
    }

    fn child_named<'a>(
        dom: &'a WeakDom,
        instance: &rbx_dom_weak::Instance,
        name: &str,
    ) -> Option<&'a rbx_dom_weak::Instance> {
        instance
            .children()
            .iter()
            .map(|r| dom.get_by_ref(*r).unwrap())
            .find(|child| child.name == name)
    }

    #[test]
    fn place_build_maps_top_level_dirs_to_services_and_loose_files_to_workspace() {
        let project = Snapshot::new("Folder", "proj")
            .with_child(
                Snapshot::new("Folder", "ServerScriptService")
                    .with_child(Snapshot::new("Script", "Main")),
            )
            .with_child(
                Snapshot::new("Folder", "ReplicatedStorage")
                    .with_child(Snapshot::new("ModuleScript", "Shared")),
            )
            .with_child(Snapshot::new("ModuleScript", "Loose"));

        let mut bytes = Vec::new();
        let warnings = write_place(&mut bytes, &project, ModelFormat::Xml).unwrap();
        assert!(warnings.is_empty());

        let dom = rbx_xml::from_reader_default(&bytes[..]).unwrap();
        let sss = service(&dom, "ServerScriptService").expect("ServerScriptService");
        assert!(child_named(&dom, sss, "Main").is_some());
        let rs = service(&dom, "ReplicatedStorage").expect("ReplicatedStorage");
        assert!(child_named(&dom, rs, "Shared").is_some());
        let workspace = service(&dom, "Workspace").expect("Workspace");
        assert!(child_named(&dom, workspace, "Loose").is_some());
    }

    #[test]
    fn an_unknown_service_shaped_dir_warns_and_falls_back_to_workspace() {
        let project = Snapshot::new("Folder", "proj").with_child(
            Snapshot::new("Folder", "MyService").with_child(Snapshot::new("ModuleScript", "X")),
        );

        let mut bytes = Vec::new();
        let warnings = write_place(&mut bytes, &project, ModelFormat::Binary).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("MyService"));

        // The whole unknown directory lands under Workspace, nothing dropped.
        let dom = rbx_binary::from_reader(&bytes[..]).unwrap();
        let workspace = service(&dom, "Workspace").expect("Workspace");
        let my_service =
            child_named(&dom, workspace, "MyService").expect("MyService under Workspace");
        assert!(child_named(&dom, my_service, "X").is_some());
    }

    #[test]
    fn a_top_level_script_named_like_a_service_is_kept_not_dropped() {
        // `Lighting.luau` is a ModuleScript named "Lighting", not the Lighting service — it must
        // survive as a Workspace child, with its source intact.
        let project = Snapshot::new("Folder", "proj").with_child(
            Snapshot::new("ModuleScript", "Lighting")
                .with_property("Source", Variant::String("return 1".to_string())),
        );

        let mut bytes = Vec::new();
        let warnings = write_place(&mut bytes, &project, ModelFormat::Binary).unwrap();
        assert!(warnings.is_empty());

        let dom = rbx_binary::from_reader(&bytes[..]).unwrap();
        // No empty Lighting service was created.
        assert!(service(&dom, "Lighting").is_none());
        let workspace = service(&dom, "Workspace").expect("Workspace");
        let script = child_named(&dom, workspace, "Lighting").expect("Lighting script kept");
        assert_eq!(script.class, "ModuleScript");
    }
}

#[cfg(test)]
mod stage15 {
    use super::*;
    use rbx_dom_weak::types::{Attributes, Color3, Enum, Tags, Variant, Vector3};

    /// Find an instance by name among `instance`'s descendants' immediate children list.
    fn prop<'a>(instance: &'a rbx_dom_weak::Instance, key: &str) -> Option<&'a Variant> {
        instance
            .properties
            .iter()
            .find(|(k, _)| k.as_str() == key)
            .map(|(_, v)| v)
    }

    #[test]
    fn place_root_instance_is_a_data_model() {
        // Stage 9 criterion 1: the built place's *root instance* is a `DataModel`, with the services
        // as its direct children — asserted on the built dom, since a reloaded model and a reloaded
        // place both get a synthetic `DataModel` root and so can't be told apart that way.
        let project = Snapshot::new("Folder", "proj")
            .with_child(
                Snapshot::new("Folder", "ReplicatedStorage")
                    .with_child(Snapshot::new("ModuleScript", "Shared")),
            )
            .with_child(Snapshot::new("ModuleScript", "Loose"));

        let (dom, warnings) = build_place_dom(&project);
        assert!(warnings.is_empty());

        let root = dom.get_by_ref(dom.root_ref()).unwrap();
        assert_eq!(root.class, "DataModel");

        let service_classes: Vec<&str> = root
            .children()
            .iter()
            .map(|r| dom.get_by_ref(*r).unwrap().class.as_str())
            .collect();
        assert!(service_classes.contains(&"ReplicatedStorage"));
        assert!(service_classes.contains(&"Workspace"));
    }

    #[test]
    fn typed_properties_survive_snapshot_place_snapshot_with_value_identity() {
        // Stage 10 criterion 3 (the place-file leg): real typed properties round-trip through the
        // binary place format with their exact values — not just frontmatter string parsing.
        let mut attributes = Attributes::new();
        attributes.insert("Health".to_string(), Variant::Float64(100.0));
        attributes.insert("Boss".to_string(), Variant::Bool(true));

        let part = Snapshot::new("Part", "Block")
            .with_property("Size", Variant::Vector3(Vector3::new(4.0, 1.0, 2.0)))
            .with_property("Material", Variant::Enum(Enum::from_u32(256)))
            .with_property("Attributes", Variant::Attributes(attributes.clone()))
            .with_property(
                "Tags",
                Variant::Tags(Tags::from(vec!["combat".to_string(), "npc".to_string()])),
            );
        // `Color3` quantizes to `Color3uint8` on a `BasePart`, so carry it where it stays a true
        // `Color3` (a light's `Color`) to assert exact identity.
        let light = Snapshot::new("PointLight", "Glow")
            .with_property("Color", Variant::Color3(Color3::new(0.1, 0.5, 0.9)));
        let project = Snapshot::new("Folder", "proj")
            .with_child(part)
            .with_child(light);

        let mut bytes = Vec::new();
        write_place(&mut bytes, &project, ModelFormat::Binary).unwrap();
        let dom = rbx_binary::from_reader(&bytes[..]).unwrap();

        let workspace = dom
            .root()
            .children()
            .iter()
            .map(|r| dom.get_by_ref(*r).unwrap())
            .find(|i| i.class == "Workspace")
            .expect("Workspace");
        let by_name = |name: &str| {
            workspace
                .children()
                .iter()
                .map(|r| dom.get_by_ref(*r).unwrap())
                .find(|i| i.name == name)
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        let block = by_name("Block");
        let glow = by_name("Glow");

        assert_eq!(
            prop(block, "Size"),
            Some(&Variant::Vector3(Vector3::new(4.0, 1.0, 2.0)))
        );
        assert_eq!(
            prop(block, "Material"),
            Some(&Variant::Enum(Enum::from_u32(256)))
        );
        assert_eq!(
            prop(glow, "Color"),
            Some(&Variant::Color3(Color3::new(0.1, 0.5, 0.9)))
        );
        match prop(block, "Attributes") {
            Some(Variant::Attributes(round_tripped)) => {
                assert_eq!(round_tripped.get("Health"), Some(&Variant::Float64(100.0)));
                assert_eq!(round_tripped.get("Boss"), Some(&Variant::Bool(true)));
            }
            other => panic!("Attributes did not round-trip: {other:?}"),
        }
        match prop(block, "Tags") {
            Some(Variant::Tags(tags)) => {
                let members: Vec<&str> = tags.iter().collect();
                assert_eq!(members, vec!["combat", "npc"]);
            }
            other => panic!("Tags did not round-trip: {other:?}"),
        }
    }
}
