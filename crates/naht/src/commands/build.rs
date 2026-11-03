use super::error::CommandError;
use super::files::{canonical, create_file, create_parent_dirs};
use super::inspect::{resolve_assets, unsyncable_lines};
use super::rojo::assemble_project_snapshot;
use crate::config::Config;
use crate::watcher::WatchError;
use naht_core::build::{self, ModelFormat};
use naht_core::limits;
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use std::io::{self};
use std::path::{Path, PathBuf};
use std::time::Duration;

const WATCH_DEBOUNCE: Duration = Duration::from_millis(200);

/// A finished build: the artifact written and every warning it raised, in the order found.
#[derive(Debug)]
pub struct Built {
    pub output: PathBuf,
    pub warnings: Vec<String>,
}

/// Build the project at `root` into `output` once. The output extension picks the artifact:
/// `.rbxl`/`.rbxlx` build a **place** (a `DataModel` with convention-mapped services), anything else
/// a **model**; `.rbxmx`/`.rbxlx` are XML, the rest binary.
pub fn build(root: &Path, output: &Path) -> Result<Built, CommandError> {
    build_once(&canonical(root)?, output)
}

/// Build once, then rebuild on every debounced change for as long as the returned guard is held.
pub fn build_watch(root: &Path, output: &Path) -> Result<(Built, impl Drop + use<>), CommandError> {
    let root = canonical(root)?;
    let built = build_once(&root, output)?;
    let watcher = spawn_build_watcher(&root, output)?;
    tracing::info!(target: "naht::build", root = %root.display(), "watching for changes");
    Ok((built, watcher))
}

fn build_once(root: &Path, output: &Path) -> Result<Built, CommandError> {
    let config = Config::load(root)?;
    let (mut snapshot, mut warnings) = assemble_project_snapshot(root, &config)?;
    warnings.extend(unsyncable_lines(&limits::scan(&snapshot)));
    warnings.extend(resolve_assets(root, &mut snapshot)?);

    create_parent_dirs(output)?;
    let file = create_file(output)?;
    let writer = io::BufWriter::new(file);
    let format = model_format(output);
    if is_place_output(output) {
        warnings.extend(build::write_place(writer, &snapshot, format)?);
    } else {
        build::write_model(writer, &snapshot, format)?;
    }
    tracing::info!(target: "naht::build", output = %output.display(), "built");
    Ok(Built {
        output: output.to_path_buf(),
        warnings,
    })
}

fn model_format(output: &Path) -> ModelFormat {
    match output.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("rbxmx") || ext.eq_ignore_ascii_case("rbxlx") => {
            ModelFormat::Xml
        }
        _ => ModelFormat::Binary,
    }
}

fn is_place_output(output: &Path) -> bool {
    matches!(
        output.extension().and_then(|ext| ext.to_str()),
        Some(ext) if ext.eq_ignore_ascii_case("rbxl") || ext.eq_ignore_ascii_case("rbxlx")
    )
}

/// Watch `root` and rebuild `output` on each debounced change. The caller holds the returned guard
/// for as long as watching should continue. Our own output writes are ignored, so the rebuild does
/// not trigger itself.
pub fn spawn_build_watcher(root: &Path, output: &Path) -> Result<impl Drop + use<>, CommandError> {
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
        if let Some(out) = output_abs.as_deref()
            && changed.iter().all(|path| path == out)
        {
            return;
        }
        match build_once(&root, &output) {
            Ok(built) => {
                for warning in &built.warnings {
                    tracing::warn!(target: "naht::build", %warning, "build warning");
                }
            }
            Err(error) => tracing::warn!(target: "naht::build", %error, "rebuild failed"),
        }
    })
    .map_err(WatchError::from)?;
    debouncer
        .watch(&watch_root, RecursiveMode::Recursive)
        .map_err(WatchError::from)?;
    Ok(debouncer)
}
