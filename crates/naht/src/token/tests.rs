use std::fs;
#[cfg(windows)]
use std::path::Path;

use super::{Token, TokenError, load_or_create, path, read};
use crate::test_logs::LogBuffer;

#[test]
fn generated_tokens_are_64_lowercase_hex_digits_and_differ() {
    let first = Token::generate().unwrap();
    let second = Token::generate().unwrap();
    assert_eq!(first.as_str().len(), 64);
    assert!(
        first
            .as_str()
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_ne!(first, second);
}

#[test]
fn a_token_matches_only_its_own_bytes() {
    let token = Token::generate().unwrap();
    let text = token.as_str().to_owned();
    assert!(token.matches(text.as_bytes()));
    assert!(!token.matches(b""));
    assert!(!token.matches(&text.as_bytes()[..63]));
    assert!(!token.matches(format!("{text}0").as_bytes()));
    let mut wrong = text.clone().into_bytes();
    wrong[63] = if wrong[63] == b'0' { b'1' } else { b'0' };
    assert!(!token.matches(&wrong));
}

#[test]
fn debug_output_never_shows_the_token() {
    let token = Token::generate().unwrap();
    let shown = format!("{token:?}");
    assert!(!shown.contains(token.as_str()), "{shown}");
    assert_eq!(shown, "Token(<redacted>)");
}

#[test]
fn the_first_load_creates_the_file_and_later_loads_reuse_it() {
    let root = tempfile::tempdir().unwrap();
    let created = load_or_create(root.path()).unwrap();
    assert!(created.created);
    let again = load_or_create(root.path()).unwrap();
    assert!(!again.created);
    assert_eq!(created.token, again.token);
    assert_eq!(read(root.path()).unwrap(), created.token);
    assert_eq!(
        fs::read_to_string(path(root.path())).unwrap(),
        created.token.as_str()
    );
}

#[cfg(unix)]
#[test]
fn the_token_file_is_readable_only_by_its_owner() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    load_or_create(root.path()).unwrap();
    let mode = fs::metadata(path(root.path()))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[cfg(unix)]
fn tightened_from(before: u32) {
    use std::os::unix::fs::PermissionsExt;
    use tracing::subscriber::with_default;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    fs::set_permissions(path(root.path()), fs::Permissions::from_mode(before)).unwrap();
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let logs = logs.clone();
            move || logs.clone()
        })
        .finish();

    let loaded = with_default(subscriber, || load_or_create(root.path()).unwrap());

    assert_eq!(loaded.token, token);
    assert!(!loaded.created);
    let mode = fs::metadata(path(root.path()))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let text = logs.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("WARN"), "{text}");
    assert!(!text.contains(token.as_str()), "{text}");
}

#[cfg(unix)]
#[test]
fn a_token_file_readable_by_others_is_tightened_with_one_warning() {
    tightened_from(0o644);
}

#[cfg(unix)]
#[test]
fn a_token_file_readable_by_the_group_alone_is_tightened_with_one_warning() {
    tightened_from(0o640);
}

#[cfg(unix)]
#[test]
fn a_private_token_file_logs_nothing_when_loaded() {
    use tracing::subscriber::with_default;

    let root = tempfile::tempdir().unwrap();
    load_or_create(root.path()).unwrap();
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let logs = logs.clone();
            move || logs.clone()
        })
        .finish();

    with_default(subscriber, || load_or_create(root.path()).unwrap());

    assert_eq!(logs.text(), "");
}

#[test]
fn a_malformed_token_file_is_refused_and_left_in_place() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), "deadbeef").unwrap();
    assert!(matches!(
        load_or_create(root.path()),
        Err(TokenError::Malformed { .. })
    ));
    assert_eq!(fs::read_to_string(path(root.path())).unwrap(), "deadbeef");
}

#[test]
fn surrounding_whitespace_in_the_token_file_is_ignored() {
    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), format!("{}\n", token.as_str())).unwrap();
    assert_eq!(read(root.path()).unwrap(), token);
}

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
    use tracing::subscriber::with_default;
    use windows_permissions::constants::{SeObjectType, SecurityInformation};
    use windows_permissions::wrappers::GetNamedSecurityInfo;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    let inherited = GetNamedSecurityInfo(
        &path(root.path()),
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl,
    )
    .unwrap();
    assert!(
        !inherited
            .as_sddl()
            .unwrap()
            .to_string_lossy()
            .starts_with("D:P")
    );
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let logs = logs.clone();
            move || logs.clone()
        })
        .finish();

    let loaded = with_default(subscriber, || load_or_create(root.path()).unwrap());

    assert_eq!(loaded.token, token);
    assert_owner_only(&path(root.path()));
    let text = logs.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("WARN"), "{text}");
    assert!(!text.contains(token.as_str()), "{text}");
}

