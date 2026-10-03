//! The dataset catalog: what exists, where it comes from, and what it promises.
//!
//! `datasets.yaml` is the only configuration this tool has. Adding a dataset
//! is a row there plus a source module, never a new command, so the catalog is
//! also the honest answer to "what does data.xfina.dev serve?".
//!
//! The half of this that matters most is [`validate`], because a published
//! path is a promise to people whose code we cannot see.

pub mod validate;

use std::path::Path;

use clap::ValueEnum;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::{Result, XfinaDataError};

/// The tool version this build reports, used to enforce `min_tool_version`.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The parsed contents of `datasets.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Catalog {
    /// Schema version of the catalog file itself.
    pub version: u32,

    /// The oldest tool version that understands this catalog.
    ///
    /// The nightly job runs a pinned release binary, so a catalog change that
    /// needs newer code would otherwise fail somewhere deep in a source
    /// module, at 02:00, with a confusing message. This turns that into one
    /// clear refusal at startup.
    pub min_tool_version: Version,

    /// Where raw documents are kept.
    pub archive: Archive,

    /// Every dataset this project knows about.
    pub datasets: Vec<Dataset>,
}

/// The raw archive: one bucket, readable by anyone at a public URL.
///
/// Neither value is a secret. The credentials that can write to the bucket
/// reach the tool through the environment and never through this file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Archive {
    /// The R2 bucket name, e.g. `xfina-data-raw`.
    pub bucket: String,

    /// Where the bucket is served, e.g. `https://raw.data.xfina.dev`.
    ///
    /// Re-deriving a series reads raw files back from here, so anyone can do
    /// it without a key, and so can we.
    pub public_url: String,

    /// How many bytes the archive may hold: R2's free tier, 10 GB. Every
    /// sync reports usage against it, warns at 80% and fails at 95%.
    ///
    /// Defaulted rather than required, like every field added after a
    /// catalog has shipped: the contract check reads the catalog on `main`
    /// with this build, and an older file must still parse.
    #[serde(default = "default_budget_bytes")]
    pub budget_bytes: u64,
}

/// R2's free tier.
fn default_budget_bytes() -> u64 {
    10_000_000_000
}

/// One dataset: a single published time series and the source behind it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dataset {
    /// Stable identifier, lowercase and hyphenated.
    pub id: String,

    /// One line describing what the series is, shown in `metadata.json`.
    pub title: String,

    /// How often the source publishes.
    pub frequency: Frequency,

    /// Whether this series is live, and so whether its path is frozen.
    pub status: Status,

    /// Whether the source may be republished at all.
    ///
    /// Some reference data cannot be redistributed. Such a dataset can be
    /// fetched and archived privately but never published, and validation
    /// refuses to let one reach [`Status::Published`].
    pub public: bool,

    /// The terms the source data comes under, recorded in `metadata.json`.
    pub licence: String,

    /// Where the data comes from and how to fetch it.
    pub source: Source,

    /// How the raw documents are archived.
    pub raw: Raw,

    /// What gets published, and under which columns.
    pub output: Output,

    /// How the dataset is shown on its page at data.xfina.dev.
    ///
    /// Presentation only: nothing here changes a published value. Required
    /// for a published dataset, so nobody has to download a series to see
    /// what it looks like before deciding to use it.
    pub preview: Option<Preview>,
}

/// How a dataset's page charts it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Preview {
    /// The heading this dataset is listed under on the index, e.g.
    /// `USD/INR Rates`. Groups appear in the order the catalog first names
    /// them, and datasets within a group in catalog order.
    pub group: String,

    /// One or two sentences on what the series is, for its card and page.
    pub summary: String,

    /// What a value is measured in, e.g. `INR per USD`. Labels the y-axis.
    pub unit: String,

    /// Which value columns to draw, each with the name a reader sees.
    pub lines: Vec<Line>,

    /// Views offered beside the published values themselves.
    #[serde(default)]
    pub views: Vec<View>,
}

/// One drawn column.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Line {
    /// A value column of the published CSV.
    pub column: String,
    /// What the legend and tooltip call it.
    pub label: String,
}

/// A view computed in the reader's browser from the published values.
///
/// Shown as a chart only, labelled as computed, and never written to any
/// file: the published CSV stays the only source of numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum View {
    /// Percentage change on the same month a year earlier.
    YearOnYear,
}

/// How often a source publishes, which decides what a period key means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Frequency {
    /// A value per publishing day. Gaps are normal: weekends and holidays.
    Daily,
    /// A value per calendar month. Gaps are a defect.
    Monthly,
}

/// Whether a dataset is live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Declared but not yet served. Its shape may still change freely.
    Planned,
    /// Being served. Its path and columns are frozen within this `/vN`.
    Published,
}

