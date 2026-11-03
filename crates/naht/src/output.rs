use std::fmt::Display;
use std::io::{self, Write};

use naht_core::reconciler::Status;

use crate::commands::{Built, Initialized};

pub struct Output<Out: Write, Err: Write> {
    out: Out,
    err: Err,
}

impl Output<io::Stdout, io::Stderr> {
    pub fn terminal() -> Self {
        Self {
            out: io::stdout(),
            err: io::stderr(),
        }
    }
}

impl<Out: Write, Err: Write> Output<Out, Err> {
    pub fn new(out: Out, err: Err) -> Self {
        Self { out, err }
    }

    pub fn say(&mut self, line: impl Display) -> io::Result<()> {
        writeln!(self.out, "{line}")
    }

    pub fn warn(&mut self, line: impl Display) -> io::Result<()> {
        writeln!(self.err, "naht: warning: {line}")
    }

    pub fn fail(&mut self, line: impl Display) -> io::Result<()> {
        writeln!(self.err, "naht: {line}")
    }

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

    pub fn built(&mut self, built: &Built) -> io::Result<()> {
        for warning in &built.warnings {
            self.warn(warning)?;
        }
        self.say(format_args!("built {}", built.output.display()))
    }
}
