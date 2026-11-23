use std::fs;
use std::io;
use std::path::Path;

use super::acl_fixtures::{
    assert_dacl_is, assert_single_explicit_ace_unprotected, owner_sid_text, set_dacl,
};
use super::windows_access_error;
use super::{Token, TokenError, load_or_create, path};
use crate::test_logs::captured_logs;

#[cfg(windows)]
fn assert_owner_only(file: &Path) {
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
    assert!(sddl.starts_with("D:P"), "the DACL is not protected: {sddl}");
    let dacl = descriptor.dacl().unwrap();
    let only_ace = dacl.get_ace(0).expect("the DACL has no ACE");
    assert!(dacl.get_ace(1).is_none(), "more than one ACE: {sddl}");
    let owner = current_process_sid().unwrap();
    assert!(EqualSid(only_ace.sid().unwrap(), &owner), "{sddl}");
}

#[cfg(windows)]
#[test]
fn a_new_token_file_is_created_with_a_protected_dacl_for_its_owner_only() {
    let root = tempfile::tempdir().unwrap();
    load_or_create(root.path()).unwrap();
    assert_owner_only(&path(root.path()));
}

#[cfg(windows)]
#[test]
fn a_token_file_open_to_other_accounts_is_restricted_with_one_warning() {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    assert!(!super::owner_only::is_file_owner_only(&path(root.path())).unwrap());
    let (logs, dispatch) = captured_logs();

    let loaded = with_default(&dispatch, || load_or_create(root.path()).unwrap());

    assert_eq!(loaded.token, token);
    assert_owner_only(&path(root.path()));
    let text = logs.text();
    assert_eq!(text.lines().count(), 2, "{text}");
    assert_eq!(text.matches("token file").count(), 1, "{text}");
    assert!(text.contains("WARN"), "{text}");
    assert!(!text.contains(token.as_str()), "{text}");
}

#[cfg(windows)]
#[test]
fn a_token_file_already_protected_loads_silently_and_keeps_its_dacl() {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    load_or_create(root.path()).unwrap();
    assert_owner_only(&path(root.path()));
    let (logs, dispatch) = captured_logs();

    let loaded = with_default(&dispatch, || load_or_create(root.path()).unwrap());

    assert!(!loaded.created);
    assert_eq!(logs.text(), "");
    assert_owner_only(&path(root.path()));
}

#[cfg(windows)]
#[test]
fn a_token_file_created_by_another_writer_is_not_replaced() {
    use std::io::ErrorKind;

    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir_all(&dir).unwrap();
    fs::write(path(root.path()), "winner").unwrap();

    let error = super::write_new(&path(root.path()), "loser").unwrap_err();

    assert_eq!(error.kind(), ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(path(root.path())).unwrap(), "winner");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn a_new_token_file_has_the_owner_only_dacl_before_any_token_byte_is_written() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();

    let file = super::owner_only::create(&path(root.path())).unwrap();

    assert_eq!(file.metadata().unwrap().len(), 0);
    assert_owner_only(&path(root.path()));
}

#[cfg(windows)]
fn assert_rewritten_to_owner_only_with_one_warning(sddl: &str, protected: bool) {
    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir_all(&dir).unwrap();
    super::owner_only::restrict_dir(&dir).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    set_dacl(&path(root.path()), sddl, protected);
    assert_dacl_is(&path(root.path()), sddl, protected);

    assert_loaded_with_one_warning(root.path(), &token);
}

#[cfg(windows)]
fn assert_loaded_with_one_warning(root: &Path, token: &Token) {
    use tracing::dispatcher::with_default;

    let (logs, dispatch) = captured_logs();

    let loaded = with_default(&dispatch, || load_or_create(root).unwrap());

    assert_eq!(loaded.token, *token);
    assert!(!loaded.created);
    assert_owner_only(&path(root));
    let text = logs.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("WARN"), "{text}");
    assert!(!text.contains(token.as_str()), "{text}");
}

#[cfg(windows)]
#[test]
fn a_protected_dacl_with_an_extra_ace_is_rewritten_with_one_warning() {
    let owner = owner_sid_text();
    assert_rewritten_to_owner_only_with_one_warning(
        &format!("D:P(A;;FA;;;{owner})(A;;FR;;;WD)"),
        true,
    );
}

#[cfg(windows)]
#[test]
fn a_protected_dacl_for_another_account_alone_is_rewritten_with_one_warning() {
    assert_rewritten_to_owner_only_with_one_warning("D:P(A;;FA;;;WD)", true);
}

