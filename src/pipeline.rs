//! The jobs the CLI runs: sync, backfill, and getting files into the archive.
//!
//! Every job works against a data directory laid out exactly as
//! `data.xfina.dev` serves it — the published CSVs, `metadata.json` and one
//! raw manifest per dataset — so what a run leaves behind is what gets
//! deployed, with nothing in between to drift.

use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Asia::Kolkata;

use crate::catalog::{Catalog, Dataset, Origin, Status};
use crate::error::{Result, XfinaDataError};
use crate::raw::store::Store;
use crate::raw::{self, Dedupe, Incoming, Manifest, RawFile};
use crate::series::{self, Series};
use crate::sources;

/// The published tree, as a directory on disk.
pub struct DataDir(PathBuf);

impl DataDir {
    /// Wrap a directory; it is created as files are written into it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self(root.into())
    }

    /// The directory itself.
    pub fn root(&self) -> &Path {
        &self.0
    }

    /// Where a dataset's raw manifest lives.
    pub fn manifest(&self, dataset: &Dataset) -> PathBuf {
        self.0
            .join(MANIFEST_DIR)
            .join(format!("{}.csv", dataset.id))
    }

    /// Where a dataset's published series lives.
    pub fn series(&self, dataset: &Dataset) -> PathBuf {
        self.0.join(&dataset.output.path)
    }

    /// Where `metadata.json` lives.
    pub fn metadata(&self) -> PathBuf {
        self.0.join(METADATA_PATH)
    }
}

/// Manifests sit under the published root, so they version with the series.
pub const MANIFEST_DIR: &str = "v1/manifests";

/// The one file describing every published series.
pub const METADATA_PATH: &str = "v1/metadata.json";

/// The calendar date in India at `now`.
///
/// Used only to name archive keys and to decide what to fetch. No published
/// date ever comes from here (rule 9); those come from inside documents.
pub fn today_in_india(now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(&Kolkata).date_naive()
}

