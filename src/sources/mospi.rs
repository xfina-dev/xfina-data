//! MoSPI's consumer price index, from its public REST API.
//!
//! An API client rather than a document parser, so it lives here and not in
//! Xfina, which has no network and should keep it that way.

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

use super::{Derived, Document, Fetched};
use crate::error::{Result, XfinaDataError};
use crate::raw::RawFile;
use crate::series::{PeriodKey, Series, YearMonth};

const API: &str = "https://api.mospi.gov.in/api/cpi/getCPIData";

/// One request's worth of filters, as the catalog states them.
pub struct Query<'a> {
    /// CPI base year, e.g. `2024`.
    pub base_year: &'a str,
    /// Series to stitch, oldest first.
    pub series: &'a [String],
    /// First year any series covers.
    pub first_year: i32,
    /// All India is `1`.
    pub state_code: u32,
    /// Combined is `3`.
    pub sector_code: u32,
    /// CPI (General) is `00`.
    pub division_code: &'a str,
}

impl Query<'_> {
    fn url(&self, series: &str, year: i32) -> String {
        format!(
            "{API}?base_year={}&series={series}&year={year}&state_code={}&sector_code={}\
             &division_code={}&Format=JSON&limit=100",
            self.base_year, self.state_code, self.sector_code, self.division_code
        )
    }
}

/// Fetch what is due.
///
/// With nothing archived yet, every series for every year. After that, only
/// the live series for this year and last — enough to pick up a new month and
/// a revision to the one before it, without asking a government API for
/// thirteen years of history every day.
pub async fn fetch(
    http: &reqwest::Client,
    query: &Query<'_>,
    today: NaiveDate,
    everything: bool,
) -> Result<Fetched> {
    let live = query
        .series
        .last()
        .ok_or_else(|| XfinaDataError::Config("mospi-cpi lists no series".to_string()))?;

    let mut wanted = Vec::new();
    if everything {
        for series in query.series {
            for year in query.first_year..=today.year() {
                wanted.push((series.as_str(), year));
            }
        }
    } else {
        for year in [today.year() - 1, today.year()] {
            if year >= query.first_year {
                wanted.push((live.as_str(), year));
            }
        }
    }

    let mut documents = Vec::new();
    for (series, year) in wanted {
        let url = query.url(series, year);
        let bytes = get(http, &url).await?;
        let response = decode(&bytes).map_err(|e| XfinaDataError::Source(format!("{url}: {e}")))?;
        // A series that does not cover a year answers with nothing, which is
        // not a document worth keeping.
        if response.data.is_empty() {
            continue;
        }
        documents.push(Document {
            name: format!(
                "{}/{}/{}-{year}.json",
                today.format("%Y"),
                today.format("%Y-%m-%d"),
                series.to_lowercase()
            ),
            bytes,
            content_type: "application/json",
            source_url: url,
        });
    }
    Ok(documents)
}

async fn get(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|e| XfinaDataError::Source(format!("{url}: {e}")))?;
    if !response.status().is_success() {
        return Err(XfinaDataError::Source(format!(
            "{url}: HTTP {}",
            response.status()
        )));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| XfinaDataError::Source(format!("{url}: {e}")))?;
    Ok(bytes.to_vec())
}

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "statusCode")]
    status_code: bool,
    #[serde(default)]
    data: Vec<Row>,
    meta_data: Option<Meta>,
}

#[derive(Deserialize)]
struct Meta {
    #[serde(rename = "totalPages")]
    total_pages: u32,
}

#[derive(Deserialize)]
struct Row {
    base_year: Value,
    series: String,
    year: Value,
    month: String,
    index: Value,
}

fn decode(bytes: &[u8]) -> std::result::Result<Response, String> {
    let response: Response = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if !response.status_code {
        return Err("API answered statusCode: false".to_string());
    }
    // One page holds a year of one division a hundred times over. More than
    // one page means a filter was ignored, and the rows are not the ones asked
    // for.
    if let Some(meta) = &response.meta_data {
        if meta.total_pages > 1 {
            return Err(format!(
                "{} pages for one filtered year; a filter was not applied",
                meta.total_pages
            ));
        }
    }
    Ok(response)
}

/// Derive the index from archived responses.
///
/// Series are applied in catalog order, so `Current` overrides `Back` for any
/// month both cover; within a series, documents are applied in fetch order,
/// so a revision fetched later wins. Both orders are fixed, which is what
/// makes re-deriving from the archive give the same answer every time.
pub fn derive(
    base_year: &str,
    series_order: &[String],
    columns: &[String],
    docs: &[(&RawFile, Vec<u8>)],
) -> Result<Derived> {
    let mut by_series: Vec<BTreeMap<YearMonth, Decimal>> =
        vec![BTreeMap::new(); series_order.len()];
    let mut notes = Vec::new();

    for (file, bytes) in docs {
        let response =
            decode(bytes).map_err(|e| XfinaDataError::Parse(format!("{}: {e}", file.key)))?;
        let mut seen = BTreeMap::new();

        for row in response.data {
            let at = |what: &str| format!("{}: {} {} {what}", file.key, row.month, text(&row.year));

            if text(&row.base_year) != base_year {
                return Err(XfinaDataError::Parse(at(&format!(
                    "is base year {}, not {base_year}",
                    text(&row.base_year)
                ))));
            }
            let Some(rank) = series_order.iter().position(|s| *s == row.series) else {
                return Err(XfinaDataError::Parse(at(&format!(
                    "belongs to series `{}`, which the catalog does not list",
                    row.series
                ))));
            };
            let month = YearMonth::new(
                text(&row.year)
                    .parse()
                    .map_err(|_| XfinaDataError::Parse(at("has a non-numeric year")))?,
                month_number(&row.month)
                    .ok_or_else(|| XfinaDataError::Parse(at("is not a month")))?,
            )?;

            // The filters ask for one state, sector and division, so a month
            // appearing twice means the rows are not what was asked for.
            if seen.insert(month, ()).is_some() {
                return Err(XfinaDataError::Parse(at("appears twice in one response")));
            }

            let index = match &row.index {
                // Strings only. A bare JSON number would have to pass through
                // f64 to become a Decimal, and could come out a different
                // number from the one MoSPI printed.
                Value::String(s) if !s.trim().is_empty() => Decimal::from_str(s.trim())
                    .map_err(|e| XfinaDataError::Parse(at(&format!("has index `{s}`: {e}"))))?,
                Value::Null => {
                    notes.push(at("has no index yet; no row"));
                    continue;
                }
                other => {
                    return Err(XfinaDataError::Parse(at(&format!(
                        "has index {other}, which is not a decimal string"
                    ))))
                }
            };
            by_series[rank].insert(month, index);
        }
    }

    let mut merged = BTreeMap::new();
    for series in by_series {
        merged.extend(series);
    }

    let mut out = Series::new(columns.to_vec())?;
    for (month, index) in merged {
        out.upsert(PeriodKey::Month(month), vec![index])?;
    }
    Ok(Derived { series: out, notes })
}

/// A JSON scalar as the text it was written as.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let name = name.trim().to_ascii_lowercase();
    MONTHS.iter().position(|m| *m == name).map(|i| i as u32 + 1)
}
