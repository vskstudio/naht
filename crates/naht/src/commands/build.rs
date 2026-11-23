use super::error::CommandError;
use super::files::{canonical, create_file, create_parent_dirs};
use super::inspect::{resolve_assets, unsyncable_lines};
use super::rojo::assemble_project_snapshot;
use crate::config::Config;
use crate::watcher::{WatchError, is_project_event};
use naht_core::build::{self, ModelFormat};
use naht_core::limits;
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, DebouncedEvent, new_debouncer};
use std::io::{self};
use std::path::{Path, PathBuf};
use std::time::Duration;

const WATCH_DEBOUNCE: Duration = Duration::from_millis(200);

/// A finished build: the artifact written and every warning it raised, in the order found.
#[derive(Debug)]
pub struct Built {
    /// The artifact that was written.
    pub output: PathBuf,
    /// Every warning the build raised, in the order found.
    pub warnings: Vec<String>,
}

/// Build the project at `root` into `output` once.
///
/// The output extension picks the artifact: `.rbxl`/`.rbxlx` build a **place** (a `DataModel` with
/// convention-mapped services), anything else a **model**; `.rbxmx`/`.rbxlx` are XML, the rest
/// binary.
///
/// # Errors
///
/// Returns [`CommandError`] when the project cannot be read or the output cannot be written.
pub fn build(root: &Path, output: &Path) -> Result<Built, CommandError> {
    build_once(&canonical(root)?, output)
}

/// Build once, then rebuild on every debounced change for as long as the returned guard is held.
///
/// # Errors
///
/// Returns [`CommandError`] when the project cannot be built or the watcher cannot start.
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

/// Watch `root` and rebuild `output` on each debounced change.
///
/// The caller holds the returned guard for as long as watching should continue. Our own output
/// writes are ignored, so the rebuild does not trigger itself.
///
/// # Errors
///
/// Returns [`CommandError`] when the watcher cannot be started.
pub fn spawn_build_watcher(root: &Path, output: &Path) -> Result<impl Drop + use<>, CommandError> {
    let root = canonical(root)?;
    let watch_root = root.clone();
    let output = output.to_path_buf();
    let output_abs = output.canonicalize().ok();
    let mut debouncer = new_debouncer(WATCH_DEBOUNCE, None, move |result: DebounceEventResult| {
        let Ok(events) = result else {
            return;
        };
        let changed = changed_project_paths(&root, &events);
        if changed.is_empty() {
            return;
        }
        let only_own_output = output_abs
            .as_deref()
            .is_some_and(|out| changed.iter().all(|path| path == out));
        if only_own_output {
            return;
        }
        match build_once(&root, &output) {
            Ok(built) => {
                for warning in &built.warnings {
                    tracing::warn!(target: "naht::build", ?warning, "build warning");
                }
            }
            Err(error) => tracing::warn!(target: "naht::build", ?error, "rebuild failed"),
        }
    })
    .map_err(WatchError::from)?;
    debouncer
        .watch(&watch_root, RecursiveMode::Recursive)
        .map_err(WatchError::from)?;
    Ok(debouncer)
}

fn changed_project_paths(root: &Path, events: &[DebouncedEvent]) -> Vec<PathBuf> {
    events
        .iter()
        .filter(|event| is_project_event(root, event))
        .flat_map(|event| event.paths.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use notify_debouncer_full::notify::{Event, EventKind};

    use super::*;

    fn event_at(path: &str) -> DebouncedEvent {
        let event = Event::new(EventKind::Any).add_path(PathBuf::from(path));
        DebouncedEvent::new(event, Instant::now())
    }

    #[test]
    fn a_build_watch_batch_keeps_project_paths_and_drops_ignored_ones() {
        let events = [
            event_at("/project/src/Main.luau"),
            event_at("/project/node_modules/pkg/index.js"),
            event_at("/project/.naht/state.db-wal"),
            event_at("/project/target/debug/naht"),
        ];
        assert_eq!(
            changed_project_paths(Path::new("/project"), &events),
            vec![PathBuf::from("/project/src/Main.luau")]
        );
    }
}
