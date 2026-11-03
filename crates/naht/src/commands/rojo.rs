use super::error::CommandError;
use super::files::{dir_name, write_file};
use crate::config::{Config, INTERNAL_DIR, PROJECT_FILE};
use naht_core::mapper;
use naht_core::snapshot::Snapshot;
use naht_core::vfs::DiskVfs;
use rbx_dom_weak::types::Variant;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct RojoProject {
    name: Option<String>,
    #[serde(rename = "servePlaceIds")]
    serve_place_ids: Option<Vec<u64>>,
    tree: Option<serde_json::Value>,
}

impl RojoProject {
    fn name(&self, path: &Path) -> String {
        self.name.clone().unwrap_or_else(|| dir_name(path))
    }

    fn serve_place_id(&self) -> Option<u64> {
        self.serve_place_ids
            .as_ref()
            .and_then(|ids| ids.first().copied())
    }
}

fn read_rojo_project(path: &Path) -> Result<RojoProject, CommandError> {
    let file = path.join("default.project.json");
    let text = fs::read_to_string(&file).map_err(|source| CommandError::Read {
        path: file.clone(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| CommandError::RojoParse { path: file, source })
}

pub(super) struct MigratedNode {
    instance: String,
    path: Option<String>,
    class: Option<String>,
    properties: BTreeMap<String, toml::Value>,
}

/// Migrate the Rojo `default.project.json` at `root` into a `naht.toml`.
///
/// Carries over the project name, the place id, and the instance tree (`$path` / `$className` /
/// `$properties`). Returns the warnings for anything Naht cannot represent, surfaced to the user,
/// never dropped. The user's source files are left untouched; only the `naht.toml` is written.
/// Public so the migration is testable directly.
///
/// # Errors
///
/// Returns [`CommandError`] when the Rojo project cannot be read or migrated.
pub fn migrate_from_rojo(root: &Path) -> Result<Vec<String>, CommandError> {
    let rojo = read_rojo_project(root)?;
    let mut nodes = Vec::new();
    let mut warnings = Vec::new();
    if let Some(tree) = &rojo.tree {
        collect_tree_nodes(tree, "", &mut nodes, &mut warnings);
    }
    let toml = render_rojo_config(&rojo.name(root), rojo.serve_place_id(), &nodes)?;
    write_file(&root.join(PROJECT_FILE), &toml)?;
    Ok(warnings)
}

fn collect_tree_nodes(
    node: &serde_json::Value,
    instance: &str,
    out: &mut Vec<MigratedNode>,
    warnings: &mut Vec<String>,
) {
    let Some(object) = node.as_object() else {
        if !instance.is_empty() {
            warnings.push(format!("{instance}: tree node is not an object; skipped"));
        }
        return;
    };

    if instance.is_empty() {
        warn_unknown_directives(object, "(root)", warnings);
    } else {
        let path = object
            .get("$path")
            .and_then(|value| value.as_str())
            .map(String::from);
        let leaf = instance.rsplit('/').next().unwrap_or(instance);
        let class = match object.get("$className").and_then(|value| value.as_str()) {
            Some(class) if class == leaf || class == "Folder" => None,
            Some(class) => Some(class.to_owned()),
            None => None,
        };
        let mut properties = BTreeMap::new();
        if let Some(props) = object
            .get("$properties")
            .and_then(|value| value.as_object())
        {
            for (key, value) in props {
                match rojo_property_to_toml(value) {
                    Some(value) => {
                        properties.insert(key.clone(), value);
                    }
                    None => warnings.push(format!(
                        "{instance}.{key}: property type cannot be represented by Naht; skipped"
                    )),
                }
            }
        }
        warn_unknown_directives(object, instance, warnings);
        if path.is_some() || class.is_some() || !properties.is_empty() {
            out.push(MigratedNode {
                instance: instance.to_owned(),
                path,
                class,
                properties,
            });
        }
    }

    for (key, child) in object {
        if key.starts_with('$') {
            continue;
        }
        let child_instance = if instance.is_empty() {
            key.clone()
        } else {
            format!("{instance}/{key}")
        };
        collect_tree_nodes(child, &child_instance, out, warnings);
    }
}

fn warn_unknown_directives(
    object: &serde_json::Map<String, serde_json::Value>,
    instance: &str,
    warnings: &mut Vec<String>,
) {
    for key in object.keys() {
        if key.starts_with('$') && !matches!(key.as_str(), "$path" | "$className" | "$properties") {
            warnings.push(format!(
                "{instance}: Rojo directive `{key}` is not supported; ignored"
            ));
        }
    }
}

fn rojo_property_to_toml(value: &serde_json::Value) -> Option<toml::Value> {
    match value {
        serde_json::Value::Bool(flag) => Some(toml::Value::Boolean(*flag)),
        serde_json::Value::String(text) => Some(toml::Value::String(text.clone())),
        serde_json::Value::Number(number) => number
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| number.as_f64().map(toml::Value::Float)),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            None
        }
    }
}

