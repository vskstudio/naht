use std::fs;
use std::io;
use std::path::Path;

use super::acl_fixtures::{
    assert_only_owner_is_granted, assert_single_explicit_ace_unprotected, owner_sid_text, set_dacl,
};
use super::{TokenError, owner_only, path, prepare_state_dir, state_dir_access};
use crate::test_logs::captured_logs;

#[test]
fn a_file_created_in_the_owner_only_state_directory_is_owner_only_before_any_set() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir(&dir).unwrap();
    owner_only::restrict_dir(&dir).unwrap();

    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path(root.path()))
        .unwrap();

    assert_only_owner_is_granted(&path(root.path()));
}

#[test]
fn a_state_directory_that_is_a_junction_is_refused_and_its_target_is_untouched() {
    let root = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let state_dir = root.path().join(".naht");
    let linked = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&state_dir)
        .arg(target.path())
        .status()
        .unwrap();
    assert!(linked.success());

    let error = prepare_state_dir(root.path()).unwrap_err();
    let text = error.to_string();

    assert!(
        matches!(error, TokenError::StateDirReparsePoint { .. }),
        "{text}"
    );
    assert!(text.contains(&state_dir.display().to_string()), "{text}");
    assert!(text.contains("real directory"), "{text}");
    assert!(!owner_only::is_dir_owner_only(target.path()).unwrap());
}

#[test]
fn a_state_directory_denied_to_this_account_names_the_fix_and_the_state_it_holds() {
    let state_dir = Path::new(r"C:\project\.naht");
    let error = state_dir_access(state_dir, io::Error::from(io::ErrorKind::PermissionDenied));
    let text = error.to_string();
    let shown = state_dir.display();

    assert!(matches!(error, TokenError::StateDirDenied { .. }), "{text}");
    assert!(
        text.contains("another account owns the directory"),
        "{text}"
    );
    assert!(
        text.contains(&format!("takeown /F \"{shown}\" /R /D Y")),
        "{text}"
    );
    assert!(
        text.contains(&format!("icacls \"{shown}\" /reset /T /C")),
        "{text}"
    );
    assert!(text.contains("run `naht serve` again"), "{text}");
    assert!(text.contains("deletes `state.db`"), "{text}");
    assert!(text.contains("place binding"), "{text}");
}

#[test]
fn a_state_directory_that_is_not_protected_is_not_taken_as_owner_only() {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir(&dir).unwrap();
    let owner = owner_sid_text();
    set_dacl(root.path(), &format!("D:P(A;;FA;;;{owner})"), true);
    set_dacl(&dir, &format!("D:(A;OICI;FA;;;{owner})"), false);
    assert_single_explicit_ace_unprotected(&dir);

    assert!(!owner_only::is_dir_owner_only(&dir).unwrap());
    let (logs, dispatch) = captured_logs();
    with_default(&dispatch, || prepare_state_dir(root.path()).unwrap());
    assert!(owner_only::is_dir_owner_only(&dir).unwrap());
    let text = logs.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("WARN"), "{text}");
}
