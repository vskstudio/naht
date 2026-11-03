use std::collections::{BTreeMap, BTreeSet};

use super::model::TextInstance;
use crate::state::InstanceRecord;

pub(crate) trait Classed {
    fn class_name(&self) -> &str;
}

impl Classed for TextInstance {
    fn class_name(&self) -> &str {
        &self.class
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Side {
    Fs,
    Studio,
}

pub(crate) fn class_of<Instance: Classed>(
    fs: Option<&Instance>,
    studio: Option<&Instance>,
    base: Option<&InstanceRecord>,
) -> String {
    fs.or(studio)
        .map(|current| current.class_name().to_owned())
        .or_else(|| base.map(|base_entry| base_entry.class.clone()))
        .unwrap_or_default()
}

pub(crate) fn union_keys<'a, FsValue, StudioValue, BaseValue>(
    fs: &'a BTreeMap<String, FsValue>,
    studio: &'a BTreeMap<String, StudioValue>,
    base: &'a BTreeMap<String, BaseValue>,
) -> BTreeSet<&'a String> {
    let mut keys = BTreeSet::new();
    keys.extend(fs.keys());
    keys.extend(studio.keys());
    keys.extend(base.keys());
    keys
}
