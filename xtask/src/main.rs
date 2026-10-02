//! Chores that are not part of the shipped tool.
//!
//! The split matters: `xfina-data` is what the scheduled job builds and runs
//! against production data, so it carries only what that job needs. Releasing,
//! and checking a parser against an archive too large to commit, are developer
//! tasks and live here, reached through `cargo xtask`.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "Release and maintenance tasks for xfina-data")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Bump the version, date the changelog section, and commit
    PrepareRelease {
        /// Which part of the version to bump
        #[arg(value_name = "major|minor|patch")]
        bump: String,
    },

    /// Tag the release on main, once it is merged
    TagRelease,

    /// Check a parser against the full raw archive
    ///
    /// The archive is hundreds of megabytes and is never committed, so this
    /// points at a local directory and is run by hand rather than in CI.
    Parity {
        /// The dataset to check
        #[arg(long)]
        dataset: String,

        /// Directory holding the raw files
        #[arg(long)]
        archive: PathBuf,

        /// Existing derived data to diff against
        #[arg(long)]
        against: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::PrepareRelease { .. } => bail!("prepare-release is not implemented yet"),
        Commands::TagRelease => bail!("tag-release is not implemented yet"),
        Commands::Parity { .. } => bail!("parity is not implemented yet"),
    }
}
