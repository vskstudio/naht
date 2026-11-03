use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::iter;

use naht::commands::CommandError;
use naht::config::ConfigError;
use naht::output::Output;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum Failure {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("cannot start logging")]
    Logging(#[source] tracing::subscriber::SetGlobalDefaultError),
    #[error("cannot write to the terminal")]
    Terminal(#[from] io::Error),
}

pub(crate) fn report<Out: Write, Err: Write>(
    out: &mut Output<Out, Err>,
    failure: &Failure,
) -> io::Result<()> {
    out.fail(Chain(failure))
}

struct Chain<'error>(&'error dyn Error);

impl fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)?;
        iter::successors(self.0.source(), |&error| error.source())
            .try_for_each(|cause| write!(f, ": {cause}"))
    }
}
