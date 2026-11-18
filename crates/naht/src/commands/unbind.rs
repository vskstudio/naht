use std::path::Path;

use super::CommandError;
use super::files::open_store;
use super::serve::take_instance_lock;

/// Forget the Studio place this project is bound to, returning the place it was bound to, if any.
///
/// # Errors
///
/// Returns [`CommandError::AlreadyRunning`] while a daemon holds the instance lock, or a state
/// error when the binding cannot be read or cleared.
pub fn unbind(root: &Path) -> Result<Option<u64>, CommandError> {
    let _instance = take_instance_lock(root)?;
    let store = open_store(root)?;
    let bound = store.place_id()?;
    store.clear_place_id()?;
    Ok(bound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unbind_clears_the_recorded_place_and_reports_it() {
        let root = tempfile::tempdir().unwrap();
        open_store(root.path()).unwrap().set_place_id(11).unwrap();

        assert_eq!(unbind(root.path()).unwrap(), Some(11));
        assert_eq!(open_store(root.path()).unwrap().place_id().unwrap(), None);
    }

    #[test]
    fn unbind_reports_nothing_when_the_project_was_never_bound() {
        let root = tempfile::tempdir().unwrap();

        assert_eq!(unbind(root.path()).unwrap(), None);
    }

    #[test]
    fn unbind_refuses_while_a_daemon_holds_the_lock() {
        let root = tempfile::tempdir().unwrap();
        let _daemon = take_instance_lock(root.path()).unwrap();

        assert!(matches!(
            unbind(root.path()),
            Err(CommandError::AlreadyRunning { .. })
        ));
    }
}
