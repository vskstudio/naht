use std::fmt::Display;
use std::io::{self, Write};

use naht_core::reconciler::Status;

use crate::commands::{Built, Initialized};

/// The command-line report: ordinary lines go to `stdout`, warnings and failures to `stderr`.
#[derive(Debug)]
pub struct Output<Stdout: Write, Stderr: Write> {
    stdout: Stdout,
    stderr: Stderr,
}

impl Output<io::Stdout, io::Stderr> {
    /// An output bound to the process's real standard streams.
    pub fn terminal() -> Self {
        Self {
            stdout: io::stdout(),
            stderr: io::stderr(),
        }
    }
}

impl<Stdout: Write, Stderr: Write> Output<Stdout, Stderr> {
    /// An output writing to the given streams, for tests and embedding.
    pub fn new(stdout: Stdout, stderr: Stderr) -> Self {
        Self { stdout, stderr }
    }

    /// Write one line to standard output.
    pub fn say(&mut self, line: impl Display) -> io::Result<()> {
        writeln!(self.stdout, "{line}")
    }

    /// Write one warning line to standard error, prefixed `naht: warning:`.
    pub fn warn(&mut self, line: impl Display) -> io::Result<()> {
        writeln!(self.stderr, "naht: warning: {line}")
    }

    /// Write one failure line to standard error, prefixed `naht:`.
    pub fn fail(&mut self, line: impl Display) -> io::Result<()> {
        writeln!(self.stderr, "naht: {line}")
    }

    /// Report what `init` created or converted, after any warnings it raised.
    pub fn initialized(&mut self, initialized: &Initialized) -> io::Result<()> {
        match initialized {
            Initialized::Rojo {
                project_file,
                warnings,
            } => {
                for warning in warnings {
                    self.warn(warning)?;
                }
                self.say(format_args!(
                    "Converted Rojo project into {}",
                    project_file.display()
                ))
            }
            Initialized::Fresh { name, root } => self.say(format_args!(
                "Initialized Naht project '{name}' at {}",
                root.display()
            )),
        }
    }

    /// Report the conflicted paths of a project, or that there are none.
    pub fn status(&mut self, status: &Status) -> io::Result<()> {
        if status.conflicted.is_empty() {
            return self.say("clean, no conflicted paths");
        }
        self.say(format_args!(
            "{} conflicted path(s):",
            status.conflicted.len()
        ))?;
        for path in &status.conflicted {
            self.say(format_args!("  {path}"))?;
        }
        Ok(())
    }

    /// Report a finished build: its warnings, then the artifact written.
    pub fn built(&mut self, built: &Built) -> io::Result<()> {
        for warning in &built.warnings {
            self.warn(warning)?;
        }
        self.say(format_args!("built {}", built.output.display()))
    }
}
