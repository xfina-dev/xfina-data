//! Source modules: how each kind of source is fetched, and how its archived
//! documents become a series.
//!
//! The two halves are deliberately separate. Fetching touches the network and
//! the clock; deriving touches neither, and sees only bytes already in the
//! archive. That split is what makes every published row reproducible.

pub mod mospi;
pub mod sbi;
pub mod sdmx;

use chrono::NaiveDate;

use crate::catalog::{Dataset, Source};
use crate::error::Result;
use crate::raw::RawFile;
use crate::series::Series;

/// A document fetched from a source, before it is archived.
#[derive(Debug, Clone)]
pub struct Document {
    /// Key below the dataset's raw prefix, e.g. `2026/2026-10-02.pdf`.
    pub name: String,
    /// The document itself, exactly as served.
    pub bytes: Vec<u8>,
    /// What it is.
    pub content_type: &'static str,
    /// Where it was served from.
    pub source_url: String,
}

/// Everything one fetch produced.
pub type Fetched = Vec<Document>;

/// A series derived from archived documents.
#[derive(Debug)]
pub struct Derived {
    /// The rows.
    pub series: Series,
    /// Documents that yielded no row, and why. Printed, never swallowed.
    pub notes: Vec<String>,
}

/// Fetch whatever this dataset's source has that is due.
///
/// `today` is the date in India. `nothing_archived` lets a source that can
/// fetch history ask for all of it on its first run.
pub async fn fetch(
    dataset: &Dataset,
    http: &reqwest::Client,
    today: NaiveDate,
    nothing_archived: bool,
) -> Result<Fetched> {
    match &dataset.source {
        Source::SbiForexCard { urls, .. } => sbi::fetch(http, urls, today).await,
        Source::MospiCpi {
            base_year,
            series,
            first_year,
            state_code,
            sector_code,
            division_code,
        } => {
            let query = mospi::Query {
                base_year,
                series,
                first_year: *first_year,
                state_code: *state_code,
                sector_code: *sector_code,
                division_code,
            };
            mospi::fetch(http, &query, today, nothing_archived).await
        }
        Source::Sdmx {
            provider,
            flow,
            version,
            key,
            start,
            refetch_days,
        } => {
            let query = sdmx::Query {
                provider: *provider,
                flow,
                version,
                key,
                start,
                refetch_days: *refetch_days,
            };
            sdmx::fetch(http, &query, today, nothing_archived).await
        }
    }
}

/// Derive this dataset's series from archived documents, in manifest order.
pub fn derive(dataset: &Dataset, docs: &[(&RawFile, Vec<u8>)]) -> Result<Derived> {
    let columns = &dataset.output.columns;
    match &dataset.source {
        Source::SbiForexCard { currency, .. } => sbi::derive(currency, columns, docs),
        Source::MospiCpi {
            base_year, series, ..
        } => mospi::derive(base_year, series, columns, docs),
        Source::Sdmx { .. } => sdmx::derive(dataset.frequency, columns, docs),
    }
}
