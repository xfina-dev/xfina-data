//! What `data.xfina.dev` serves beyond the CSVs themselves: `metadata.json`,
//! and `site-data.json`, from which the Vue app in `site/` renders the pages.

use std::fs::{self, File};
use std::path::Path;

use chrono::{DateTime, SubsecRound, Utc};
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Frequency, SdmxProvider, Source, Status, Unreadable, TOOL_VERSION};
use crate::error::{Result, XfinaDataError};
use crate::pipeline::{DataDir, METADATA_PATH};
use crate::raw::{self, Manifest};
use crate::series;

/// `metadata.json`: one entry per published series.
#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    /// The tool that wrote these files.
    pub generated_by: String,
    /// Where every manifest's `key` resolves, e.g. `https://raw.data.xfina.dev`.
    pub raw_base_url: String,
    /// Bytes the raw archive holds across every dataset, published or not.
    #[serde(default)]
    pub raw_bytes: u64,
    /// The archive's storage budget, in bytes.
    #[serde(default)]
    pub raw_budget_bytes: u64,
    /// Every published series.
    pub datasets: Vec<Entry>,
}

/// One published series, described.
#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    /// Dataset id, matching the catalog.
    pub id: String,
    /// One-line description.
    pub title: String,
    /// How often the source publishes.
    pub frequency: Frequency,
    /// Path of the CSV below the site root.
    pub path: String,
    /// The CSV header.
    pub columns: Vec<String>,
    /// The terms the data comes under.
    pub licence: String,
    /// Where it comes from.
    pub source: Source,
    /// Data rows in the CSV, header excluded.
    pub rows: usize,
    /// First period.
    pub first: String,
    /// Last period.
    pub last: String,
    /// sha256 of the CSV as served.
    pub sha256: String,
    /// When the CSV's content last changed.
    ///
    /// Carried over unchanged when a run rewrites an identical file, so it
    /// answers "is there anything new?" rather than "did a job run?".
    pub updated_at: DateTime<Utc>,
    /// Path of the raw manifest below the site root.
    pub manifest: String,
    /// How many raw files the manifest records.
    pub raw_files: usize,
    /// How many bytes those files take in the archive.
    #[serde(default)]
    pub raw_bytes: u64,
    /// Archived documents no parser can read, by sha256, with the reason.
    /// The periods they would have dated have no rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<Unreadable>,
}

/// Rewrite `metadata.json` from what is in the data directory.
pub fn write_metadata(catalog: &Catalog, data: &DataDir, now: DateTime<Utc>) -> Result<()> {
    let previous: Option<Metadata> = match fs::read(data.metadata()) {
        Ok(bytes) => Some(
            serde_json::from_slice(&bytes)
                .map_err(|e| XfinaDataError::Store(format!("{METADATA_PATH}: {e}")))?,
        ),
        Err(_) => None,
    };

    let mut entries = Vec::new();
    for dataset in catalog
        .datasets
        .iter()
        .filter(|d| d.status == Status::Published)
    {
        let path = data.series(dataset);
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(&path)?;
        let sha256 = raw::sha256(&bytes);
        let parsed = series::csv::read(bytes.as_slice())?;
        let Some((first, last)) = parsed.range() else {
            continue;
        };
        let updated_at = previous
            .as_ref()
            .and_then(|m| m.datasets.iter().find(|e| e.id == dataset.id))
            .filter(|e| e.sha256 == sha256)
            .map_or(now.trunc_subsecs(0), |e| e.updated_at);
        let manifest = data.manifest(dataset);

        entries.push(Entry {
            id: dataset.id.clone(),
            title: dataset.title.clone(),
            frequency: dataset.frequency,
            path: dataset.output.path.clone(),
            columns: dataset.output.columns.clone(),
            licence: dataset.licence.clone(),
            source: dataset.source.clone(),
            rows: parsed.len(),
            first: first.to_string(),
            last: last.to_string(),
            sha256,
            updated_at,
            manifest: relative(data.root(), &manifest),
            raw_files: Manifest::load(&manifest)?.len(),
            raw_bytes: Manifest::load(&manifest)?.bytes(),
            unreadable: dataset.raw.unreadable.clone(),
        });
    }

    let mut raw_bytes = 0;
    for dataset in &catalog.datasets {
        raw_bytes += Manifest::load(&data.manifest(dataset))?.bytes();
    }
    let metadata = Metadata {
        generated_by: format!("xfina-data {TOOL_VERSION}"),
        raw_base_url: catalog.archive.public_url.clone(),
        raw_bytes,
        raw_budget_bytes: catalog.archive.budget_bytes,
        datasets: entries,
    };
    let path = data.metadata();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| XfinaDataError::Store(format!("{METADATA_PATH}: {e}")))?;
    json.push('\n');
    fs::write(path, json)?;
    Ok(())
}

/// `site-data.json`: everything the site needs to render its pages, read by
/// the Vue app in `site/` when it builds. Every public dataset with a preview
/// is listed under its group, in catalog order, published or not; a published
/// one carries the facts from `metadata.json`.
///
/// Not served, and not part of the `/v1/` contract: it is a build input. The
/// pages it produces are static HTML, so search engines and link previews see
/// each dataset's title, summary and facts without running any script.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SiteData {
    /// Groups in the order the catalog first names them.
    pub groups: Vec<SiteGroup>,
}

