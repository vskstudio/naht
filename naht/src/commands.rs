//! The user-facing commands behind the CLI. Each is a thin orchestration over `naht-core`: the
//! sync brain, the state store, and the model builder. The binary's `main` parses arguments and
//! dispatches here; the integration tests drive these functions directly.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use naht_core::build::{self, ModelFormat};
use naht_core::snapshot::Snapshot;
use naht_core::state::StateStore;
use naht_core::vfs::{DiskVfs, RootedVfs};
use naht_core::{limits, mapper, reconciler};
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use rbx_dom_weak::types::Variant;
use serde::Deserialize;

use crate::config::{Config, PROJECT_FILE};
use crate::server::AppState;
use crate::session::Session;

/// How long a served long-poll is held open before returning empty.
const LONG_POLL: Duration = Duration::from_secs(25);

/// How long to coalesce filesystem events before a `--watch` rebuild.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(200);

/// Naht's internal directory, holding the state database; never part of a build.
const INTERNAL_DIR: &str = ".naht";

/// Scaffold a new project at `path`, or convert an existing Rojo project there when `from_rojo`.
pub fn init(path: &Path, from_rojo: bool) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))?;
    // Resolve the real directory so a default `.` names the project after the actual folder, not
    // the literal ".".
    let root = canonical(path)?;
    let project_file = root.join(PROJECT_FILE);
    if project_file.exists() {
        bail!(
            "{} already exists; refusing to overwrite",
            project_file.display()
        );
    }

    if from_rojo {
        let warnings = migrate_from_rojo(&root)?;
        for warning in &warnings {
            eprintln!("  warning: {warning}");
        }
        println!("Converted Rojo project into {}", project_file.display());
    } else {
        let name = dir_name(&root);
        std::fs::write(&project_file, render_rojo_config(&name, None, &[])?)
            .with_context(|| format!("writing {}", project_file.display()))?;
        scaffold_source(&root)?;
        ensure_gitignore(&root)?;
        println!("Initialized Naht project '{name}' at {}", root.display());
    }
    Ok(())
}

/// Print the project's conflict state: the paths frozen pending `resolve`, or that it is clean.
pub fn status(root: &Path) -> Result<()> {
    let store = open_store(root)?;
    let status = reconciler::status(&store)?;
    if status.conflicted.is_empty() {
        println!("clean — no conflicted paths");
    } else {
        println!("{} conflicted path(s):", status.conflicted.len());
        for path in &status.conflicted {
            println!("  {path}");
        }
    }
    Ok(())
}

/// Clear a conflict at `path` once its markers have been removed by hand.
pub fn resolve(root: &Path, path: &str) -> Result<()> {
    let store = open_store(root)?;
    let vfs = rooted(root)?;
    reconciler::resolve(&vfs, &store, path)?;
    println!("resolved {path}");
    Ok(())
}

/// Build the project at `root` into `output` once. The output extension picks the artifact:
/// `.rbxl`/`.rbxlx` build a **place** (a `DataModel` with convention-mapped services), anything else
/// a **model**; `.rbxmx`/`.rbxlx` are XML, the rest binary.
pub fn build(root: &Path, output: &Path) -> Result<()> {
    build_once(&canonical(root)?, output)
}

/// Build once, then rebuild on every debounced change until interrupted (Stage 9 `--watch`).
pub async fn build_watch(root: &Path, output: &Path) -> Result<()> {
    let root = canonical(root)?;
    build_once(&root, output)?;
    let _watcher = spawn_build_watcher(&root, output)?;
    tracing::info!(target: "naht::build", root = %root.display(), "watching for changes");
    println!("watching {} — rebuilding on change", root.display());
    std::future::pending::<()>().await;
    Ok(())
}

