//! The per-project token that gates every daemon route except `GET /info` (architecture §7).
//!
//! `naht serve` creates `.naht/token` on its first run and reuses it after. The file holds 32 random
//! bytes as 64 lowercase hex digits, readable only by its owner: mode 0600 on Unix, and on Windows a
//! protected DACL that grants only the current user. The plugin sends the token in the `x-naht-token`
//! header; `naht token` prints it.

use std::fmt::{self, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use hmac::{Hmac, KeyInit, Mac};
use naht_core::protocol;
use sha2::Sha256;

const TOKEN_BYTES: usize = 32;
const TOKEN_DIGITS: usize = TOKEN_BYTES * 2;

/// The daemon's secret. Its `Debug` output is redacted so a log line never carries it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// Draw a fresh token from the operating system's random source.
    ///
    /// # Errors
    ///
    /// Returns [`TokenError::Random`] when the system cannot supply random bytes.
    pub fn generate() -> Result<Self, TokenError> {
        let mut bytes = [0_u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes).map_err(TokenError::Random)?;
        Ok(Self(hex(&bytes)))
    }

    /// The token's text, as `naht token` prints it and the plugin sends it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `presented`, the bytes of a request's token header, equal this token. The comparison
    /// reads every byte whatever the first mismatch, so its timing does not reveal the token.
    pub fn matches(&self, presented: &[u8]) -> bool {
        constant_time_eq(self.0.as_bytes(), presented)
    }

    /// The daemon proof for `nonce` on `port`: the lowercase hex HMAC-SHA256 of
    /// [`protocol::proof_message`] keyed by the token text, which the plugin checks before it sends
    /// the token. `None` only if the MAC refuses the key, which HMAC never does.
    pub fn prove(&self, port: u16, nonce: &str) -> Option<String> {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(self.0.as_bytes()).ok()?;
        mac.update(protocol::proof_message(port, nonce).as_bytes());
        Some(hex(&mac.finalize().into_bytes()))
    }

    fn parse(text: &str) -> Option<Self> {
        let well_formed = text.len() == TOKEN_DIGITS
            && text
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
        well_formed.then(|| Self(text.to_owned()))
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// The token a project's daemon uses, and whether this call created it.
#[derive(Debug)]
pub struct Loaded {
    /// The token in `.naht/token`.
    pub token: Token,
    /// Whether the file was missing and this call wrote it.
    pub created: bool,
}

/// Why the token file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// The token file could not be read or written.
    #[error("cannot read or write the token file `{}`", path.display())]
    Io {
        /// The token file.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// The token file cannot be opened by this account, which in practice means another account owns it.
    #[error(
        "the token file `{}` cannot be opened by this account, so `naht serve` cannot read or repair the token. This happens when another account owns the file, for example one created from an elevated shell. Take ownership first from a command prompt with `takeown /F \"{}\"`, then run `naht serve` again. To start over instead, delete the file, run `naht serve` to make a new token, and enter the new token in the plugin.",
        path.display(),
        path.display()
    )]
    Denied {
        /// The token file.
        path: PathBuf,
    },
    /// The state directory could not be read or written.
    #[error("cannot use the state directory `{}`", path.display())]
    StateDirIo {
        /// The state directory.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// The state directory cannot be opened by this account, which in practice means another account owns it.
    #[error(
        "the state directory `{}` cannot be opened by this account, so `naht serve` cannot use it. This happens when another account owns the directory, for example one created from an elevated shell. Take ownership first from a command prompt: `takeown /F \"{}\" /R /D Y`, then `icacls \"{}\" /reset /T /C`, then run `naht serve` again. Deleting the directory instead also deletes `state.db`, which holds the bases, the recorded conflicts and the place binding.",
        path.display(),
        path.display(),
        path.display()
    )]
    StateDirDenied {
        /// The state directory.
        path: PathBuf,
    },
    /// The state directory is a junction or symbolic link, which `naht` refuses to follow.
    #[error("the state directory `{}` is a junction or symbolic link: replace it with a real directory and run `naht serve` again", path.display())]
    StateDirReparsePoint {
        /// The state directory.
        path: PathBuf,
    },
    /// The token file does not hold a well-formed token, typically after a crash while it was written.
    #[error("the token file `{}` holds no token: delete it and run `naht serve` to make a new one", path.display())]
    Malformed {
        /// The token file.
        path: PathBuf,
    },
    /// The system could not supply random bytes for a new token.
    #[error("cannot draw random bytes for the token: {0}")]
    Random(getrandom::Error),
}

