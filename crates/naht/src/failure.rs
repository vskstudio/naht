use std::error::Error;
use std::fmt;
use std::io;
use std::iter;

use naht::commands::CommandError;
use naht::config::ConfigError;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum Failure {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error("cannot start logging")]
    Logging(#[source] tracing::subscriber::SetGlobalDefaultError),
}

pub(crate) fn report(failure: &Failure) -> io::Result<()> {
    eprintln!("naht: {}", Chain(failure));
    Ok(())
}

struct Chain<'error>(&'error dyn Error);

impl fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_string())?;
        iter::successors(self.0.source(), |&error| error.source())
            .try_for_each(|cause| write!(f, ": {cause}"))
    }
}
