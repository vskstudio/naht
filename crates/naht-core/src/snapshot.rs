//! The instance snapshot tree — the in-memory shape a file tree maps to and from.

use std::collections::BTreeMap;

use rbx_dom_weak::types::Variant;

/// The property name that carries a script's body. The file contents live here; everything else
/// in a script's [`Snapshot::properties`] is inline frontmatter.
pub const SOURCE_PROPERTY: &str = "Source";

/// A lightweight, comparable instance tree.
///
/// This is deliberately not a [`rbx_dom_weak::WeakDom`]: the reconciler diffs *shapes*, and an
/// owned, ordered tree (`BTreeMap` properties, `Vec` children sorted by name) makes equality and
/// round-trip assertions exact. Conversion to a `WeakDom` happens at the `build`/serialize edges.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The instance name (the `Name` property).
    pub name: String,
    /// The Roblox class name, e.g. `Script`, `ModuleScript`, `Folder`.
    pub class: String,
    /// Non-`Name` properties, ordered for deterministic comparison and serialization.
    pub properties: BTreeMap<String, Variant>,
    /// Child instances, kept sorted by name.
    pub children: Vec<Snapshot>,
}

impl Snapshot {
    /// Create a leaf snapshot of the given class and name, with no properties or children.
    pub fn new(class: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            class: class.into(),
            properties: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    /// Builder: set a property.
    #[must_use]
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<Variant>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Builder: add a child, keeping children sorted by name.
    #[must_use]
    pub fn with_child(mut self, child: Snapshot) -> Self {
        self.push_child(child);
        self
    }

    /// Add a child, keeping children sorted by name.
    pub fn push_child(&mut self, child: Snapshot) {
        self.children.push(child);
        self.children.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// The script body, if this instance carries one.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        match self.properties.get(SOURCE_PROPERTY) {
            Some(Variant::String(s)) => Some(s),
            _ => None,
        }
    }
}