/// A heading on the index, with its datasets in catalog order.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SiteGroup {
    /// The heading, e.g. `USD/INR Rates`.
    pub name: String,
    /// Its datasets, in catalog order.
    pub datasets: Vec<SiteDataset>,
}

/// One dataset as the site shows it.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SiteDataset {
    /// Dataset id, matching the catalog and the page's path.
    pub id: String,
    /// One-line description, the page's title.
    pub title: String,
    /// The short name the header's picker shows.
    pub name: String,
    /// One or two sentences on what the series is.
    pub summary: String,
    /// What a value is measured in; labels the y-axis.
    pub unit: String,
    /// How often the source publishes.
    pub frequency: Frequency,
    /// Which value columns to draw, with the names a reader sees.
    pub lines: Vec<crate::catalog::Line>,
    /// Views computed in the browser beside the published values.
    pub views: Vec<crate::catalog::View>,
    /// Absent until the dataset is published: the index then says so, and
    /// it gets no page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<SitePublished>,
}

/// What `metadata.json` says about a published dataset, for its card and page.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SitePublished {
    /// Path of the CSV below the site root.
    pub path: String,
    /// The CSV header.
    pub columns: Vec<String>,
    /// First period.
    pub first: String,
    /// Last period.
    pub last: String,
    /// Data rows in the CSV, header excluded.
    pub rows: usize,
    /// The day the CSV's content last changed, `YYYY-MM-DD`.
    pub updated: String,
    /// The terms the data comes under.
    pub licence: String,
    /// Where it comes from.
    pub source: SiteSource,
    /// Path of the raw manifest below the site root.
    pub manifest: String,
    /// How many raw files the manifest records.
    pub raw_files: usize,
}

/// Where a dataset comes from, in words and as a link.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SiteSource {
    /// The source, named for a reader.
    pub text: String,
    /// The source's own page.
    pub url: String,
}

/// Build `site-data.json` from the catalog and the data directory's
/// `metadata.json`. Without a `metadata.json`, every dataset is unpublished.
pub fn site_data(catalog: &Catalog, data: &DataDir) -> Result<SiteData> {
    let metadata: Option<Metadata> = match File::open(data.metadata()) {
        Ok(file) => Some(
            serde_json::from_reader(file)
                .map_err(|e| XfinaDataError::Store(format!("{METADATA_PATH}: {e}")))?,
        ),
        Err(_) => None,
    };

    let mut groups: Vec<SiteGroup> = Vec::new();
    for dataset in catalog.datasets.iter().filter(|d| d.public) {
        let Some(preview) = &dataset.preview else {
            continue;
        };
        let published = metadata
            .iter()
            .flat_map(|m| &m.datasets)
            .find(|e| e.id == dataset.id)
            .map(|entry| {
                let (text, url) = describe(&entry.source);
                SitePublished {
                    path: entry.path.clone(),
                    columns: entry.columns.clone(),
                    first: entry.first.clone(),
                    last: entry.last.clone(),
                    rows: entry.rows,
                    updated: entry.updated_at.format("%Y-%m-%d").to_string(),
                    licence: entry.licence.clone(),
                    source: SiteSource {
                        text,
                        url: url.to_string(),
                    },
                    manifest: entry.manifest.clone(),
                    raw_files: entry.raw_files,
                }
            });
        let item = SiteDataset {
            id: dataset.id.clone(),
            title: dataset.title.clone(),
            name: preview.name.clone().unwrap_or_else(|| dataset.id.clone()),
            summary: preview.summary.clone(),
            unit: preview.unit.clone(),
            frequency: dataset.frequency,
            lines: preview.lines.clone(),
            views: preview.views.clone(),
            published,
        };
        match groups.iter_mut().find(|g| g.name == preview.group) {
            Some(group) => group.datasets.push(item),
            None => groups.push(SiteGroup {
                name: preview.group.clone(),
                datasets: vec![item],
            }),
        }
    }
    Ok(SiteData { groups })
}

/// Write `site-data.json` for the site's build.
pub fn write_site_data(catalog: &Catalog, data: &DataDir, out: &Path) -> Result<()> {
    let site = site_data(catalog, data)?;
    let mut json = serde_json::to_string_pretty(&site)
        .map_err(|e| XfinaDataError::Store(format!("{}: {e}", out.display())))?;
    json.push('\n');
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, json)?;
    Ok(())
}

fn describe(source: &Source) -> (String, &str) {
    match source {
        Source::SbiForexCard { urls, currency } => (
            format!("State Bank of India forex card rate sheet, {currency} TT rates"),
            urls.first().map_or("https://sbi.bank.in", String::as_str),
        ),
        Source::MospiCpi {
            base_year, series, ..
        } => (
            format!(
                "MoSPI eSankhyiki CPI, base {base_year}, {} series",
                series.join(" and ")
            ),
            "https://esankhyiki.mospi.gov.in/macroindicators?product=cpi",
        ),
        Source::Sdmx {
            provider: SdmxProvider::Bis,
            flow,
            key,
            ..
        } => (
            format!("BIS Data Portal, {flow} series {key}"),
            "https://data.bis.org/topics/XRU",
        ),
        Source::Sdmx {
            provider: SdmxProvider::Imf,
            flow,
            key,
            ..
        } => (
            format!("IMF Data, {flow} dataset, series {key}"),
            "https://data.imf.org/en/datasets/IMF.STA:CPI",
        ),
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
