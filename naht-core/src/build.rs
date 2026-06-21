//! Serialize a [`Snapshot`] tree to a Roblox model file via the `rbx-dom` ecosystem.
//!
//! The reconciler works on the lightweight, comparable [`Snapshot`] tree; building a distributable
//! artifact is a separate edge concern (architecture §6: build config is independent of sync
//! mapping). Here a snapshot is converted into a [`WeakDom`] and written as binary `rbxm` or XML
//! `rbxmx`. We depend on `rbx_binary`/`rbx_xml` rather than hand-rolling the format.

use std::io::Write;

use rbx_dom_weak::{InstanceBuilder, WeakDom};

use crate::snapshot::Snapshot;

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
}