impl TokenError {
    /// Whether the token file does not exist yet, so a first run may create it.
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Io { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// The token file path of the project at `root`.
pub fn path(root: &Path) -> PathBuf {
    root.join(".naht").join("token")
}

/// Read the project's token, creating the file on the first run.
///
/// # Errors
///
/// Returns [`TokenError`] when the file exists but holds no token, or cannot be read or created.
pub fn load_or_create(root: &Path) -> Result<Loaded, TokenError> {
    #[cfg(windows)]
    prepare_state_dir(root)?;
    match restrict_to_owner(&path(root)) {
        Ok(()) => read(root).map(|token| Loaded {
            token,
            created: false,
        }),
        Err(error) if error.is_missing() => create(root).map(|token| Loaded {
            token,
            created: true,
        }),
        Err(error) => Err(error),
    }
}

/// Read the project's token from its file.
///
/// # Errors
///
/// Returns [`TokenError::Io`] when the file cannot be read, and [`TokenError::Malformed`] when it
/// does not hold a token.
pub fn read(root: &Path) -> Result<Token, TokenError> {
    let path = path(root);
    let text = fs::read_to_string(&path).map_err(|source| TokenError::Io {
        path: path.clone(),
        source,
    })?;
    Token::parse(text.trim()).ok_or(TokenError::Malformed { path })
}

#[cfg(unix)]
fn restrict_to_owner(path: &Path) -> Result<(), TokenError> {
    use std::os::unix::fs::PermissionsExt;
    let io_error = |source| TokenError::Io {
        path: path.to_path_buf(),
        source,
    };
    let mode = fs::metadata(path).map_err(io_error)?.permissions().mode();
    let shared_with_others = mode & 0o077;
    if shared_with_others == 0 {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(io_error)?;
    tracing::warn!(
        target: "naht::token",
        path = %path.display(),
        "the token file was readable by other users; it is now owner-only"
    );
    Ok(())
}

#[cfg(windows)]
fn restrict_to_owner(path: &Path) -> Result<(), TokenError> {
    let access_error = |source| windows_access_error(path, source);
    if owner_only::is_file_owner_only(path).map_err(access_error)? {
        return Ok(());
    }
    owner_only::restrict_file(path).map_err(access_error)?;
    tracing::warn!(
        target: "naht::token",
        path = %path.display(),
        "the token file was open to other accounts; it is now owner-only"
    );
    Ok(())
}

#[cfg(windows)]
fn windows_access_error(path: &Path, source: io::Error) -> TokenError {
    if source.kind() == io::ErrorKind::PermissionDenied {
        return TokenError::Denied {
            path: path.to_path_buf(),
        };
    }
    TokenError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Make the `.naht` state directory of the project at `root` owner-only, before anything is
/// created inside it.
///
/// # Errors
///
/// Returns [`TokenError::StateDirReparsePoint`] when `.naht` is a junction or symbolic link,
/// [`TokenError::StateDirDenied`] when its access rules deny this account, and
/// [`TokenError::StateDirIo`] for any other failure.
#[cfg(windows)]
pub(crate) fn prepare_state_dir(root: &Path) -> Result<(), TokenError> {
    let dir = root.join(".naht");
    fs::create_dir_all(root).map_err(|source| state_dir_io(&dir, source))?;
    match fs::create_dir(&dir) {
        Ok(()) => owner_only::restrict_dir(&dir).map_err(|source| state_dir_access(&dir, source)),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            refuse_reparse_point(&dir)?;
            let owner_only_now = owner_only::is_dir_owner_only(&dir)
                .map_err(|source| state_dir_access(&dir, source))?;
            if owner_only_now {
                return Ok(());
            }
            owner_only::restrict_dir(&dir).map_err(|source| state_dir_access(&dir, source))?;
            tracing::warn!(
                target: "naht::token",
                path = %dir.display(),
                "the state directory was open to other accounts; it is now owner-only"
            );
            Ok(())
        }
        Err(error) => Err(state_dir_io(&dir, error)),
    }
}

#[cfg(windows)]
fn refuse_reparse_point(dir: &Path) -> Result<(), TokenError> {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    let metadata = fs::symlink_metadata(dir).map_err(|source| state_dir_access(dir, source))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        return Ok(());
    }
    Err(TokenError::StateDirReparsePoint {
        path: dir.to_path_buf(),
    })
}

#[cfg(windows)]
fn state_dir_access(dir: &Path, source: io::Error) -> TokenError {
    if source.kind() == io::ErrorKind::PermissionDenied {
        return TokenError::StateDirDenied {
            path: dir.to_path_buf(),
        };
    }
    state_dir_io(dir, source)
}

#[cfg(windows)]
fn state_dir_io(path: &Path, source: io::Error) -> TokenError {
    TokenError::StateDirIo {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(not(any(unix, windows)))]
fn restrict_to_owner(_path: &Path) -> Result<(), TokenError> {
    Ok(())
}

fn create(root: &Path) -> Result<Token, TokenError> {
    let path = path(root);
    let token = Token::generate()?;
    let io_error = |source| TokenError::Io {
        path: path.clone(),
        source,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    write_new(&path, token.as_str()).map_err(io_error)?;
    Ok(token)
}

#[cfg(unix)]
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

#[cfg(windows)]
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    owner_only::write_new(path, text)
}

#[cfg(not(any(unix, windows)))]
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .concat()
}

fn constant_time_eq(expected: &[u8], presented: &[u8]) -> bool {
    expected.len() == presented.len()
        && expected
            .iter()
            .zip(presented)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}

#[cfg(test)]
pub(crate) const TEST_TOKEN: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[cfg(test)]
impl Token {
    pub(crate) fn for_tests() -> Self {
        Self(TEST_TOKEN.to_owned())
    }
}

#[cfg(all(test, windows))]
mod acl_fixtures;
#[cfg(all(test, windows))]
pub(crate) use acl_fixtures::{assert_dacl_is, assert_only_owner_is_granted, owner_sid_text};
#[cfg(windows)]
mod owner_only;
#[cfg(all(test, windows))]
mod state_dir_tests;
#[cfg(test)]
mod tests;