/// The single build step shared by `build` and `build --watch`. `root` must already be canonical.
fn build_once(root: &Path, output: &Path) -> Result<()> {
    let config = Config::load(root)?;
    let (mut snapshot, warnings) = assemble_project_snapshot(root, &config)?;
    for warning in &warnings {
        eprintln!("naht: warning: {warning}");
    }
    warn_unsyncable(&snapshot);
    resolve_assets(root, &mut snapshot)?;

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    let file =
        std::fs::File::create(output).with_context(|| format!("creating {}", output.display()))?;
    let writer = std::io::BufWriter::new(file);
    let format = model_format(output);
    if is_place_output(output) {
        for warning in build::write_place(writer, &snapshot, format)? {
            eprintln!("naht: warning: {warning}");
        }
    } else {
        build::write_model(writer, &snapshot, format)?;
    }
    tracing::info!(target: "naht::build", output = %output.display(), "built");
    println!("built {}", output.display());
    Ok(())
}

/// Ask a running daemon to re-sync now (Studio ↔ filesystem). Full on-demand Studio → filesystem
/// pull lands with the Stage 6 plugin; for now this nudges the daemon's reconnect re-diff.
pub async fn pull(config: &Config) -> Result<()> {
    let url = format!("http://{}:{}/info", Ipv4Addr::LOCALHOST, config.port());
    let response = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .with_context(|| format!("connecting to the daemon at {url} (is `naht serve` running?)"))?;
    if !response.status().is_success() {
        bail!("daemon returned {}", response.status());
    }
    println!("pulled — daemon re-synced");
    Ok(())
}

/// Run the sync daemon for the project at `root` on `port`, using `config` for the project name and
/// place guard.
pub async fn serve(config: Config, root: &Path, port: u16) -> Result<()> {
    let root = canonical(root)?;
    // Surface anything that can't round-trip live before the session starts (architecture §9). With
    // terrain sync enabled the daemon drives the blob channel, so terrain is no longer flagged.
    if let Ok(snapshot) = mapper::snapshot_dir(&DiskVfs::new(), &root) {
        report_unsyncable(&serve_warnings(&config, &snapshot));
    }
    let store = open_store(&root)?;
    let session = Session::new(
        root.clone(),
        store,
        config.project_name(&root),
        config.serve.place_id,
    );
    let state = AppState::new(session, LONG_POLL);

    let _watcher =
        crate::watcher::spawn(&root, state.clone()).context("starting the file watcher")?;

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(target: "naht::server", %addr, root = %root.display(), "naht serving");
    println!("naht serving {} on http://{addr}", root.display());
    crate::server::serve(listener, state)
        .await
        .context("serving")
}

// --- helpers -------------------------------------------------------------------------------------

/// When `[assets]` is enabled, upload local asset files referenced by properties and rewrite them to
/// `rbxassetid://…`. Disabled by default, so the common case does no network I/O and leaves every
/// property untouched. Public so the disabled-path no-op is testable at the snapshot level.
pub fn resolve_assets(root: &Path, snapshot: &mut Snapshot) -> Result<()> {
    let config = Config::load(root)?;
    if !config.assets.is_enabled() {
        return Ok(());
    }
    let store = open_store(root)?;
    let uploader = crate::uploader::OpenCloudUploader::from_config(&config.assets)?;
    let vfs = RootedVfs::new(root.to_path_buf(), DiskVfs::new());
    // A failed upload pauses only that asset's path (architecture §8); the rest still resolve, so we
    // report each failure rather than abort the whole build on the first bad asset.
    let failures = naht_core::assets::rewrite_snapshot_assets(&uploader, &store, &vfs, snapshot);
    if !failures.is_empty() {
        eprintln!("naht: {} asset(s) could not be uploaded:", failures.len());
        for failure in &failures {
            eprintln!(
                "  warning: {} ({}) left at its original reference: {}",
                failure.path, failure.property, failure.error
            );
        }
    }
    Ok(())
}

/// The instances that can't round-trip live in a `serve` session, honoring the project's enabled
/// capabilities — notably `[serve] terrain_sync`, which drives the daemon's blob channel and so stops
/// terrain from being flagged. The same options the running session uses, so the warning matches what
/// actually syncs.
#[must_use]
pub fn serve_warnings(config: &Config, snapshot: &Snapshot) -> Vec<limits::Warning> {
    limits::scan_with(
        snapshot,
        limits::Options {
            terrain_sync: config.terrain_sync(),
        },
    )
}