/// Where a dataset's data comes from.
///
/// One variant per source shape rather than per dataset, so a second currency
/// or a second index from the same publisher is a catalog row, not new code.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    /// SBI's daily forex card rate sheet, published as a PDF.
    SbiForexCard {
        /// Where to fetch it, tried in order.
        urls: Vec<String>,
        /// Which currency's TT rates to publish, e.g. `USD`.
        currency: String,
    },

    /// MoSPI's consumer price index, over its REST API.
    MospiCpi {
        /// CPI base year, e.g. `2024`.
        base_year: String,
        /// Which series to stitch together, oldest first, e.g. `Back` then
        /// `Current`. Where two cover the same month the later one wins, and
        /// the last is the live one a scheduled run keeps up to date.
        series: Vec<String>,
        /// The first year any of them covers.
        ///
        /// MoSPI's filter listing names the live series' years but not the
        /// back series', so where history starts has to be stated.
        first_year: i32,
        /// MoSPI's code for the state; `1` is All India.
        state_code: u32,
        /// MoSPI's code for the sector; `3` is Combined.
        sector_code: u32,
        /// MoSPI's code for the division; `00` is CPI (General).
        ///
        /// A string, because the API distinguishes `00` from `0` in its own
        /// listings even where it accepts both.
        division_code: String,
    },

    /// A statistics agency's SDMX API, read as CSV: the BIS or the IMF.
    ///
    /// One variant for both, because both publish one series per key with
    /// the same two columns that matter — `TIME_PERIOD` and `OBS_VALUE` — and
    /// another series from either is then a catalog row, not new code.
    Sdmx {
        /// Which agency.
        provider: SdmxProvider,
        /// The dataflow, e.g. `WS_XRU` or `CPI`.
        flow: String,
        /// The dataflow's version, e.g. `1.0` or `5.0.0`.
        version: String,
        /// The series key, e.g. `D.IN.INR.A`.
        key: String,
        /// The first period to ask for when nothing is archived yet.
        start: String,
        /// After the first run, ask only for this many days back, enough to
        /// pick up revisions. Absent means fetch the whole series each time,
        /// which suits a small one; identical responses are not re-archived.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refetch_days: Option<u32>,
    },
}

/// An agency publishing over SDMX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SdmxProvider {
    /// The Bank for International Settlements, `stats.bis.org`.
    Bis,
    /// The International Monetary Fund, `api.imf.org`.
    Imf,
}

/// How a dataset's raw documents are archived.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Raw {
    /// Key prefix within the archive, e.g. `sbi/forex-card-rates`.
    pub prefix: String,

    /// How files normally arrive for this dataset.
    ///
    /// Per-file origin is recorded in the manifest and can differ: a day
    /// recovered after a missed run is `upstream` even here.
    pub origin: Origin,

    /// A secondary source to recover missed documents from, if one exists.
    pub recover_from: Option<String>,

    /// Archived documents that no parser can read, each with the reason.
    ///
    /// Every archived document must otherwise yield its rows or fail the
    /// derivation, because a silently skipped document looks exactly like a
    /// holiday in the published CSV. This list is the one way past that rule,
    /// and it is public: each entry is in the catalog and in `metadata.json`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<Unreadable>,
}

/// One archived document that is kept but never read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Unreadable {
    /// Its sha256, so every archived copy of the same bytes is covered —
    /// SBI serves one sheet under several dates.
    pub sha256: String,
    /// Why it cannot be read, for the people the gap affects.
    pub reason: String,
}

/// How a raw file reached the archive.
///
/// Recorded per file rather than per dataset, because a single series mixes
/// them: most days arrive from the source, some are recovered from a mirror
/// after a missed run, and a few are only ever downloaded by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Origin {
    /// Fetched by a scheduled run.
    Auto,
    /// Downloaded by a person and handed to `raw put`.
    Manual,
    /// Recovered from a secondary mirror.
    Upstream,
}

/// What a dataset publishes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Output {
    /// Path under the published root, e.g. `v1/fx/sbi-forex-card-usd.csv`.
    pub path: String,

    /// The CSV header, in order. The first column is the period key.
    pub columns: Vec<String>,
}

impl Catalog {
    /// Read and parse a catalog, without validating it.
    ///
    /// Callers that are about to act on the catalog should use
    /// [`Catalog::load_validated`] instead; this exists for the contract
    /// check, which has to read an older catalog that may no longer pass
    /// today's rules.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_yaml(&text).map_err(|e| match e {
            // Name the file. A parse error with no path is useless once there
            // is more than one catalog in play, which the contract check
            // guarantees there will be.
            XfinaDataError::Config(message) => {
                XfinaDataError::Config(format!("{}: {message}", path.display()))
            }
            other => other,
        })
    }

    /// Parse a catalog from YAML text, without validating it.
    pub fn from_yaml(text: &str) -> Result<Self> {
        serde_yaml_ng::from_str(text).map_err(|e| XfinaDataError::Config(e.to_string()))
    }

    /// Read, parse and validate a catalog.
    pub fn load_validated(path: &Path) -> Result<Self> {
        let catalog = Self::load(path)?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Check every rule, reporting all violations rather than the first.
    pub fn validate(&self) -> Result<()> {
        validate::check(self)
    }

    /// Find a dataset by id.
    pub fn dataset(&self, id: &str) -> Option<&Dataset> {
        self.datasets.iter().find(|d| d.id == id)
    }
}
