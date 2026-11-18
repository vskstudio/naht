//! The per-project token that gates every daemon route except `GET /info` (architecture §7).
//!
//! `naht serve` creates `.naht/token` on its first run and reuses it after. The file holds 32 random
//! bytes as 64 lowercase hex digits, readable only by its owner on Unix. The plugin sends the token in
//! the `x-naht-token` header; `naht token` prints it.

use std::fmt::{self, Formatter};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
    match read(root) {
        Ok(token) => {
            restrict_to_owner(&path(root))?;
            Ok(Loaded {
                token,
                created: false,
            })
        }
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

#[cfg(not(unix))]
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
    let mut file = private_file(&path).map_err(io_error)?;
    file.write_all(token.as_str().as_bytes())
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    Ok(token)
}

#[cfg(unix)]
fn private_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> io::Result<fs::File> {
    OpenOptions::new().write(true).create_new(true).open(path)
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

#[cfg(test)]
mod tests;
