//! CLI argument definitions using clap.
//!
//! The CLI supports three subcommands: `config`, `update`, and `render`.
//! The `config` command accepts flags for non-interactive operation.

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Arguments for the `config` subcommand.
///
/// Controls whether the configuration wizard runs interactively or uses defaults.
/// Non-interactive mode is triggered by `--yes` or `--config-from`.
#[derive(Args, Debug)]
pub struct ConfigArgs {
    /// Run non-interactively using detected defaults
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Use PDF settings from existing config file (re-scans repo for files/authors)
    #[arg(long, value_name = "FILE")]
    pub config_from: Option<PathBuf>,

    /// Override output PDF path (enables PDF output in non-interactive mode)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Generates a src-book.toml config file
    Config(ConfigArgs),
    /// Renders the book according to the contents of the src-book.toml config file
    Render,
    /// Refreshes file lists and authors without re-running the full config wizard
    Update,
}

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}
