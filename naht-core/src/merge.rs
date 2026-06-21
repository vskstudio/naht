//! 3-way text merge for scripts, with the last-sync content as the base.
//!
//! A clean merge yields the merged text; an unmergeable one yields the text with git-style conflict
//! markers labelled `FS` / `Studio` (architecture §4). [`has_conflict_markers`] lets `resolve`
//! refuse to clear a path while markers remain.

/// The label put on the filesystem side of a conflict marker.
pub const FS_LABEL: &str = "FS";
/// The label put on the Studio side of a conflict marker.
pub const STUDIO_LABEL: &str = "Studio";

/// The outcome of a 3-way merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge {
    /// The two sides merged cleanly into this text.
    Clean(String),
    /// The sides overlapped; this text carries git-style conflict markers.
    Conflict(String),
}

/// 3-way merge `fs` and `studio` against their common `base`.
#[must_use]
pub fn merge3(base: &str, fs: &str, studio: &str) -> Merge {
    match diffy::merge(base, fs, studio) {
        Ok(merged) => Merge::Clean(merged),
        // diffy hard-codes the `ours` / `theirs` labels; relabel them to FS / Studio so the markers
        // tell the user which side is which.
        Err(conflicted) => Merge::Conflict(
            conflicted
                .replace("<<<<<<< ours", &format!("<<<<<<< {FS_LABEL}"))
                .replace(">>>>>>> theirs", &format!(">>>>>>> {STUDIO_LABEL}")),
        ),
    }
}

/// Whether `text` still contains any git-style conflict marker line.
#[must_use]
pub fn has_conflict_markers(text: &str) -> bool {
    text.lines()
        .any(|line| line.starts_with("<<<<<<<") || line.starts_with(">>>>>>>") || line == "=======")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "local a = 1\nlocal b = 2\nlocal c = 3\n";

    #[test]
    fn non_overlapping_edits_merge_cleanly() {
        let fs = "local a = 10\nlocal b = 2\nlocal c = 3\n";
        let studio = "local a = 1\nlocal b = 2\nlocal c = 30\n";
        let Merge::Clean(merged) = merge3(BASE, fs, studio) else {
            panic!("expected a clean merge");
        };
        assert_eq!(merged, "local a = 10\nlocal b = 2\nlocal c = 30\n");
    }

    #[test]
    fn overlapping_edits_conflict_with_labelled_markers() {
        let fs = "local a = 11\nlocal b = 2\nlocal c = 3\n";
        let studio = "local a = 22\nlocal b = 2\nlocal c = 3\n";
        let Merge::Conflict(text) = merge3(BASE, fs, studio) else {
            panic!("expected a conflict");
        };
        assert!(text.contains("<<<<<<< FS"));
        assert!(text.contains(">>>>>>> Studio"));
        // No data lost: both sides' versions survive in the markers.
        assert!(text.contains("local a = 11"));
        assert!(text.contains("local a = 22"));
        assert!(has_conflict_markers(&text));
    }

    #[test]
    fn clean_text_has_no_markers() {
        assert!(!has_conflict_markers("local a = 1\nlocal b = 2\n"));
    }
}
