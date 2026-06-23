//! `naht` — the CLI and localhost sync daemon.
//!
//! This binary parses arguments and dispatches to [`naht::commands`]; all sync decisions live in
//! [`naht_core`] and the daemon modules. Commands: `init` (`--from-rojo` to convert a Rojo project),
//! `serve`, `pull`, `build`, `status`, `resolve`.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use naht::commands;
use naht::config::Config;

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
async fn main() -> Result<()> {
    let cli = Cli::parse();
    naht::logging::init(cli.verbose);
    match cli.command {
        Command::Init { path, from_rojo } => commands::init(&path, from_rojo),
        Command::Serve { path, port } => {
            let config = Config::load(&path)?;
            let port = config.resolve_port(port)?;
            commands::serve(config, &path, port).await
        }
        Command::Pull { path } => {
            let config = Config::load(&path)?;
            commands::pull(&config).await
        }
        Command::Build {
            path,
            output,
            watch,
        } => {
            if watch {
                commands::build_watch(&path, &output).await
            } else {
                commands::build(&path, &output)
            }
        }
        Command::Status { path } => commands::status(&path),
        Command::Resolve { path, project } => commands::resolve(&project, &path),
        Command::PackagePlugin { src, output } => commands::package_plugin(&src, &output),
        Command::CheckVersion { tag } => commands::check_release_version(&tag),
    }
}