#[cfg(windows)]
fn captured_logs() -> (LogBuffer, tracing::Dispatch) {
    let logs = LogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer({
            let logs = logs.clone();
            move || logs.clone()
        })
        .finish();
    (logs, tracing::Dispatch::new(subscriber))
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
fn owner_sid_text() -> String {
    use windows_permissions::utilities::current_process_sid;
    use windows_permissions::wrappers::ConvertSidToStringSid;

    let sid = current_process_sid().unwrap();
    ConvertSidToStringSid(&sid)
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn set_protected_dacl(file: &Path, sddl: &str) {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_permissions::constants::{AccessRights, SeObjectType, SecurityInformation};
    use windows_permissions::wrappers::{
        ConvertStringSecurityDescriptorToSecurityDescriptor, SetSecurityInfo,
    };

    let descriptor = ConvertStringSecurityDescriptorToSecurityDescriptor(sddl).unwrap();
    let mut handle = fs::OpenOptions::new()
        .access_mode((AccessRights::ReadControl | AccessRights::WriteDac).bits())
        .open(file)
        .unwrap();
    SetSecurityInfo(
        &mut handle,
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        descriptor.dacl(),
        None,
    )
    .unwrap();
}

#[cfg(windows)]
fn assert_rewritten_to_owner_only_with_one_warning(sddl: &str) {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    set_protected_dacl(&path(root.path()), sddl);
    let (logs, dispatch) = captured_logs();

    let loaded = with_default(&dispatch, || load_or_create(root.path()).unwrap());

    assert_eq!(loaded.token, token);
    assert!(!loaded.created);
    assert_owner_only(&path(root.path()));
    let text = logs.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("WARN"), "{text}");
    assert!(!text.contains(token.as_str()), "{text}");
}

#[cfg(windows)]
#[test]
fn a_protected_dacl_with_an_extra_ace_is_rewritten_with_one_warning() {
    let owner = owner_sid_text();
    assert_rewritten_to_owner_only_with_one_warning(&format!("D:P(A;;FA;;;{owner})(A;;FR;;;WD)"));
}

#[cfg(windows)]
#[test]
fn a_protected_dacl_for_another_account_alone_is_rewritten_with_one_warning() {
    assert_rewritten_to_owner_only_with_one_warning("D:P(A;;FA;;;WD)");
}

#[cfg(windows)]
#[test]
fn a_protected_dacl_with_a_deny_ace_on_everyone_fails_closed_and_keeps_the_file() {
    use std::io::ErrorKind;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    let owner = owner_sid_text();
    set_protected_dacl(
        &path(root.path()),
        &format!("D:P(D;;FA;;;WD)(A;;FA;;;{owner})"),
    );

    let error = load_or_create(root.path()).unwrap_err();

    assert!(
        matches!(&error, TokenError::Io { source, .. } if source.kind() == ErrorKind::PermissionDenied),
        "{error:?}"
    );
    assert_eq!(
        fs::read_to_string(path(root.path())).unwrap(),
        token.as_str()
    );
}

#[cfg(windows)]
#[test]
fn a_protected_owner_ace_carrying_an_inherit_flag_loads_silently_as_owner_only() {
    use tracing::dispatcher::with_default;

    let root = tempfile::tempdir().unwrap();
    let token = Token::generate().unwrap();
    fs::create_dir_all(root.path().join(".naht")).unwrap();
    fs::write(path(root.path()), token.as_str()).unwrap();
    let owner = owner_sid_text();
    set_protected_dacl(&path(root.path()), &format!("D:P(A;OICI;FA;;;{owner})"));
    assert_owner_only(&path(root.path()));
    let (logs, dispatch) = captured_logs();

    let loaded = with_default(&dispatch, || load_or_create(root.path()).unwrap());

    assert_eq!(loaded.token, token);
    assert!(!loaded.created);
    assert_owner_only(&path(root.path()));
    assert!(logs.text().is_empty(), "{}", logs.text());
}

const PROOF_NONCE: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

#[test]
fn the_proof_is_the_hmac_sha256_the_plugin_suite_pins() {
    assert_eq!(
        Token::for_tests().prove(34872, PROOF_NONCE).as_deref(),
        Some("dcb7f3d58ef90f33f851024789b159b0482f018970b17c628f3009c9a6aa5928")
    );
}

#[test]
fn a_proof_for_another_port_does_not_match() {
    assert_eq!(
        Token::for_tests().prove(34873, PROOF_NONCE).as_deref(),
        Some("db399fc25be7c3c5ccf32d3052e20508e391ff333896dd4c3aca877b02fc1901")
    );
}

#[test]
fn another_token_proves_differently() {
    let other = Token::generate().unwrap();
    assert_ne!(
        other.prove(34872, PROOF_NONCE),
        Token::for_tests().prove(34872, PROOF_NONCE)
    );
}
