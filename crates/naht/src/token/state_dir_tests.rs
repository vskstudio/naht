use std::fs;
use std::path::Path;

use tracing::dispatcher::with_default;

use super::acl_fixtures::{owner_sid_text, set_dacl};
use super::tests::captured_logs;
use super::{TokenError, owner_only, path, prepare_state_dir};
use crate::commands::serve::take_instance_lock;

#[test]
fn a_fresh_project_lock_creates_an_owner_only_state_directory_and_lock_file_silently() {
    let root = tempfile::tempdir().unwrap();
    let (logs, dispatch) = captured_logs();

    let lock = with_default(&dispatch, || take_instance_lock(root.path()).unwrap());

    assert!(owner_only::is_dir_owner_only(&root.path().join(".naht")).unwrap());
    assert_only_owner_is_granted(&root.path().join(".naht").join("daemon.lock"));
    assert_eq!(logs.text(), "");
    drop(lock);
}

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
fn a_state_directory_denied_to_this_account_names_the_fix() {
    let root = tempfile::tempdir().unwrap();
    let state_dir = root.path().join(".naht");
    fs::create_dir(&state_dir).unwrap();
    let owner = owner_sid_text();
    set_dacl(
        &state_dir,
        &format!("D:P(D;;FA;;;WD)(A;;FA;;;{owner})"),
        true,
    );

    let error = prepare_state_dir(root.path()).unwrap_err();
    let text = error.to_string();

    assert!(matches!(error, TokenError::StateDirDenied { .. }), "{text}");
    assert!(text.contains("delete the directory"), "{text}");
    assert!(text.contains("run `naht serve`"), "{text}");
}

fn assert_only_owner_is_granted(file: &Path) {
    use windows_permissions::constants::{SeObjectType, SecurityInformation};
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::{EqualSid, GetNamedSecurityInfo};

    let descriptor = GetNamedSecurityInfo(
        file,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl,
    )
    .unwrap();
    let sddl = descriptor.as_sddl().unwrap().to_string_lossy().into_owned();
    let dacl = descriptor.dacl().unwrap();
    let only_ace = dacl.get_ace(0).expect("the DACL has no ACE");
    assert!(dacl.get_ace(1).is_none(), "more than one ACE: {sddl}");
    let owner = current_process_sid().unwrap();
    assert!(EqualSid(only_ace.sid().unwrap(), &owner), "{sddl}");
}
