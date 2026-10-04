//! The `xfina-data` command line tool.
//!
//! Every subcommand here is reachable from the nightly job or from a laptop;
//! there is no separate "admin" path. What the job does on a schedule is what
//! a person can do by hand, with the same flags and the same validation —
//! except that writing to the archive needs R2 credentials, which only exist
//! in GitHub Actions.

use std::path::PathBuf;

use anyhow::{bail, Result};
use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use xfina_data::catalog::{validate, Catalog, Origin};
use xfina_data::pipeline::{self, DataDir};
use xfina_data::publish;
use xfina_data::raw::store::{Public, Store};

#[derive(Parser, Debug)]
#[command(
    name = "xfina-data",
    about = "Fetch, archive and publish open Indian financial datasets",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Where the catalog is, and the data directory a command works in.
#[derive(Args, Debug)]
struct Paths {
    /// The dataset catalog
    #[arg(long, default_value = "datasets.yaml")]
    config: PathBuf,

    /// The published tree: series, metadata.json and raw manifests
    #[arg(long, default_value = "data")]
    data: PathBuf,

    /// Archive into, and read raw files from, a local directory laid out like
    /// the bucket instead of R2. For working without credentials, which is
    /// everywhere except GitHub Actions.
    #[arg(long)]
    local_archive: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Fetch what is due, archive it, and rebuild the series it feeds
    Sync {
        #[command(flatten)]
        paths: Paths,

        /// Sync only this dataset
        #[arg(long)]
        dataset: Option<String>,

        /// Fetch and derive, but upload nothing and write nothing
        #[arg(long)]
        dry_run: bool,
    },

    /// Rebuild a series from raw files already in the archive
    ///
    /// This is how a parser fix reaches published data: the raw documents
    /// never change, so improving the parser and rebuilding is always
    /// available and never needs the source to still be serving them.
    Backfill {
        #[command(flatten)]
        paths: Paths,

        /// The dataset to rebuild
        #[arg(long)]
        dataset: String,

        /// Read raw files from a local copy of the archive instead of the
        /// public URL
        #[arg(long)]
        from_dir: Option<PathBuf>,

        /// Write nothing; fail if the published series differs from what the
        /// archive derives
        #[arg(long)]
        check: bool,
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

    /// Write what the site in site/ builds its pages from
    #[command(subcommand)]
    Site(SiteCommands),
}

#[derive(Subcommand, Debug)]
enum RawCommands {
    /// Archive one file that could not be fetched automatically
    Put {
        #[command(flatten)]
        paths: Paths,

        /// The dataset it belongs to
        #[arg(long)]
        dataset: String,

        /// Its key below the dataset's raw prefix, e.g. `2026/2026-09-19.pdf`
        #[arg(long)]
        name: String,

        /// Where these exact bytes were obtained from
        #[arg(long)]
        source_url: String,

        /// How it reached us
        #[arg(long, value_enum, default_value_t = Origin::Manual)]
        origin: Origin,

        /// The file to archive
        file: PathBuf,
    },

    /// Remove archived files whose bytes another archived file holds
    ///
    /// A one-time cleanup for copies an early import stored before imports
    /// deduplicated by content. Reports what it would do unless --apply.
    Dedupe {
        #[command(flatten)]
        paths: Paths,

        /// The dataset whose archive to clean
        #[arg(long)]
        dataset: String,

        /// Delete the duplicates; without this, only report them
        #[arg(long)]
        apply: bool,
    },

