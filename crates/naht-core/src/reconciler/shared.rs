use std::collections::{BTreeMap, BTreeSet};

use std::path::Path;

use super::model::{Direction, TextInstance};
use crate::state::InstanceRecord;
use crate::vfs::{Vfs, VfsError};

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

impl Side {
    pub(crate) fn directions(self) -> (Direction, Direction) {
        match self {
            Self::Fs => (Direction::ToStudio, Direction::ToFs),
            Self::Studio => (Direction::ToFs, Direction::ToStudio),
        }
    }
}

pub(crate) fn write_toward<V: Vfs>(
    vfs: &mut V,
    path: &str,
    content: &[u8],
    toward: Direction,
) -> Result<(), VfsError> {
    if toward == Direction::ToFs {
        vfs.write(Path::new(path), content)?;
    }
    Ok(())
}

pub(crate) fn remove_fs_copy<V: Vfs>(
    vfs: &mut V,
    path: &str,
    present: Side,
) -> Result<(), VfsError> {
    if matches!(present, Side::Fs) {
        vfs.remove(Path::new(path))?;
    }
    Ok(())
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