/// Print, to stderr, every instance that can't round-trip live — never silently dropped.
fn warn_unsyncable(snapshot: &Snapshot) {
    report_unsyncable(&limits::scan(snapshot));
}

/// Print a set of unsyncable warnings to stderr, with a count header. Empty is silent.
fn report_unsyncable(warnings: &[limits::Warning]) {
    if warnings.is_empty() {
        return;
    }
    eprintln!("naht: {} item(s) cannot live-sync:", warnings.len());
    for warning in warnings {
        eprintln!("  warning: {}", warning.message());
    }
}

/// A minimal Rojo `default.project.json`, just the fields Naht can carry over. The `tree` is kept as
/// raw JSON and walked during migration, since its shape is recursive and partly free-form.
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

fn read_rojo_project(path: &Path) -> Result<RojoProject> {
    let file = path.join("default.project.json");
    let text =
        std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))
}

/// One migrated instance-tree node, ready to render into a `[[tree]]` entry.
struct MigratedNode {
    instance: String,
    path: Option<String>,
    class: Option<String>,
    properties: std::collections::BTreeMap<String, toml::Value>,
}

/// Migrate the Rojo `default.project.json` at `root` into a `naht.toml`: the project name, the place
/// id, and the instance tree (`$path` / `$className` / `$properties`). Returns the warnings for
/// anything Naht cannot represent — surfaced to the user, never dropped. The user's source files are
/// left untouched; only the `naht.toml` is written. Public so the migration is testable directly.
pub fn migrate_from_rojo(root: &Path) -> Result<Vec<String>> {
    let rojo = read_rojo_project(root)?;
    let mut nodes = Vec::new();
    let mut warnings = Vec::new();
    if let Some(tree) = &rojo.tree {
        collect_tree_nodes(tree, String::new(), &mut nodes, &mut warnings);
    }
    let toml = render_rojo_config(&rojo.name(root), rojo.serve_place_id(), &nodes)?;
    let project_file = root.join(PROJECT_FILE);
    std::fs::write(&project_file, toml)
        .with_context(|| format!("writing {}", project_file.display()))?;
    Ok(warnings)
}

/// Walk a Rojo tree node, accumulating the instance path, and emit a [`MigratedNode`] for anything
/// the convention cannot infer (a `$path`, a non-inferable `$className`, or `$properties`).
fn collect_tree_nodes(
    node: &serde_json::Value,
    instance: String,
    out: &mut Vec<MigratedNode>,
    warnings: &mut Vec<String>,
) {
    let Some(object) = node.as_object() else {
        if !instance.is_empty() {
            warnings.push(format!("{instance}: tree node is not an object; skipped"));
        }
        return;
    };

    if !instance.is_empty() {
        let path = object
            .get("$path")
            .and_then(|v| v.as_str())
            .map(String::from);
        let leaf = instance.rsplit('/').next().unwrap_or(&instance);
        // Skip a `$className` the convention already infers: a service named after itself, or a plain
        // directory's `Folder`. Anything else is a genuine override Naht must record.
        let class = match object.get("$className").and_then(|v| v.as_str()) {
            Some(class) if class == leaf || class == "Folder" => None,
            Some(class) => Some(class.to_string()),
            None => None,
        };
        let mut properties = std::collections::BTreeMap::new();
        if let Some(props) = object.get("$properties").and_then(|v| v.as_object()) {
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
        warn_unknown_directives(object, &instance, warnings);
        if path.is_some() || class.is_some() || !properties.is_empty() {
            out.push(MigratedNode {
                instance: instance.clone(),
                path,
                class,
                properties,
            });
        }
    } else {
        warn_unknown_directives(object, "(root)", warnings);
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
        collect_tree_nodes(child, child_instance, out, warnings);
    }
}

/// Warn about Rojo directives Naht does not carry, so nothing is silently dropped.
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

/// Convert a Rojo `$properties` JSON value to a TOML primitive, or `None` for a shape Naht cannot
/// represent (a typed `{ \"Type\", \"Value\" }` block, an array, a nested object).
fn rojo_property_to_toml(value: &serde_json::Value) -> Option<toml::Value> {
    match value {
        serde_json::Value::Bool(b) => Some(toml::Value::Boolean(*b)),
        serde_json::Value::String(s) => Some(toml::Value::String(s.clone())),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| n.as_f64().map(toml::Value::Float)),
        _ => None,
    }
}

