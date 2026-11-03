use std::collections::BTreeMap;
use std::path::Path;

use naht_core::reconciler::TextInstance;
use rbx_dom_weak::types::Variant;
use rbx_dom_weak::{Instance, WeakDom};
use std::fs;

pub(crate) fn read_place(path: &Path) -> WeakDom {
    rbx_binary::from_reader(fs::read(path).unwrap().as_slice()).unwrap()
}

pub(crate) fn root_classes(dom: &WeakDom) -> Vec<String> {
    dom.root()
        .children()
        .iter()
        .map(|reference| {
            dom.get_by_ref(*reference)
                .unwrap()
                .class
                .as_str()
                .to_owned()
        })
        .collect()
}

pub(crate) fn service<'a>(dom: &'a WeakDom, class: &str) -> Option<&'a Instance> {
    dom.root()
        .children()
        .iter()
        .map(|reference| dom.get_by_ref(*reference).unwrap())
        .find(|instance| instance.class == class)
}

pub(crate) fn child<'a>(dom: &'a WeakDom, instance: &Instance, name: &str) -> Option<&'a Instance> {
    instance
        .children()
        .iter()
        .map(|reference| dom.get_by_ref(*reference).unwrap())
        .find(|child| child.name == name)
}

pub(crate) fn property<'a>(instance: &'a Instance, key: &str) -> Option<&'a Variant> {
    instance
        .properties
        .iter()
        .find(|(property_name, _)| property_name.as_str() == key)
        .map(|(_, property_value)| property_value)
}

pub(crate) fn one(path: &str, content: &str) -> BTreeMap<String, TextInstance> {
    let mut map = BTreeMap::new();
    map.insert(
        path.to_owned(),
        TextInstance::new(path, "ModuleScript", content),
    );
    map
}