pub(super) fn render_rojo_config(
    name: &str,
    place_id: Option<u64>,
    nodes: &[MigratedNode],
) -> Result<String, CommandError> {
    let mut doc = toml::Table::new();

    let mut project = toml::Table::new();
    project.insert("name".to_owned(), toml::Value::String(name.to_owned()));
    doc.insert("project".to_owned(), toml::Value::Table(project));

    if let Some(id) = place_id {
        let mut serve = toml::Table::new();
        serve.insert(
            "place_id".to_owned(),
            toml::Value::try_from(id).map_err(CommandError::RenderConfig)?,
        );
        doc.insert("serve".to_owned(), toml::Value::Table(serve));
    }

    if !nodes.is_empty() {
        let entries = nodes
            .iter()
            .map(|node| {
                let mut entry = toml::Table::new();
                entry.insert(
                    "instance".to_owned(),
                    toml::Value::String(node.instance.clone()),
                );
                if let Some(path) = &node.path {
                    entry.insert("path".to_owned(), toml::Value::String(path.clone()));
                }
                if let Some(class) = &node.class {
                    entry.insert("class".to_owned(), toml::Value::String(class.clone()));
                }
                if !node.properties.is_empty() {
                    let props: toml::Table = node.properties.clone().into_iter().collect();
                    entry.insert("properties".to_owned(), toml::Value::Table(props));
                }
                toml::Value::Table(entry)
            })
            .collect();
        doc.insert("tree".to_owned(), toml::Value::Array(entries));
    }

    toml::to_string(&doc).map_err(CommandError::RenderConfig)
}

pub(super) fn assemble_project_snapshot(
    root: &Path,
    config: &Config,
) -> Result<(Snapshot, Vec<String>), CommandError> {
    let mut snapshot =
        mapper::snapshot_dir(&DiskVfs::new(), root).map_err(|source| CommandError::Tree {
            path: root.to_path_buf(),
            source,
        })?;
    snapshot.children.retain(|child| child.name != INTERNAL_DIR);
    let mut warnings = Vec::new();

    if config.tree.is_empty() {
        return Ok((snapshot, warnings));
    }

    let source_roots: BTreeSet<&str> = config
        .tree
        .iter()
        .filter_map(|node| node.path.as_deref())
        .filter_map(|path| path.split('/').find(|segment| !segment.is_empty()))
        .collect();
    snapshot
        .children
        .retain(|child| !source_roots.contains(child.name.as_str()));

    for node in &config.tree {
        let segments: Vec<&str> = node
            .instance
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        let Some((leaf, parents)) = segments.split_last() else {
            warnings.push("a [[tree]] entry has an empty `instance`; skipped".to_owned());
            continue;
        };

        let mut built = match &node.path {
            Some(path) => {
                let source_dir = root.join(path);
                let mut from =
                    mapper::snapshot_dir(&DiskVfs::new(), &source_dir).map_err(|source| {
                        CommandError::Tree {
                            path: source_dir,
                            source,
                        }
                    })?;
                from.name = String::from(*leaf);
                from
            }
            None => Snapshot::new(
                node.class.clone().unwrap_or_else(|| "Folder".to_owned()),
                *leaf,
            ),
        };
        if let Some(class) = &node.class {
            built.class.clone_from(class);
        }
        for (key, value) in &node.properties {
            match toml_value_to_variant(value) {
                Some(variant) => {
                    built.properties.insert(key.clone(), variant);
                }
                None => warnings.push(format!(
                    "{}.{key}: unsupported property value; skipped",
                    node.instance
                )),
            }
        }
        graft(&mut snapshot, parents, built);
    }

    Ok((snapshot, warnings))
}

fn graft(parent: &mut Snapshot, parents: &[&str], leaf: Snapshot) {
    let Some((segment, rest)) = parents.split_first() else {
        place_leaf(parent, leaf);
        return;
    };
    if let Some(folder) = parent
        .children
        .iter_mut()
        .find(|child| child.name == *segment)
    {
        graft(folder, rest, leaf);
    } else {
        parent.push_child(Snapshot::new("Folder", *segment));
        graft(parent, parents, leaf);
    }
}

fn place_leaf(parent: &mut Snapshot, leaf: Snapshot) {
    if let Some(existing) = parent
        .children
        .iter_mut()
        .find(|child| child.name == leaf.name)
    {
        *existing = leaf;
    } else {
        parent.push_child(leaf);
    }
}

fn toml_value_to_variant(value: &toml::Value) -> Option<Variant> {
    match value {
        toml::Value::Boolean(flag) => Some(Variant::Bool(*flag)),
        toml::Value::Integer(number) => Some(Variant::Int64(*number)),
        toml::Value::Float(f) => Some(Variant::Float64(*f)),
        toml::Value::String(text) => Some(Variant::String(text.clone())),
        toml::Value::Datetime(_) | toml::Value::Array(_) | toml::Value::Table(_) => None,
    }
}