/// Fetch, archive, and — for published datasets — rebuild the series.
///
/// A dataset that fails does not stop the others: an SBI outage is no reason
/// to skip CPI. The run still fails at the end, naming every dataset that did
/// not update, because a dataset that did not update must say so (rule 14).
pub async fn sync(
    catalog: &Catalog,
    data: &DataDir,
    store: &Store,
    http: &reqwest::Client,
    only: Option<&str>,
    now: DateTime<Utc>,
    write: bool,
) -> Result<()> {
    let datasets = select(catalog, only)?;
    let mut failures = Vec::new();

    for dataset in datasets {
        match sync_one(dataset, data, store, http, now, write).await {
            Ok(summary) => println!("{}: {summary}", dataset.id),
            Err(e) => {
                eprintln!("{}: FAILED: {e}", dataset.id);
                failures.push(format!("{}: {e}", dataset.id));
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(XfinaDataError::Incomplete(format!(
            "{} dataset(s) did not update:\n{}",
            failures.len(),
            failures.join("\n")
        )))
    }
}

async fn sync_one(
    dataset: &Dataset,
    data: &DataDir,
    store: &Store,
    http: &reqwest::Client,
    now: DateTime<Utc>,
    write: bool,
) -> Result<String> {
    let manifest_path = data.manifest(dataset);
    let mut manifest = Manifest::load(&manifest_path)?;

    let documents = sources::fetch(dataset, http, today_in_india(now), manifest.is_empty()).await?;

    // Bytes of what this run archived, kept so deriving them does not read
    // them straight back from the bucket.
    let mut fresh: HashMap<String, Vec<u8>> = HashMap::new();
    let mut new_files = Vec::new();
    for document in documents {
        let bytes = document.bytes.clone();
        let incoming = Incoming {
            key: format!("{}/{}", dataset.raw.prefix, document.name),
            bytes: document.bytes,
            content_type: document.content_type,
            origin: Origin::Auto,
            source_url: document.source_url,
            fetched_at: now,
        };
        if let Some(file) = raw::archive(store, &mut manifest, incoming, Dedupe::BySha).await? {
            println!("{}: archived {}", dataset.id, file.key);
            fresh.insert(file.key.clone(), bytes);
            new_files.push(file);
        }
    }

    // Saved before anything is derived. What reached the archive is recorded
    // even if the parser then rejects it, and that file stays exactly where
    // it is for the fixed parser to read later.
    if write {
        manifest.save(&manifest_path)?;
    }

    if dataset.status == Status::Planned {
        return Ok(format!(
            "{} new raw file(s), {} held; planned, so nothing is published",
            new_files.len(),
            manifest.len()
        ));
    }

    let series_path = data.series(dataset);
    let (series, report) = if series_path.exists() {
        if new_files.is_empty() {
            return Ok(format!(
                "no new raw files; {} unchanged",
                dataset.output.path
            ));
        }
        let mut published = series::csv::read(File::open(&series_path)?)?;
        let docs = load(store, &readable(dataset, &new_files), &fresh).await?;
        let derived = derive(dataset, &docs)?;
        let report = published.merge(&derived)?;
        (published, report.to_string())
    } else {
        // Nothing published yet: the whole series, from everything archived.
        let files = manifest.files();
        let docs = load(store, &readable(dataset, files.iter().copied()), &fresh).await?;
        let derived = derive(dataset, &docs)?;
        let rows = derived.len();
        (
            derived,
            format!("{rows} row(s) derived from {} raw file(s)", files.len()),
        )
    };

    series.check_invariants()?;
    if write {
        write_series(&series, &series_path)?;
    }
    Ok(report)
}

/// Rebuild a published series from nothing but its archived raw files.
///
/// This is how a parser fix reaches published data, and how anyone can check
/// that what is served is what the archive says. With `check`, nothing is
/// written and any difference is an error.
pub async fn backfill(
    catalog: &Catalog,
    data: &DataDir,
    store: &Store,
    id: &str,
    check: bool,
) -> Result<()> {
    let dataset = one(catalog, id)?;
    let manifest = Manifest::load(&data.manifest(dataset))?;
    if manifest.is_empty() {
        return Err(XfinaDataError::Store(format!(
            "{id}: no raw files are recorded in {}",
            data.manifest(dataset).display()
        )));
    }

    let files = manifest.files();
    let docs = load(
        store,
        &readable(dataset, files.iter().copied()),
        &HashMap::new(),
    )
    .await?;
    let derived = derive(dataset, &docs)?;
    derived.check_invariants()?;

    let path = data.series(dataset);
    let differences = if path.exists() {
        let published = series::csv::read(File::open(&path)?)?;
        let removed = published
            .iter()
            .filter(|(key, _)| derived.get(key).is_none())
            .count();
        let mut compare = published;
        let report = compare.merge(&derived)?;
        println!("{id}: {report}, {removed} only in the published file");
        !report.is_empty() || removed > 0
    } else {
        println!("{id}: {} row(s), nothing published yet", derived.len());
        true
    };

    if check {
        if differences {
            return Err(XfinaDataError::Parse(format!(
                "{id}: the published series is not what its archived raw files derive"
            )));
        }
        println!(
            "{id}: published series matches its {} raw file(s)",
            files.len()
        );
        return Ok(());
    }

    write_series(&derived, &path)
}

/// Archive every file under `from`, keyed by its path below it.
///
/// For bringing an existing archive in, such as the predecessor's PDFs. Run
/// twice, it archives nothing new: each file is either recorded already or
/// adopted from the bucket if a previous run uploaded it and then died.
#[allow(clippy::too_many_arguments)]
pub async fn import(
    catalog: &Catalog,
    data: &DataDir,
    store: &Store,
    id: &str,
    from: &Path,
    origin: Origin,
    source_url_base: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let dataset = one(catalog, id)?;
    let manifest_path = data.manifest(dataset);
    let mut manifest = Manifest::load(&manifest_path)?;

    let mut paths = Vec::new();
    walk(from, &mut paths)?;
    paths.sort();

    let base = source_url_base.trim_end_matches('/');
    let mut added = 0;
    for (index, path) in paths.iter().enumerate() {
        let relative = path
            .strip_prefix(from)
            .map_err(|e| XfinaDataError::Store(format!("{}: {e}", path.display())))?
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let incoming = Incoming {
            key: format!("{}/{relative}", dataset.raw.prefix),
            bytes: fs::read(path)?,
            content_type: raw::content_type_for(&relative)?,
            origin,
            source_url: format!("{base}/{relative}"),
            fetched_at: now,
        };
        if raw::archive(store, &mut manifest, incoming, Dedupe::ByKey)
            .await?
            .is_some()
        {
            added += 1;
        }
        // Saved as it goes, so a run that dies part-way through leaves a
        // manifest naming everything it did upload.
        if (index + 1) % 100 == 0 {
            manifest.save(&manifest_path)?;
            println!("{id}: {}/{} file(s) processed", index + 1, paths.len());
        }
    }

    manifest.save(&manifest_path)?;
    println!(
        "{id}: {added} new file(s) archived, {} already held, {} in the manifest",
        paths.len() - added,
        manifest.len()
    );
    Ok(())
}

/// Archive one file that could not be fetched automatically.
#[allow(clippy::too_many_arguments)]
pub async fn put(
    catalog: &Catalog,
    data: &DataDir,
    store: &Store,
    id: &str,
    file: &Path,
    name: &str,
    origin: Origin,
    source_url: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let dataset = one(catalog, id)?;
    let manifest_path = data.manifest(dataset);
    let mut manifest = Manifest::load(&manifest_path)?;
    let incoming = Incoming {
        key: format!("{}/{name}", dataset.raw.prefix),
        bytes: fs::read(file)?,
        content_type: raw::content_type_for(name)?,
        origin,
        source_url: source_url.to_string(),
        fetched_at: now,
    };
    match raw::archive(store, &mut manifest, incoming, Dedupe::ByKey).await? {
        Some(file) => println!("{id}: archived {}", file.key),
        None => println!("{id}: already archived, nothing to do"),
    }
    manifest.save(&manifest_path)
}

/// The files a derivation reads: all of them except those the catalog lists
/// as unreadable.
///
/// Those stay in the archive but are never downloaded or handed to a parser.
/// Each skip is printed with its reason, so the run log says exactly which
/// periods have no row and why.
fn readable<'a>(
    dataset: &Dataset,
    files: impl IntoIterator<Item = &'a RawFile>,
) -> Vec<&'a RawFile> {
    files
        .into_iter()
        .filter(|file| {
            match dataset
                .raw
                .unreadable
                .iter()
                .find(|u| u.sha256 == file.sha256)
            {
                Some(listed) => {
                    println!(
                        "{}: skipped {}, listed as unreadable: {}",
                        dataset.id, file.key, listed.reason
                    );
                    false
                }
                None => true,
            }
        })
        .collect()
}

