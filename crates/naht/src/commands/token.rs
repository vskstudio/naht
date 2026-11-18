use std::io::Write;
use std::path::Path;

use super::error::CommandError;
use crate::output::Output;
use crate::token;

/// Print the project's token, so the Studio plugin can be given it once.
///
/// # Errors
///
/// Returns [`CommandError::NoToken`] when `naht serve` has not created the token for this project,
/// and [`CommandError::Token`] when the file cannot be read or holds no token.
pub fn print_token<Stdout: Write, Stderr: Write>(
    root: &Path,
    out: &mut Output<Stdout, Stderr>,
) -> Result<(), CommandError> {
    let token = read_token(root)?;
    out.say(format_args!("{}", token.as_str()))
        .map_err(CommandError::Output)
}

pub(super) fn read_token(root: &Path) -> Result<token::Token, CommandError> {
    token::read(root).map_err(|error| {
        if error.is_missing() {
            CommandError::NoToken {
                path: token::path(root),
            }
        } else {
            CommandError::Token(error)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_token_prints_the_token_serve_created() {
        let root = tempfile::tempdir().unwrap();
        let created = token::load_or_create(root.path()).unwrap().token;
        let mut stdout = Vec::new();
        let mut out = Output::new(&mut stdout, Vec::new());
        print_token(root.path(), &mut out).unwrap();
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            format!("{}\n", created.as_str())
        );
    }

    #[test]
    fn print_token_without_a_token_file_asks_for_serve() {
        let root = tempfile::tempdir().unwrap();
        let mut out = Output::new(Vec::new(), Vec::new());
        let error = print_token(root.path(), &mut out).unwrap_err();
        assert!(matches!(error, CommandError::NoToken { .. }), "{error:?}");
    }
}