    /// Archive a directory of existing raw files in one pass
    Import {
        #[command(flatten)]
        paths: Paths,

        /// The dataset they belong to
        #[arg(long)]
        dataset: String,

        /// Directory to read from; paths below it become keys below the
        /// dataset's raw prefix
        #[arg(long)]
        from: PathBuf,

        /// URL each file's path is appended to for its `source_url`, pinned to
        /// a commit when the files come from a git repository
        #[arg(long)]
        source_url_base: String,

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

        /// A previous catalog to check the published contract against
        ///
        /// A file rather than a git ref, deliberately: CI writes out the base
        /// revision with `git show`, and the tool stays unaware that git
        /// exists.
        #[arg(long)]
        base: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum SiteCommands {
    /// Write the site's build input: every dataset with its preview and, once
    /// published, its facts from metadata.json
    Data {
        #[command(flatten)]
        paths: Paths,

        /// Where to write site-data.json
        #[arg(long, default_value = "site/src/site-data.json")]
        out: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let http = http_client()?;

    match cli.command {
        Commands::Sync {
            paths,
            dataset,
            dry_run,
        } => {
            let catalog = Catalog::load_validated(&paths.config)?;
            let data = DataDir::new(&paths.data);
            let public = Public::new(http.clone(), &catalog.archive.public_url);
            let store = match (dry_run, paths.local_archive) {
                (true, _) => Store::DryRun(public),
                (false, Some(dir)) => Store::Dir(dir),
                (false, None) => Store::r2_from_env(&catalog.archive.bucket, public)?,
            };
            let now = Utc::now();
            let outcome = pipeline::sync(
                &catalog,
                &data,
                &store,
                &http,
                dataset.as_deref(),
                now,
                !dry_run,
            )
            .await;
            // Metadata is rewritten even after a partial failure, so the
            // datasets that did update are described as they now are.
            if !dry_run {
                publish::write_metadata(&catalog, &data, now)?;
            }
            outcome?;
            pipeline::check_budget(&catalog, &data)?;
        }
        Commands::Backfill {
            paths,
            dataset,
            from_dir,
            check,
        } => {
            let catalog = Catalog::load_validated(&paths.config)?;
            let data = DataDir::new(&paths.data);
            let store = match from_dir.or(paths.local_archive) {
                Some(dir) => Store::Dir(dir),
                None => Store::Public(Public::new(http.clone(), &catalog.archive.public_url)),
            };
            pipeline::backfill(&catalog, &data, &store, &dataset, check).await?;
            if !check {
                publish::write_metadata(&catalog, &data, Utc::now())?;
            }
        }
        Commands::Raw(RawCommands::Put {
            paths,
            dataset,
            name,
            source_url,
            origin,
            file,
        }) => {
            let catalog = Catalog::load_validated(&paths.config)?;
            let store = writable_store(&catalog, &http, paths.local_archive.clone())?;
            pipeline::put(
                &catalog,
                &DataDir::new(&paths.data),
                &store,
                &dataset,
                &file,
                &name,
                origin,
                &source_url,
                Utc::now(),
            )
            .await?;
        }
        Commands::Raw(RawCommands::Dedupe {
            paths,
            dataset,
            apply,
        }) => {
            let catalog = Catalog::load_validated(&paths.config)?;
            // A report needs no credentials; only deleting does.
            let store = if apply {
                writable_store(&catalog, &http, paths.local_archive.clone())?
            } else {
                match paths.local_archive.clone() {
                    Some(dir) => Store::Dir(dir),
                    None => Store::Public(Public::new(http.clone(), &catalog.archive.public_url)),
                }
            };
            pipeline::dedupe(
                &catalog,
                &DataDir::new(&paths.data),
                &store,
                &dataset,
                apply,
            )
            .await?;
            if apply {
                publish::write_metadata(&catalog, &DataDir::new(&paths.data), Utc::now())?;
            }
        }
        Commands::Raw(RawCommands::Import {
            paths,
            dataset,
            from,
            source_url_base,
            origin,
        }) => {
            let catalog = Catalog::load_validated(&paths.config)?;
            let store = writable_store(&catalog, &http, paths.local_archive.clone())?;
            pipeline::import(
                &catalog,
                &DataDir::new(&paths.data),
                &store,
                &dataset,
                &from,
                origin,
                &source_url_base,
                Utc::now(),
            )
            .await?;
        }
        Commands::Reconcile { .. } => bail!("reconcile is not implemented yet"),
        Commands::Config(ConfigCommands::Validate { config, base }) => {
            validate_config(config, base)?
        }
        Commands::Site(SiteCommands::Data { paths, out }) => {
            let catalog = Catalog::load_validated(&paths.config)?;
            publish::write_site_data(&catalog, &DataDir::new(&paths.data), &out)?;
            println!("site data written to {}", out.display());
        }
    }
    Ok(())
}

fn writable_store(
    catalog: &Catalog,
    http: &reqwest::Client,
    local: Option<PathBuf>,
) -> Result<Store> {
    if let Some(dir) = local {
        return Ok(Store::Dir(dir));
    }
    let public = Public::new(http.clone(), &catalog.archive.public_url);
    Ok(Store::r2_from_env(&catalog.archive.bucket, public)?)
}

fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        // Sources see who is asking, and can say so if they would rather we
        // did not.
        .user_agent(concat!(
            "xfina-data/",
            env!("CARGO_PKG_VERSION"),
            " (+https://data.xfina.dev)"
        ))
        .timeout(std::time::Duration::from_secs(120))
        .build()?)
}

fn validate_config(config: PathBuf, base: Option<PathBuf>) -> Result<()> {
    let catalog = Catalog::load(&config)?;
    catalog.validate()?;

    // The base catalog is read without validating it. It is whatever was on
    // main, and holding an old file to today's rules would fail pull requests
    // for something their author did not do.
    if let Some(base) = base {
        let previous = Catalog::load(&base)?;
        validate::check_contract(&catalog, &previous)?;
        println!(
            "{}: {} dataset(s) valid, contract kept against {}",
            config.display(),
            catalog.datasets.len(),
            base.display()
        );
    } else {
        println!(
            "{}: {} dataset(s) valid",
            config.display(),
            catalog.datasets.len()
        );
    }

    Ok(())
}
