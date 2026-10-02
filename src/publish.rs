//! What `data.xfina.dev` serves beyond the CSVs themselves: `metadata.json`,
//! the landing page, and the headers that let browsers on other origins read
//! any of it.

use std::collections::HashMap;
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
            unreadable: dataset.raw.unreadable.clone(),
        });
    }

    let metadata = Metadata {
        generated_by: format!("xfina-data {TOOL_VERSION}"),
        raw_base_url: catalog.archive.public_url.clone(),
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

/// Build the deployable site: the data directory, an index of cards, one
/// preview page per published dataset, the assets and the headers.
pub fn build_site(catalog: &Catalog, data: &DataDir, site: &Path, out: &Path) -> Result<()> {
    if out.exists() {
        fs::remove_dir_all(out)?;
    }
    copy_tree(&data.root().join("v1"), &out.join("v1"))?;
    copy_tree(&site.join("assets"), &out.join("assets"))?;
    fs::copy(site.join("_headers"), out.join("_headers"))?;
    fs::copy(site.join("404.html"), out.join("404.html"))?;

    let metadata: Option<Metadata> = match File::open(data.metadata()) {
        Ok(file) => Some(
            serde_json::from_reader(file)
                .map_err(|e| XfinaDataError::Store(format!("{METADATA_PATH}: {e}")))?,
        ),
        Err(_) => None,
    };
    let template = fs::read_to_string(site.join("dataset.html"))?;
    // One header for every page, so the pages cannot drift apart.
    let header = fs::read_to_string(site.join("header.html"))?;

    let mut cards: HashMap<&str, String> = HashMap::new();
    for entry in metadata.iter().flat_map(|m| &m.datasets) {
        let Some(dataset) = catalog.dataset(&entry.id) else {
            continue;
        };
        let Some(preview) = &dataset.preview else {
            continue;
        };
        let page = format!("datasets/{}/", entry.id);

        cards.insert(
            &dataset.id,
            format!(
                "<article class=\"card\"><div class=\"card-header\">\
                 <h3 class=\"card-title\">{title}</h3>\
                 <p class=\"card-description\">{summary}</p></div>\
                 <div class=\"card-content\"><div class=\"facts\">{facts}</div>\
                 <div class=\"actions\"><a class=\"btn btn-sm\" href=\"/{page}\">Preview</a>\
                 <a class=\"btn btn-outline btn-sm\" href=\"/{path}\" download>Download CSV</a>\
                 </div></div></article>\n",
                title = escape(&dataset.title),
                summary = escape(&preview.summary),
                facts = facts(entry),
                path = escape(&entry.path),
            ),
        );

        let config = serde_json::json!({
            "path": entry.path,
            "frequency": dataset.frequency,
            "unit": preview.unit,
            "lines": preview.lines,
            "views": preview.views,
        });
        // Embedded in a <script> element, where `</` would end it early.
        let config = config.to_string().replace("</", "<\\/");
        let (source_text, source_url) = describe(&dataset.source);
        let html = template
            .replace("<!-- HEADER -->", &header)
            .replace("{{ID}}", &escape(&dataset.id))
            .replace("{{TITLE}}", &escape(&dataset.title))
            .replace("{{SUMMARY}}", &escape(&preview.summary))
            .replace("{{FACTS}}", &facts(entry))
            .replace("{{CSV_PATH}}", &escape(&entry.path))
            .replace("{{COLUMNS}}", &escape(&entry.columns.join(",")))
            .replace(
                "{{SOURCE}}",
                &format!(
                    "<a href=\"{}\">{}</a>",
                    escape(source_url),
                    escape(&source_text)
                ),
            )
            .replace("{{LICENCE}}", &escape(&entry.licence))
            .replace("{{MANIFEST_PATH}}", &escape(&entry.manifest))
            .replace("{{RAW_FILES}}", &entry.raw_files.to_string())
            .replace("{{CONFIG_JSON}}", &config);
        let dir = out.join(&page);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("index.html"), html)?;
    }
    // The index lists every public dataset under its group, published or
    // not. One still being brought up says so, so a reader can see what is
    // coming without mistaking it for something they can use today.
    let mut groups: Vec<(&str, String)> = Vec::new();
    for dataset in catalog.datasets.iter().filter(|d| d.public) {
        let Some(preview) = &dataset.preview else {
            continue;
        };
        let card = match cards.remove(dataset.id.as_str()) {
            Some(card) => card,
            None => format!(
                "<article class=\"card\"><div class=\"card-header\">\
                 <h3 class=\"card-title\">{title}</h3>\
                 <p class=\"card-description\">{summary}</p></div>\
                 <div class=\"card-content\"><div class=\"facts\"><span><b>{frequency}</b></span>\
                 <span class=\"badge\">Not published yet</span></div></div></article>\n",
                title = escape(&dataset.title),
                summary = escape(&preview.summary),
                frequency = frequency_name(dataset.frequency),
            ),
        };
        match groups.iter_mut().find(|(name, _)| *name == preview.group) {
            Some((_, html)) => html.push_str(&card),
            None => groups.push((&preview.group, card)),
        }
    }
    let mut sections = String::new();
    for (name, html) in &groups {
        sections.push_str(&format!(
            "<section class=\"group\"><h2 class=\"group-title\">{}</h2>\
             <div class=\"cards\">\n{html}</div></section>\n",
            escape(name)
        ));
    }
    if sections.is_empty() {
        sections.push_str("<p class=\"muted\">Nothing is published yet.</p>\n");
    }

    // Listed in the same order as the groups above them.
    let mut endpoints = Vec::new();
    for (name, _) in &groups {
        for dataset in catalog
            .datasets
            .iter()
            .filter(|d| d.preview.as_ref().is_some_and(|p| p.group == *name))
        {
            if let Some(entry) = metadata
                .iter()
                .flat_map(|m| &m.datasets)
                .find(|e| e.id == dataset.id)
            {
                endpoints.push(format!("https://data.xfina.dev/{}", escape(&entry.path)));
            }
        }
    }
    let endpoints = endpoints.join("\n");

    let page = fs::read_to_string(site.join("index.html"))?
        .replace("<!-- HEADER -->", &header)
        .replace("<!-- GROUPS -->", &sections)
        .replace("<!-- ENDPOINTS -->", &endpoints);
    fs::write(out.join("index.html"), page)?;
    Ok(())
}

/// The one-line facts under a dataset's title.
fn facts(entry: &Entry) -> String {
    let frequency = frequency_name(entry.frequency);
    format!(
        "<span><b>{frequency}</b></span><span><b>{}</b> → <b>{}</b></span>\
         <span><b>{}</b> rows</span><span>updated {}</span>",
        escape(&entry.first),
        escape(&entry.last),
        entry.rows,
        entry.updated_at.format("%Y-%m-%d"),
    )
}

fn frequency_name(frequency: Frequency) -> &'static str {
    match frequency {
        Frequency::Daily => "Daily",
        Frequency::Monthly => "Monthly",
    }
}

/// Where a dataset comes from, in words a reader would use, and a link.
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

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if !entry.file_name().to_string_lossy().ends_with(".tmp") {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
