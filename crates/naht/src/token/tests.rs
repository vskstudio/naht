use std::fs;

use super::{Token, TokenError, load_or_create, path, read};

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