/// Render a migrated project to `naht.toml` text: `[project]`, optional `[serve]`, and a `[[tree]]`
/// array for the genuine exceptions.
fn render_rojo_config(name: &str, place_id: Option<u64>, nodes: &[MigratedNode]) -> Result<String> {
    let mut doc = toml::Table::new();

    let mut project = toml::Table::new();
    project.insert("name".to_string(), toml::Value::String(name.to_string()));
    doc.insert("project".to_string(), toml::Value::Table(project));

    if let Some(id) = place_id {
        let mut serve = toml::Table::new();
        serve.insert("place_id".to_string(), toml::Value::Integer(id as i64));
        doc.insert("serve".to_string(), toml::Value::Table(serve));
    }

    if !nodes.is_empty() {
        let entries = nodes
            .iter()
            .map(|node| {
                let mut entry = toml::Table::new();
                entry.insert(
                    "instance".to_string(),
                    toml::Value::String(node.instance.clone()),
                );
                if let Some(path) = &node.path {
                    entry.insert("path".to_string(), toml::Value::String(path.clone()));
                }
                if let Some(class) = &node.class {
                    entry.insert("class".to_string(), toml::Value::String(class.clone()));
                }
                if !node.properties.is_empty() {
                    let props: toml::Table = node.properties.clone().into_iter().collect();
                    entry.insert("properties".to_string(), toml::Value::Table(props));
                }
                toml::Value::Table(entry)
            })
            .collect();
        doc.insert("tree".to_string(), toml::Value::Array(entries));
    }

    toml::to_string(&doc).context("rendering naht.toml")
}