#[cfg(windows)]
#[test]
fn a_token_file_denied_to_this_account_names_the_fix() {
    let error = windows_access_error(
        &path(Path::new(r"C:\project")),
        io::Error::from(io::ErrorKind::PermissionDenied),
    );
    let text = error.to_string();

    assert!(matches!(error, TokenError::Denied { .. }), "{text}");
    assert!(text.contains("another account owns the file"), "{text}");
    assert!(
        text.contains("takeown /F \"C:\\project\\.naht\\token\""),
        "{text}"
    );
    assert!(text.contains("delete the file"), "{text}");
    assert!(text.contains("run `naht serve`"), "{text}");
    assert!(text.contains("enter the new token in the plugin"), "{text}");
}

#[cfg(windows)]
#[test]
fn a_token_file_owned_by_this_account_with_an_extra_ace_is_repaired_with_one_warning() {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir(&dir).unwrap();
    let owner = owner_sid_text();
    set_dacl(&dir, &format!("D:P(A;OICI;FA;;;{owner})"), true);
    fs::write(path(root.path()), token.as_str()).unwrap();
    set_dacl(
        &path(root.path()),
        &format!("D:P(A;;FA;;;{owner})(A;;FR;;;WD)"),
        true,
    );
    assert!(!super::owner_only::is_file_owner_only(&path(root.path())).unwrap());

    let (logs, dispatch) = captured_logs();
    let loaded = with_default(&dispatch, || load_or_create(root.path()).unwrap());

    assert_eq!(loaded.token, token);
    assert!(!loaded.created);
    assert!(super::owner_only::is_file_owner_only(&path(root.path())).unwrap());
    let text = logs.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(
        text.contains("the token file was open to other accounts"),
        "{text}"
    );
}

#[cfg(windows)]
#[test]
fn a_deleted_token_file_is_replaced_by_a_new_owner_only_token() {
    let root = tempfile::tempdir().unwrap();

    let first = load_or_create(root.path()).unwrap();
    assert!(first.created);
    fs::remove_file(path(root.path())).unwrap();
    let renewed = load_or_create(root.path()).unwrap();

    assert!(renewed.created);
    assert_ne!(renewed.token, first.token);
    assert_owner_only(&path(root.path()));
}

#[cfg(windows)]
#[test]
fn a_protected_owner_ace_carrying_an_inherit_flag_loads_silently_as_owner_only() {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir_all(&dir).unwrap();
    super::owner_only::restrict_dir(&dir).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    let owner = owner_sid_text();
    set_dacl(
        &path(root.path()),
        &format!("D:P(A;OICI;FA;;;{owner})"),
        true,
    );
    assert_owner_only(&path(root.path()));
    let (logs, dispatch) = captured_logs();

    let loaded = with_default(&dispatch, || load_or_create(root.path()).unwrap());

    assert_eq!(loaded.token, token);
    assert!(!loaded.created);
    assert_owner_only(&path(root.path()));
    assert!(logs.text().is_empty(), "{}", logs.text());
}

#[cfg(windows)]
#[test]
fn a_token_file_with_an_unprotected_owner_only_dacl_is_rewritten_with_one_warning() {
    let owner = owner_sid_text();
    assert_rewritten_to_owner_only_with_one_warning(&format!("D:(A;;FA;;;{owner})"), false);
}

#[cfg(windows)]
#[test]
fn an_unprotected_owner_ace_with_no_inherited_ace_is_rewritten_with_one_warning() {
    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir(&dir).unwrap();
    let owner = owner_sid_text();
    let sddl = format!("D:(A;;FA;;;{owner})");
    set_dacl(&dir, &format!("D:P(A;;FA;;;{owner})"), true);
    fs::write(path(root.path()), token.as_str()).unwrap();
    set_dacl(&path(root.path()), &sddl, false);
    assert_single_explicit_ace_unprotected(&path(root.path()));
    super::owner_only::restrict_dir(&dir).unwrap();
    assert_dacl_is(&path(root.path()), &sddl, false);

    assert_loaded_with_one_warning(root.path(), &token);
}

#[cfg(windows)]
#[test]
fn an_owner_only_token_file_that_is_not_protected_is_not_taken_as_owner_only() {
    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir(&dir).unwrap();
    let owner = owner_sid_text();
    set_dacl(&dir, &format!("D:P(A;;FA;;;{owner})"), true);
    fs::write(path(root.path()), token.as_str()).unwrap();
    set_dacl(&path(root.path()), &format!("D:(A;;FA;;;{owner})"), false);
    assert_single_explicit_ace_unprotected(&path(root.path()));

    assert!(!super::owner_only::is_file_owner_only(&path(root.path())).unwrap());
}

#[cfg(windows)]
#[test]
fn a_loose_state_directory_is_tightened_with_one_warning() {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(".naht");
    fs::create_dir(&dir).unwrap();
    let (logs, dispatch) = captured_logs();

    let loaded = with_default(&dispatch, || load_or_create(root.path()).unwrap());

    assert!(loaded.created);
    assert!(super::owner_only::is_dir_owner_only(&dir).unwrap());
    let text = logs.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("state directory"), "{text}");
}
