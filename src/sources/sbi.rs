//! SBI's daily forex card rate sheet.
//!
//! The sheet is a PDF, and reading it is Xfina's job (rule 15): this module
//! fetches it, and turns the [`RateSheet`] Xfina returns into rows. It never
//! opens a PDF itself.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use xfina::models::ParseRequest;
use xfina::reference_rates::sbi_forex_card::parse_sbi_forex_card_rates;

use super::{Derived, Fetched};
use crate::error::{Result, XfinaDataError};
use crate::raw::RawFile;
use crate::series::{PeriodKey, Series};

/// The two columns this dataset publishes, as Xfina names them.
const TT_BUY: &str = "tt_buy";
const TT_SELL: &str = "tt_sell";

/// Fetch today's sheet, trying each URL in order.
///
/// `today` is the date in India and decides only the archive key. The date a
/// row is published under comes from inside the sheet (rule 9), so a sheet
/// fetched on Sunday that SBI last updated on Friday still lands on Friday.
pub async fn fetch(http: &reqwest::Client, urls: &[String], today: NaiveDate) -> Result<Fetched> {
    let mut failures = Vec::new();
    for url in urls {
        match fetch_one(http, url).await {
            Ok(bytes) => {
                return Ok(vec![super::Document {
                    name: format!("{}/{}.pdf", today.format("%Y"), today.format("%Y-%m-%d")),
                    bytes,
                    content_type: "application/pdf",
                    source_url: url.clone(),
                }])
            }
            Err(e) => failures.push(format!("{url}: {e}")),
        }
    }
    Err(XfinaDataError::Source(format!(
        "no SBI URL served a rate sheet:\n  {}",
        failures.join("\n  ")
    )))
}

async fn fetch_one(http: &reqwest::Client, url: &str) -> std::result::Result<Vec<u8>, String> {
    let response = http.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    // An error page served with a 200 is still not a rate sheet, and archiving
    // it would put HTML in an archive whose manifest calls it a PDF.
    if !bytes.starts_with(b"%PDF") {
        return Err("response is not a PDF".to_string());
    }
    Ok(bytes.to_vec())
}

/// Derive rows for `currency` from archived sheets.
///
/// Every sheet must parse. One that does not fails the whole derivation and
/// is named in the error, because a missing day is indistinguishable from a
/// bank holiday to anyone reading the CSV.
pub fn derive(currency: &str, columns: &[String], docs: &[(&RawFile, Vec<u8>)]) -> Result<Derived> {
    let mut failures = Vec::new();
    let mut notes = Vec::new();
    // Date -> (values, the file they came from). Several files can carry one
    // date: SBI serves its last sheet until it publishes the next.
    let mut rows: BTreeMap<NaiveDate, (Vec<Decimal>, &str)> = BTreeMap::new();

    for (file, bytes) in docs {
        let sheet = match parse_sbi_forex_card_rates(ParseRequest::new(bytes)) {
            Ok(parsed) => parsed.data,
            Err(e) => {
                failures.push(format!("{}: {e}", file.key));
                continue;
            }
        };

        // A sheet that does not even print the columns is a layout this
        // dataset does not understand, not a day without a quote.
        if !sheet.columns.iter().any(|c| c == TT_BUY) || !sheet.columns.iter().any(|c| c == TT_SELL)
        {
            failures.push(format!(
                "{}: sheet of {} has no {TT_BUY}/{TT_SELL} columns (has {:?})",
                file.key, sheet.date, sheet.columns
            ));
            continue;
        }

        let Some(rates) = sheet.currencies.iter().find(|c| c.currency == currency) else {
            failures.push(format!(
                "{}: sheet of {} does not quote {currency}",
                file.key, sheet.date
            ));
            continue;
        };

        let (Some(buy), Some(sell)) = (rates.rates.get(TT_BUY), rates.rates.get(TT_SELL)) else {
            // The sheet prints the columns but leaves this currency's cells
            // empty, as it does on some holidays. That is an absent row, and
            // the reason is recorded rather than swallowed.
            notes.push(format!(
                "{}: sheet of {} quotes no {currency} TT rate; no row",
                file.key, sheet.date
            ));
            continue;
        };

        if rates.unit != 1 {
            failures.push(format!(
                "{}: {currency} is quoted per {} units; this dataset publishes per unit",
                file.key, rates.unit
            ));
            continue;
        }

        if *buy <= Decimal::ZERO || sell <= buy {
            failures.push(format!(
                "{}: sheet of {} has {currency} tt_buy {buy}, tt_sell {sell}; \
                 a sell rate must be above a positive buy rate",
                file.key, sheet.date
            ));
            continue;
        }

        let values = vec![*buy, *sell];
        match rows.get(&sheet.date) {
            Some((held, from)) if *held != values => failures.push(format!(
                "{}: sheet of {} has {values:?}, but {from} has {held:?} for the same date",
                file.key, sheet.date
            )),
            Some(_) => {}
            None => {
                rows.insert(sheet.date, (values, file.key.as_str()));
            }
        }
    }

    if !failures.is_empty() {
        return Err(XfinaDataError::Parse(format!(
            "{} of {} SBI sheet(s) could not be used:\n  {}",
            failures.len(),
            docs.len(),
            failures.join("\n  ")
        )));
    }

    let mut series = Series::new(columns.to_vec())?;
    for (date, (values, _)) in rows {
        series.upsert(PeriodKey::Day(date), values)?;
    }
    Ok(Derived { series, notes })
}
