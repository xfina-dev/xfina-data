//! Statistics agencies' SDMX APIs — the BIS and the IMF — read as CSV.
//!
//! An API client, so it lives here rather than in Xfina. Both agencies answer
//! a series key with one row per period, and only two columns matter:
//! `TIME_PERIOD` and `OBS_VALUE`. Everything else in the response is kept in
//! the archived file, untouched, for anyone who needs it.

use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;

use super::{Derived, Document, Fetched};
use crate::catalog::{Frequency, SdmxProvider};
use crate::error::{Result, XfinaDataError};
use crate::raw::RawFile;
use crate::series::{PeriodKey, Series, YearMonth};

/// What the catalog says about one series.
pub struct Query<'a> {
    /// Which agency.
    pub provider: SdmxProvider,
    /// Dataflow id.
    pub flow: &'a str,
    /// Dataflow version.
    pub version: &'a str,
    /// Series key.
    pub key: &'a str,
    /// First period to ask for on a first run.
    pub start: &'a str,
    /// Days back to ask for on later runs, if not the whole series.
    pub refetch_days: Option<u32>,
}

impl Query<'_> {
    /// The request URL, and the `Accept` header it needs.
    ///
    /// The IMF chooses CSV by header rather than by query parameter, so the
    /// header is part of what makes a recorded `source_url` reproducible and
    /// is named here, beside the URL, where nobody can forget it.
    fn request(&self, from: Option<&str>) -> (String, &'static str) {
        match self.provider {
            SdmxProvider::Bis => {
                let mut url = format!(
                    "https://stats.bis.org/api/v2/data/dataflow/BIS/{}/{}/{}?format=csv",
                    self.flow, self.version, self.key
                );
                if let Some(from) = from {
                    url.push_str(&format!("&startPeriod={from}"));
                }
                (url, "text/csv")
            }
            SdmxProvider::Imf => (
                format!(
                    "https://api.imf.org/external/sdmx/3.0/data/dataflow/IMF.STA/{}/{}/{}",
                    self.flow, self.version, self.key
                ),
                "application/vnd.sdmx.data+csv;version=2.0.0",
            ),
        }
    }
}

/// Fetch the series: all of it on a first run, and afterwards either all of
/// it again or only the recent window the catalog asks for.
pub async fn fetch(
    http: &reqwest::Client,
    query: &Query<'_>,
    today: NaiveDate,
    everything: bool,
) -> Result<Fetched> {
    let from = match (everything, query.refetch_days) {
        (true, _) => Some(query.start.to_string()),
        (false, Some(days)) => Some(
            (today - Duration::days(days.into()))
                .format("%Y-%m-%d")
                .to_string(),
        ),
        (false, None) => None,
    };
    // Only the BIS takes a start period; the IMF series is small enough to
    // ask for whole every time.
    let from = if query.provider == SdmxProvider::Bis {
        from
    } else {
        None
    };
    let (url, accept) = query.request(from.as_deref());

    let response = http
        .get(&url)
        .header("Accept", accept)
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
        .map_err(|e| XfinaDataError::Source(format!("{url}: {e}")))?
        .to_vec();

    // An error page or an empty answer is not worth archiving as data.
    let header = bytes.split(|b| *b == b'\n').next().unwrap_or_default();
    let header = String::from_utf8_lossy(header);
    if !header.contains("TIME_PERIOD") || !header.contains("OBS_VALUE") {
        return Err(XfinaDataError::Source(format!(
            "{url}: response is not SDMX CSV (first line `{}`)",
            header.chars().take(80).collect::<String>()
        )));
    }

    Ok(vec![Document {
        name: format!(
            "{}/{}/{}-{}.csv",
            today.format("%Y"),
            today.format("%Y-%m-%d"),
            query.flow.to_lowercase(),
            query.key
        ),
        bytes,
        content_type: "text/csv",
        source_url: url,
    }])
}

/// Derive the series from archived responses, later fetches winning, so a
/// revision fetched today replaces the value fetched before it.
///
/// A period with no value — an empty cell, `NaN`, or the BIS's `M` status for
/// "missing" — is an absent row. It is counted and reported, never turned
/// into a number.
pub fn derive(
    frequency: Frequency,
    columns: &[String],
    docs: &[(&RawFile, Vec<u8>)],
) -> Result<Derived> {
    let mut values: BTreeMap<PeriodKey, Decimal> = BTreeMap::new();
    let mut notes = Vec::new();

    for (file, bytes) in docs {
        let mut reader = csv::Reader::from_reader(bytes.as_slice());
        let header = reader
            .headers()
            .map_err(|e| XfinaDataError::Parse(format!("{}: {e}", file.key)))?
            .clone();
        let column = |name: &str| header.iter().position(|h| h == name);
        let (Some(time), Some(value)) = (column("TIME_PERIOD"), column("OBS_VALUE")) else {
            return Err(XfinaDataError::Parse(format!(
                "{}: no TIME_PERIOD and OBS_VALUE columns",
                file.key
            )));
        };
        let status = column("OBS_STATUS");

        let mut seen = BTreeMap::new();
        let mut missing = 0;
        for (line, record) in reader.records().enumerate() {
            let record = record.map_err(|e| XfinaDataError::Parse(format!("{}: {e}", file.key)))?;
            let at = |what: String| format!("{} line {}: {what}", file.key, line + 2);
            let period_text = record.get(time).unwrap_or_default();
            let key = period(frequency, period_text).map_err(|e| XfinaDataError::Parse(at(e)))?;

            // The request named one series key, so one period appearing twice
            // means the rows are not the series that was asked for.
            if seen.insert(key, ()).is_some() {
                return Err(XfinaDataError::Parse(at(format!(
                    "{period_text} appears twice in one response"
                ))));
            }

            let text = record.get(value).unwrap_or_default().trim();
            let flagged = status.and_then(|i| record.get(i)) == Some("M");
            if flagged || text.is_empty() || text.eq_ignore_ascii_case("nan") {
                missing += 1;
                continue;
            }
            let number = Decimal::from_str(text)
                .map_err(|e| XfinaDataError::Parse(at(format!("value `{text}`: {e}"))))?;
            values.insert(key, number);
        }
        if missing > 0 {
            notes.push(format!(
                "{}: {missing} period(s) carry no value; no rows for them",
                file.key
            ));
        }
    }

    let mut series = Series::new(columns.to_vec())?;
    for (key, value) in values {
        series.upsert(key, vec![value])?;
    }
    Ok(Derived { series, notes })
}

/// A period as the agencies write it: `2026-09-29` for a day, and `2026-09`
/// or `2026-M09` for a month.
fn period(frequency: Frequency, text: &str) -> std::result::Result<PeriodKey, String> {
    match frequency {
        Frequency::Daily => NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .map(PeriodKey::Day)
            .map_err(|e| format!("`{text}` is not a day: {e}")),
        Frequency::Monthly => {
            let (year, month) = text
                .split_once("-M")
                .or_else(|| text.split_once('-'))
                .ok_or_else(|| format!("`{text}` is not a month"))?;
            let year: i32 = year
                .parse()
                .map_err(|_| format!("`{text}` is not a month"))?;
            let month: u32 = month
                .parse()
                .map_err(|_| format!("`{text}` is not a month"))?;
            YearMonth::new(year, month)
                .map(PeriodKey::Month)
                .map_err(|e| e.to_string())
        }
    }
}
