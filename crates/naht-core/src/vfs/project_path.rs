use crate::binary::BLOB_EXTENSION;
use crate::mapper::{init_class, script_class};

/// Validate a path received from the Studio protocol as project-relative.
///
/// The daemon joins protocol paths to a project root, so absolute paths and parent components
/// would otherwise escape that root. Backslashes and drive prefixes are rejected explicitly so the
/// same wire contract is safe on Unix and Windows.
///
/// # Errors
///
/// Returns [`ProjectPathError`] when `path` is not a valid project-relative path.
pub fn validate_project_path(path: &str) -> Result<(), ProjectPathError> {
    if path.is_empty() {
        return Err(ProjectPathError::Empty);
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(ProjectPathError::Absolute);
    }
    if path.contains('\0') {
        return Err(ProjectPathError::Nul);
    }

    for component in path.split(['/', '\\']) {
        if component.is_empty() {
            return Err(ProjectPathError::EmptyComponent);
        }
        if component == "." || component == ".." {
            return Err(ProjectPathError::Traversal);
        }
        if component.contains(':') {
            return Err(ProjectPathError::DrivePrefix);
        }
        if component.chars().any(is_forbidden_character) {
            return Err(ProjectPathError::ForbiddenCharacter);
        }
        if is_windows_device_name(component) {
            return Err(ProjectPathError::ReservedName);
        }
    }
    Ok(())
}

const WINDOWS_DEVICE_NAMES: [&str; 33] = [
    "CON",
    "CONIN$",
    "CONOUT$",
    "PRN",
    "AUX",
    "NUL",
    "CLOCK$",
    "COM0",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "COM\u{b9}",
    "COM\u{b2}",
    "COM\u{b3}",
    "LPT0",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    "LPT\u{b9}",
    "LPT\u{b2}",
    "LPT\u{b3}",
];

fn is_windows_device_name(component: &str) -> bool {
    let stem = component
        .split_once('.')
        .map_or(component, |(stem, _)| stem)
        .trim_end_matches(' ');
    WINDOWS_DEVICE_NAMES
        .iter()
        .any(|name| stem.eq_ignore_ascii_case(name))
}

fn is_forbidden_character(character: char) -> bool {
    matches!(character, '<' | '>' | '"' | '|' | '?' | '*') || character <= '\u{1f}'
}

/// Validate a path sent to `/changes`: a project-relative, visible file the mapper turns into a script.
///
/// # Errors
///
/// Returns [`ProjectPathError`] when the path is unsafe or does not map to a Studio script.
pub fn validate_source_path(path: &str) -> Result<(), ProjectPathError> {
    validate_project_path(path)?;
    reject_hidden_components(path)?;
    let file_name = last_component(path);
    if script_class(file_name).is_some() || init_class(file_name).is_some() {
        Ok(())
    } else {
        Err(ProjectPathError::Unmapped)
    }
}

/// Validate a directory path a `/changes` deletion names: a project-relative, visible path.
///
/// # Errors
///
/// Returns [`ProjectPathError`] when the path is unsafe or hidden.
pub fn validate_directory_path(path: &str) -> Result<(), ProjectPathError> {
    validate_project_path(path)?;
    reject_hidden_components(path)
}

/// Validate a path sent to `/blobs`: a project-relative, visible file with the blob extension.
///
/// # Errors
///
/// Returns [`ProjectPathError`] when the path is unsafe or is not a binary blob.
pub fn validate_blob_path(path: &str) -> Result<(), ProjectPathError> {
    validate_project_path(path)?;
    reject_hidden_components(path)?;
    match last_component(path).rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && extension == BLOB_EXTENSION => Ok(()),
        _ => Err(ProjectPathError::Unmapped),
    }
}

fn reject_hidden_components(path: &str) -> Result<(), ProjectPathError> {
    if path
        .split(['/', '\\'])
        .any(|component| component.starts_with('.'))
    {
        Err(ProjectPathError::Hidden)
    } else {
        Ok(())
    }
}

