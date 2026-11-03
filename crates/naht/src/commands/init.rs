use super::error::CommandError;
use super::files::{canonical, dir_name, read_optional, write_file};
use super::rojo::{migrate_from_rojo, render_rojo_config};
use crate::config::PROJECT_FILE;
use std::fs;
use std::path::{Path, PathBuf};

/// What `init` created, or what it converted, for the caller to report.
#[derive(Debug)]
pub enum Initialized {
    /// A Rojo project was converted in place.
    Rojo {
        /// The Naht project file written in place of the Rojo one.
        project_file: PathBuf,
        /// Anything the conversion could not carry over, in the order found.
        warnings: Vec<String>,
    },
    /// A fresh project was scaffolded.
    Fresh {
        /// The project name, taken from the directory unless the config names it.
        name: String,
        /// The directory the project was created in.
        root: PathBuf,
    },
}

/// Scaffold a new project at `path`, or convert an existing Rojo project there when `from_rojo`.
///
/// # Errors
///
/// Returns [`CommandError`] when the project files cannot be written.
pub fn init(path: &Path, from_rojo: bool) -> Result<Initialized, CommandError> {
    fs::create_dir_all(path).map_err(|source| CommandError::CreateDir {
        path: path.to_path_buf(),
        source,
    })?;
    let root = canonical(path)?;
    let project_file = root.join(PROJECT_FILE);
    if project_file.exists() {
        return Err(CommandError::ProjectExists { path: project_file });
    }

    if from_rojo {
        let warnings = migrate_from_rojo(&root)?;
        Ok(Initialized::Rojo {
            project_file,
            warnings,
        })
    } else {
        let name = dir_name(&root);
        write_file(&project_file, &render_rojo_config(&name, None, &[])?)?;
        scaffold_source(&root)?;
        ensure_gitignore(&root)?;
        Ok(Initialized::Fresh { name, root })
    }
}

fn scaffold_source(path: &Path) -> Result<(), CommandError> {
    let src = path.join("src");
    fs::create_dir_all(&src).map_err(|source| CommandError::CreateDir {
        path: src.clone(),
        source,
    })?;
    write_file(
        &src.join("Hello.luau"),
        "-- Naht sample module\nreturn {}\n",
    )
}

fn ensure_gitignore(path: &Path) -> Result<(), CommandError> {
    let gitignore = path.join(".gitignore");
    let existing = read_optional(&gitignore)?;
    if existing.lines().any(|line| line.trim() == "/.naht") {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str("/.naht\n");
    write_file(&gitignore, &updated)
}
