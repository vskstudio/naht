//! The user-facing commands behind the CLI. Each is a thin orchestration over `naht-core`: the
//! sync brain, the state store, and the model builder. The binary's `main` parses arguments and
//! dispatches here; the integration tests drive these functions directly.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use naht_core::build::{self, ModelFormat};
use naht_core::snapshot::Snapshot;
use naht_core::state::StateStore;
use naht_core::vfs::{DiskVfs, RootedVfs};
use naht_core::{limits, mapper, reconciler};
use serde::Deserialize;

use crate::config::{Config, PROJECT_FILE};
use crate::server::AppState;
use crate::session::Session;

/// How long a served long-poll is held open before returning empty.
const LONG_POLL: Duration = Duration::from_secs(25);

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
        let rojo = read_rojo_project(&root)?;
        std::fs::write(
            &project_file,
            render_config(&rojo.name(&root), rojo.serve_place_id()),
        )
        .with_context(|| format!("writing {}", project_file.display()))?;
        println!("Converted Rojo project into {}", project_file.display());
    } else {
        let name = dir_name(&root);
        std::fs::write(&project_file, render_config(&name, None))
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

/// Build the project at `root` into a model file at `output`, choosing the format by extension
/// (`.rbxmx`/`.rbxlx` → XML, otherwise binary).
pub fn build(root: &Path, output: &Path) -> Result<()> {
    let root = canonical(root)?;
    let mut snapshot =
        mapper::snapshot_dir(&DiskVfs::new(), &root).context("snapshotting the project")?;
    // `.naht` holds internal state, not Roblox source — keep it out of the artifact.
    snapshot.children.retain(|child| child.name != INTERNAL_DIR);
    warn_unsyncable(&snapshot);

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
    }
    let file =
        std::fs::File::create(output).with_context(|| format!("creating {}", output.display()))?;
    build::write_model(
        std::io::BufWriter::new(file),
        &snapshot,
        model_format(output),
    )?;
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

/// Run the sync daemon for the project at `root`, using `config` for the port and place guard.
pub async fn serve(config: Config, root: &Path) -> Result<()> {
    let root = canonical(root)?;
    // Surface anything that can't round-trip live before the session starts (architecture §9).
    if let Ok(snapshot) = mapper::snapshot_dir(&DiskVfs::new(), &root) {
        warn_unsyncable(&snapshot);
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

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, config.port()));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    println!("naht serving {} on http://{addr}", root.display());
    crate::server::serve(listener, state)
        .await
        .context("serving")
}

// --- helpers -------------------------------------------------------------------------------------

/// Print, to stderr, every instance that can't round-trip live — never silently dropped.
fn warn_unsyncable(snapshot: &Snapshot) {
    let warnings = limits::scan(snapshot);
    if warnings.is_empty() {
        return;
    }
    eprintln!("naht: {} item(s) cannot live-sync:", warnings.len());
    for warning in &warnings {
        eprintln!("  warning: {}", warning.message());
    }
}

/// A minimal Rojo `default.project.json`, just the fields Naht can carry over.
#[derive(Debug, Deserialize)]
struct RojoProject {
    name: Option<String>,
    #[serde(rename = "servePlaceIds")]
    serve_place_ids: Option<Vec<u64>>,
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

fn render_config(name: &str, place_id: Option<u64>) -> String {
    let mut out = format!("[project]\nname = \"{name}\"\n");
    if let Some(id) = place_id {
        out.push_str(&format!("\n[serve]\nplace_id = {id}\n"));
    }
    out
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