fn last_component(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Why a protocol path is not a safe project-relative path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProjectPathError {
    /// No path was supplied.
    #[error("path must not be empty")]
    Empty,
    /// The path starts at the filesystem root.
    #[error("absolute paths are not allowed")]
    Absolute,
    /// The path contains an empty component.
    #[error("path contains an empty component")]
    EmptyComponent,
    /// The path contains a parent or current-directory component.
    #[error("path traversal components are not allowed")]
    Traversal,
    /// The path contains a NUL byte.
    #[error("NUL bytes are not allowed in paths")]
    Nul,
    /// The path contains a Windows drive prefix.
    #[error("drive prefixes are not allowed")]
    DrivePrefix,
    /// The path has a hidden component such as `.naht`.
    #[error("hidden path components are not allowed")]
    Hidden,
    /// The file does not map to a Studio instance the daemon can write.
    #[error("the file does not map to a Studio instance")]
    Unmapped,
    /// A path component is a Windows device name such as `CON` or `LPT1`.
    #[error("Windows device names are not allowed in paths")]
    ReservedName,
    /// A path component holds a character Windows forbids in file names, or a control byte.
    #[error("characters Windows forbids in file names are not allowed")]
    ForbiddenCharacter,
}

#[cfg(test)]
mod tests {
    use super::{ProjectPathError, validate_project_path};

    const FIXTURE: &str = include_str!("../../tests/fixtures/project_path_names.json");

    fn fixture_names(key: &str) -> Vec<String> {
        let fixture: serde_json::Value =
            serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
        fixture[key]
            .as_array()
            .expect("fixture key is an array")
            .iter()
            .map(|name| name.as_str().expect("fixture name is a string").to_owned())
            .collect()
    }

    #[test]
    fn folder_names_the_daemon_refuses_are_refused_as_a_segment() {
        for name in fixture_names("refused") {
            let path = format!("Dir/{name}/Leaf.luau");
            assert!(
                validate_project_path(&path).is_err(),
                "daemon accepted {name:?}"
            );
        }
    }

    #[test]
    fn folder_names_the_daemon_accepts_are_accepted_as_a_segment() {
        for name in fixture_names("accepted") {
            let path = format!("Dir/{name}/Leaf.luau");
            assert!(
                validate_project_path(&path).is_ok(),
                "daemon refused {name:?}"
            );
        }
    }

    fn refused_as_forbidden_character(character: &str) {
        for path in [
            format!("Dir/a{character}b/Leaf.luau"),
            format!("a{character}b.luau"),
            format!("Dir/Leaf{character}.luau"),
        ] {
            assert_eq!(
                validate_project_path(&path),
                Err(ProjectPathError::ForbiddenCharacter),
                "{path:?}"
            );
        }
    }

    #[test]
    fn a_less_than_sign_is_refused_in_every_segment() {
        refused_as_forbidden_character("<");
    }

    #[test]
    fn a_greater_than_sign_is_refused_in_every_segment() {
        refused_as_forbidden_character(">");
    }

    #[test]
    fn a_double_quote_is_refused_in_every_segment() {
        refused_as_forbidden_character("\"");
    }

    #[test]
    fn a_pipe_is_refused_in_every_segment() {
        refused_as_forbidden_character("|");
    }

    #[test]
    fn a_question_mark_is_refused_in_every_segment() {
        refused_as_forbidden_character("?");
    }

    #[test]
    fn an_asterisk_is_refused_in_every_segment() {
        refused_as_forbidden_character("*");
    }

    #[test]
    fn a_control_byte_is_refused_in_every_segment() {
        refused_as_forbidden_character("\u{1}");
        refused_as_forbidden_character("\u{1f}");
    }

    #[test]
    fn empty_middle_segments_are_refused_as_empty_components() {
        for name in ["", "\\", "a//b"] {
            let path = format!("Dir/{name}/Leaf.luau");
            assert_eq!(
                validate_project_path(&path),
                Err(ProjectPathError::EmptyComponent),
                "{name:?}"
            );
        }
    }
}
