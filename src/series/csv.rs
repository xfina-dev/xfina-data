//! Reading and writing the published CSV.
//!
//! This is the format other people's code consumes, so the two directions are
//! kept exactly inverse: whatever is written must read back as the same
//! series, trailing zeros included.

use std::io::{Read, Write};
use std::str::FromStr;

use rust_decimal::Decimal;

use super::{PeriodKey, Series};
use crate::error::{Result, XfinaDataError};

/// Write a series as CSV: the header, then rows in period order.
///
/// Values are written exactly as they were parsed. `rust_decimal` keeps the
/// scale it was given, so a rate published as `72.50` is written back as
/// `72.50` and not `72.5` — the source said two decimals, and rounding or
/// trimming that is inventing precision either way.
pub fn write<W: Write>(series: &Series, writer: W) -> Result<()> {
    let mut out = csv::Writer::from_writer(writer);

    out.write_record(series.columns())
        .map_err(|e| XfinaDataError::Store(format!("writing header: {e}")))?;

    for (key, values) in series.iter() {
        let mut record = Vec::with_capacity(values.len() + 1);
        record.push(key.to_string());
        record.extend(values.iter().map(Decimal::to_string));
        out.write_record(&record)
            .map_err(|e| XfinaDataError::Store(format!("writing {key}: {e}")))?;
    }

    out.flush()
        .map_err(|e| XfinaDataError::Store(format!("flushing csv: {e}")))?;
    Ok(())
}

/// Read a series back from CSV.
///
/// Used to load what is already published before merging today's rows into
/// it, so a run only ever has to derive the periods it actually fetched.
pub fn read<R: Read>(reader: R) -> Result<Series> {
    let mut input = csv::Reader::from_reader(reader);

    let columns: Vec<String> = input
        .headers()
        .map_err(|e| XfinaDataError::Parse(format!("reading header: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();

    let mut series = Series::new(columns)?;

    for (index, record) in input.records().enumerate() {
        // +2: one for the header row, one because humans count from one.
        let line = index + 2;
        let record = record.map_err(|e| XfinaDataError::Parse(format!("line {line}: {e}")))?;

        let mut fields = record.iter();
        let key = fields
            .next()
            .ok_or_else(|| XfinaDataError::Parse(format!("line {line}: empty row")))?;
        let key = PeriodKey::from_str(key)
            .map_err(|e| XfinaDataError::Parse(format!("line {line}: {e}")))?;

        let mut values = Vec::with_capacity(series.arity());
        for (column, field) in fields.enumerate() {
            // A blank cell is why this project exists: the predecessor turned
            // one into 0.0 and published 54 of them. Refuse it by name.
            if field.trim().is_empty() {
                return Err(XfinaDataError::Parse(format!(
                    "line {line}: {key} has a blank `{}` — a missing value is not a number",
                    series.columns()[column + 1]
                )));
            }
            values.push(Decimal::from_str(field).map_err(|e| {
                XfinaDataError::Parse(format!(
                    "line {line}: {key} has `{field}` in `{}`: {e}",
                    series.columns()[column + 1]
                ))
            })?);
        }

        series.upsert(key, values)?;
    }

    Ok(series)
}