/// Build the project's snapshot, honoring any `[[tree]]` mappings (migrated from Rojo). With no tree
/// config this is the plain convention scan of the project directory (the common case, unchanged).
/// A `$path`-mapped instance is grafted at its instance location from its filesystem source — which
/// is left in place — and any source root it draws from is dropped from the convention scan so it
/// does not also appear as a literal top-level folder. Returns warnings for unrepresentable entries.
fn assemble_project_snapshot(root: &Path, config: &Config) -> Result<(Snapshot, Vec<String>)> {
    let mut snapshot =
        mapper::snapshot_dir(&DiskVfs::new(), root).context("snapshotting the project")?;
    // `.naht` holds internal state, not Roblox source — keep it out of the artifact.
    snapshot.children.retain(|child| child.name != INTERNAL_DIR);
    let mut warnings = Vec::new();

    if config.tree.is_empty() {
        return Ok((snapshot, warnings));
    }

    let source_roots: std::collections::BTreeSet<&str> = config
        .tree
        .iter()
        .filter_map(|node| node.path.as_deref())
        .filter_map(|path| path.split('/').find(|segment| !segment.is_empty()))
        .collect();
    snapshot
        .children
        .retain(|child| !source_roots.contains(child.name.as_str()));

    for node in &config.tree {
        let segments: Vec<&str> = node.instance.split('/').filter(|s| !s.is_empty()).collect();
        let Some((leaf, parents)) = segments.split_last() else {
            warnings.push("a [[tree]] entry has an empty `instance`; skipped".to_string());
            continue;
        };

        let mut built = match &node.path {
            Some(path) => {
                let mut from = mapper::snapshot_dir(&DiskVfs::new(), &root.join(path))
                    .with_context(|| {
                        format!("snapshotting {path} for instance {}", node.instance)
                    })?;
                from.name = (*leaf).to_string();
                from
            }
            None => Snapshot::new(
                node.class.clone().unwrap_or_else(|| "Folder".to_string()),
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

/// Insert `leaf` into `root` at the instance path `parents`, creating intermediate `Folder`s as
/// needed and replacing an existing instance of the same name.
fn graft(root: &mut Snapshot, parents: &[&str], leaf: Snapshot) {
    let mut current = root;
    for segment in parents {
        let index = match current.children.iter().position(|c| c.name == *segment) {
            Some(index) => index,
            None => {
                current.push_child(Snapshot::new("Folder", *segment));
                current
                    .children
                    .iter()
                    .position(|c| c.name == *segment)
                    .expect("child just inserted")
            }
        };
        current = &mut current.children[index];
    }
    if let Some(index) = current.children.iter().position(|c| c.name == leaf.name) {
        current.children[index] = leaf;
        current.children.sort_by(|a, b| a.name.cmp(&b.name));
    } else {
        current.push_child(leaf);
    }
}

/// Convert a `[[tree]]` property's TOML primitive to a Roblox [`Variant`], or `None` for a shape that
/// has no primitive mapping.
fn toml_value_to_variant(value: &toml::Value) -> Option<Variant> {
    match value {
        toml::Value::Boolean(b) => Some(Variant::Bool(*b)),
        toml::Value::Integer(i) => Some(Variant::Int64(*i)),
        toml::Value::Float(f) => Some(Variant::Float64(*f)),
        toml::Value::String(s) => Some(Variant::String(s.clone())),
        _ => None,
    }
}

fn scaffold_source(path: &Path) -> Result<()> {
    let src = path.join("src");
    std::fs::create_dir_all(&src).with_context(|| format!("creating {}", src.display()))?;
    let sample = src.join("Hello.luau");
    std::fs::write(&sample, "-- Naht sample module\nreturn {}\n")
        .with_context(|| format!("writing {}", sample.display()))
}

fn ensure_gitignore(path: &Path) -> Result<()> {
    let gitignore = path.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
    if existing.lines().any(|line| line.trim() == "/.naht") {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str("/.naht\n");
    std::fs::write(&gitignore, updated).with_context(|| format!("writing {}", gitignore.display()))
}

fn model_format(output: &Path) -> ModelFormat {
    match output.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("rbxmx") || ext.eq_ignore_ascii_case("rbxlx") => {
            ModelFormat::Xml
        }
        _ => ModelFormat::Binary,
    }
}

/// Whether `output` is a place file (`.rbxl`/`.rbxlx`) rather than a model.
fn is_place_output(output: &Path) -> bool {
    matches!(
        output.extension().and_then(|ext| ext.to_str()),
        Some(ext) if ext.eq_ignore_ascii_case("rbxl") || ext.eq_ignore_ascii_case("rbxlx")
    )
}

/// Watch `root` and rebuild `output` on each debounced change. The caller holds the returned guard
/// for as long as watching should continue. Our own output writes are ignored, so the rebuild does
/// not trigger itself.
pub fn spawn_build_watcher(root: &Path, output: &Path) -> Result<impl Drop> {
    let root = canonical(root)?;
    let watch_root = root.clone();
    let output = output.to_path_buf();
    let output_abs = output.canonicalize().ok();
    let mut debouncer = new_debouncer(WATCH_DEBOUNCE, None, move |result: DebounceEventResult| {
        let Ok(events) = result else {
            return;
        };
        let changed: Vec<PathBuf> = events
            .iter()
            .flat_map(|event| event.paths.iter().cloned())
            .collect();
        if changed.is_empty() {
            return;
        }
        if let Some(out) = output_abs.as_deref() {
            if changed.iter().all(|path| path == out) {
                return; // our own write — don't rebuild ourselves into a loop
            }
        }
        if let Err(error) = build_once(&root, &output) {
            tracing::warn!(target: "naht::build", %error, "rebuild failed");
        }
    })?;
    debouncer.watch(&watch_root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

fn open_store(root: &Path) -> Result<StateStore> {
    StateStore::open(&root.join(INTERNAL_DIR).join("state.db"))
        .context("opening the state database")
}

fn rooted(root: &Path) -> Result<RootedVfs<DiskVfs>> {
    Ok(RootedVfs::new(canonical(root)?, DiskVfs::new()))
}

fn canonical(root: &Path) -> Result<std::path::PathBuf> {
    root.canonicalize()
        .with_context(|| format!("project directory not found: {}", root.display()))
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("naht")
        .to_string()
}
