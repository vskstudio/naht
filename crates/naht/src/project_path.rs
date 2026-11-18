use std::path::{Component, Path};

use naht_core::vfs::{is_forbidden_name_character, is_windows_device_name};

pub(crate) fn stays_inside_project(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

pub(crate) fn has_portable_names(path: &Path) -> bool {
    path.components().all(|component| match component {
        Component::Normal(name) => name.to_str().is_some_and(is_portable_name),
        Component::CurDir | Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
            true
        }
    })
}

fn is_portable_name(name: &str) -> bool {
    !is_windows_device_name(name) && !name.chars().any(is_forbidden_name_character)
}
