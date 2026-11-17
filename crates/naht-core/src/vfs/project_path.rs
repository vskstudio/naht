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
    }
    Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::{validate_project_path, ProjectPathError};

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
