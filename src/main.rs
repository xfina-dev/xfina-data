//! The `xfinata` command line tool.
//!
//! Every subcommand here is reachable from the nightly job or from a laptop;
//! there is no separate "admin" path. What the job does on a schedule is what
//! a person can do by hand, with the same flags and the same validation.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "xfinata",
    about = "Fetch, archive and publish open Indian financial datasets",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Fetch what is due, archive it, and rebuild the series it feeds
    Sync {
        /// The dataset catalog
        #[arg(long, default_value = "datasets.yaml")]
        config: PathBuf,

        /// Sync only this dataset, whether or not it is due
        #[arg(long)]
        dataset: Option<String>,

        /// Do everything except write: no upload, no commit, no deploy
        #[arg(long)]
        dry_run: bool,
    },

    /// Rebuild a series from raw files already in the archive
    ///
    /// This is how a parser fix reaches published data: the raw documents
    /// never change, so improving the parser and rebuilding is always
    /// available and never needs the source to still be serving them.
    Backfill {
        /// The dataset to rebuild
        #[arg(long)]
        dataset: String,

        /// Read raw files from a local directory instead of the archive
        #[arg(long)]
        from_dir: Option<PathBuf>,
    },

    /// Work with the raw archive directly
    #[command(subcommand)]
    Raw(RawCommands),

    /// Recover raw files for dates we are missing, from a secondary source
    Reconcile {
        /// The dataset to reconcile
        #[arg(long)]
        dataset: String,
    },

    /// Work with the dataset catalog
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Build the published site into a directory
    #[command(subcommand)]
    Site(SiteCommands),
}

#[derive(Subcommand, Debug)]
enum RawCommands {
    /// Archive one file that could not be fetched automatically
    Put {
        /// The dataset it belongs to
        #[arg(long)]
        dataset: String,

        /// Where this file came from
        #[arg(long, value_enum, default_value_t = Origin::Manual)]
        origin: Origin,

        /// The file to archive
        file: PathBuf,
    },

    /// Archive a directory of existing raw files in one pass
    Import {
        /// The dataset they belong to
        #[arg(long)]
        dataset: String,

        /// Directory to read from
        #[arg(long)]
        from: PathBuf,

        /// Where these files came from
        #[arg(long, value_enum)]
        origin: Origin,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Check the catalog, and that it does not break the published contract
    Validate {
        /// The dataset catalog
        #[arg(long, default_value = "datasets.yaml")]
        config: PathBuf,

        /// Compare against the catalog at this git ref, failing on a
        /// removed or renamed published path
        #[arg(long)]
        base: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum SiteCommands {
    /// Render the CSVs, metadata and landing page into a directory
    Build {
        /// Destination directory
        #[arg(long, default_value = "dist")]
        out: PathBuf,
    },
}

/// How a raw file reached the archive.
///
/// Recorded per file rather than per dataset, because a single series mixes
/// them: most days arrive from the source, some are recovered from a mirror
/// after a missed run, and a few are only ever downloaded by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Origin {
    /// Fetched by a scheduled run
    Auto,
    /// Downloaded by a person and handed to `raw put`
    Manual,
    /// Recovered from a secondary mirror
    Upstream,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Sync { .. } => bail!("sync is not implemented yet"),
        Commands::Backfill { .. } => bail!("backfill is not implemented yet"),
        Commands::Raw(RawCommands::Put { .. }) => bail!("raw put is not implemented yet"),
        Commands::Raw(RawCommands::Import { .. }) => bail!("raw import is not implemented yet"),
        Commands::Reconcile { .. } => bail!("reconcile is not implemented yet"),
        Commands::Config(ConfigCommands::Validate { .. }) => {
            bail!("config validate is not implemented yet")
        }
        Commands::Site(SiteCommands::Build { .. }) => bail!("site build is not implemented yet"),
    }
}