fn derive(dataset: &Dataset, docs: &[(&RawFile, Vec<u8>)]) -> Result<Series> {
    let derived = sources::derive(dataset, docs)?;
    for note in &derived.notes {
        println!("{}: note: {note}", dataset.id);
    }
    Ok(derived.series)
}

async fn load<'a>(
    store: &Store,
    files: &[&'a RawFile],
    fresh: &HashMap<String, Vec<u8>>,
) -> Result<Vec<(&'a RawFile, Vec<u8>)>> {
    let mut docs = Vec::with_capacity(files.len());
    for file in files {
        let bytes = match fresh.get(&file.key) {
            Some(bytes) => bytes.clone(),
            None => raw::read_verified(store, file).await?,
        };
        docs.push((*file, bytes));
    }
    Ok(docs)
}

fn write_series(series: &Series, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Written beside the target and renamed over it, so a run that dies
    // mid-write never leaves a truncated CSV to be deployed.
    let temporary = path.with_extension("csv.tmp");
    series::csv::write(series, File::create(&temporary)?)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path
            .file_name()
            .is_some_and(|n| !n.to_string_lossy().starts_with('.'))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn one<'a>(catalog: &'a Catalog, id: &str) -> Result<&'a Dataset> {
    catalog
        .dataset(id)
        .ok_or_else(|| XfinaDataError::Config(format!("no dataset `{id}` in the catalog")))
}

fn select<'a>(catalog: &'a Catalog, only: Option<&str>) -> Result<Vec<&'a Dataset>> {
    match only {
        Some(id) => Ok(vec![one(catalog, id)?]),
        None => Ok(catalog.datasets.iter().collect()),
    }
}
