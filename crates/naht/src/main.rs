//! `naht`, the CLI and localhost sync daemon.
//!
//! This binary parses arguments and dispatches to [`naht::commands`]; all sync decisions live in
//! [`naht_core`] and the daemon modules. Commands: `init` (`--from-rojo` to convert a Rojo project),
//! `serve`, `pull`, `build`, `status`, `resolve`.
//!
//! | Exit code | Meaning |
//! |---|---|
//! | 0 | the command succeeded |
//! | 2 | the command line is wrong, reported by `clap` |
//! | 65 | the project's input is malformed or the request conflicts with it |
//! | 66 | an input file or the project directory cannot be read |
//! | 69 | the daemon is not reachable, or its port cannot be bound |
//! | 70 | an internal failure that is not the operator's doing |
//! | 71 | the file watcher cannot start |
//! | 73 | an output file or project file cannot be created |
//! | 74 | a file or the state database cannot be read or written |
//! | 78 | the configuration is invalid |

mod exit;
mod failure;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use naht::commands;
use naht::config::Config;
use naht::output::Output;

use crate::exit::Exit;
use crate::failure::Failure;

/// Bidirectional, conflict-safe filesystem sync for Roblox Studio.
#[derive(Parser)]
#[command(name = "naht", version, about)]
struct Cli {
    /// Raise the log level (repeat for more: `-v` debug, `-vv` trace). `NAHT_LOG`/`RUST_LOG` override.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new project (or convert a Rojo one with `--from-rojo`).
    Init {
        /// The project directory; defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Convert an existing `default.project.json` instead of scaffolding.
        #[arg(long)]
        from_rojo: bool,
    },
    /// Run the sync daemon for a project.
    Serve {
        /// The project directory; defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Override the configured/default port.
        #[arg(long)]
        port: Option<u16>,
    },
    /// Ask a running daemon to re-sync now.
    Pull {
        /// The project directory; defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Build the project into a Roblox model (`.rbxm`/`.rbxmx`) or place (`.rbxl`/`.rbxlx`) file.
    Build {
        /// The project directory; defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// The output file; the extension picks model vs place and binary vs XML.
        #[arg(short, long)]
        output: PathBuf,
        /// Rebuild the output on every change instead of building once.
        #[arg(long)]
        watch: bool,
    },
    /// Show the project's conflict state.
    Status {
        /// The project directory; defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Clear a resolved conflict once its markers are gone.
    Resolve {
        /// The conflicted path, as listed by `status`.
        path: String,
        /// The project directory; defaults to the current directory.
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    /// Package the Studio plugin into an installable `.rbxmx` (used by the release pipeline).
    PackagePlugin {
        /// The plugin source directory.
        #[arg(long, default_value = "plugin/src")]
        src: PathBuf,
        /// The output `.rbxmx` file.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Verify a release tag matches the workspace version (used by the release pipeline).
    CheckVersion {
        /// The tag to check, e.g. `v0.1.0`.
        tag: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut out = Output::terminal();
    let exit = match naht::logging::init(cli.verbose) {
        Ok(()) => match dispatch(cli.command, &mut out).await {
            Ok(()) => Exit::Success,
            Err(failure) => {
                failure::report(&mut out, &failure).map_or(Exit::IoError, |()| Exit::from(&failure))
            }
        },
        Err(error) => failure::report(&mut out, &Failure::Logging(error))
            .map_or(Exit::IoError, |()| Exit::Software),
    };
    exit.into()
}

async fn dispatch<Out: Write, Err: Write>(
    command: Command,
    out: &mut Output<Out, Err>,
) -> Result<(), Failure> {
    match command {
        Command::Init { path, from_rojo } => {
            let initialized = commands::init(&path, from_rojo)?;
            out.initialized(&initialized)?;
        }
        Command::Serve { path, port } => {
            let config = Config::load(&path)?;
            let port = config.resolve_port(port)?;
            commands::serve(config, &path, port, out).await?;
        }
        Command::Pull { path } => {
            let config = Config::load(&path)?;
            commands::pull(&config).await?;
            out.say("pulled, daemon re-synced")?;
        }
        Command::Build {
            path,
            output,
            watch: true,
        } => {
            let (built, _watcher) = commands::build_watch(&path, &output)?;
            out.built(&built)?;
            out.say(format_args!(
                "watching {}, rebuilding on change",
                path.display()
            ))?;
            std::future::pending::<()>().await;
        }
        Command::Build {
            path,
            output,
            watch: false,
        } => {
            let built = commands::build(&path, &output)?;
            out.built(&built)?;
        }
        Command::Status { path } => {
            let status = commands::status(&path)?;
            out.status(&status)?;
        }
        Command::Resolve { path, project } => {
            commands::resolve(&project, &path)?;
            out.say(format_args!("resolved {path}"))?;
        }
        Command::PackagePlugin { src, output } => {
            commands::package_plugin(&src, &output)?;
            out.say(format_args!("packaged plugin into {}", output.display()))?;
        }
        Command::CheckVersion { tag } => {
            commands::check_release_version(&tag)?;
            out.say(format_args!(
                "release tag '{tag}' matches workspace version '{}'",
                naht_core::version()
            ))?;
        }
    }
    Ok(())
}
