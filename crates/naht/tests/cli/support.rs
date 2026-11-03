use std::collections::BTreeMap;
use std::path::Path;

use naht_core::reconciler::TextInstance;
use rbx_dom_weak::types::Variant;
use rbx_dom_weak::{Instance, WeakDom};

pub fn read_place(path: &Path) -> WeakDom {
    rbx_binary::from_reader(&std::fs::read(path).unwrap()[..]).unwrap()
}

pub fn root_classes(dom: &WeakDom) -> Vec<String> {
    dom.root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap().class.as_str().to_string())
        .collect()
}

pub fn service<'a>(dom: &'a WeakDom, class: &str) -> Option<&'a Instance> {
    dom.root()
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .find(|instance| instance.class == class)
}

pub fn child<'a>(dom: &'a WeakDom, instance: &Instance, name: &str) -> Option<&'a Instance> {
    instance
        .children()
        .iter()
        .map(|r| dom.get_by_ref(*r).unwrap())
        .find(|child| child.name == name)
}

pub fn property<'a>(instance: &'a Instance, key: &str) -> Option<&'a Variant> {
    instance
        .properties
        .iter()
        .find(|(k, _)| k.as_str() == key)
        .map(|(_, v)| v)
}

pub fn one(path: &str, content: &str) -> BTreeMap<String, TextInstance> {
    let mut map = BTreeMap::new();
    map.insert(
        path.to_string(),
        TextInstance::new(path, "ModuleScript", content),
    );
    map
}
