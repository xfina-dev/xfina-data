//! Time series: the shape everything published ends up in.
//!
//! A series is a sorted map from a period to a row of values, which gives
//! three properties for free that the predecessor project had to remember by
//! hand: rows come out in order, a period cannot appear twice, and re-deriving
//! the same document twice changes nothing.

pub mod csv;

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::error::{Result, XfinaDataError};

/// A calendar month, as a monthly series is keyed.
///
/// Distinct from a date on purpose: a CPI reading is a property of March, not
/// of the 1st of March, and writing it as a day invites a consumer to treat it
/// as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct YearMonth {
    year: i32,
    month: u32,
}

impl YearMonth {
    /// Build a month, rejecting anything outside 1..=12.
    pub fn new(year: i32, month: u32) -> Result<Self> {
        if !(1..=12).contains(&month) {
            return Err(XfinaDataError::Parse(format!(
                "month {month} is not 1..=12"
            )));
        }
        Ok(Self { year, month })
    }

    /// The calendar year.
    pub fn year(self) -> i32 {
        self.year
    }

    /// The month number, 1..=12.
    pub fn month(self) -> u32 {
        self.month
    }

    /// The month after this one.
    pub fn next(self) -> Self {
        if self.month == 12 {
            Self {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Self {
                year: self.year,
                month: self.month + 1,
            }
        }
    }
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

impl FromStr for YearMonth {
    type Err = XfinaDataError;

    fn from_str(s: &str) -> Result<Self> {
        let (year, month) = s
            .split_once('-')
            .ok_or_else(|| XfinaDataError::Parse(format!("`{s}` is not YYYY-MM")))?;
        if year.len() != 4 || month.len() != 2 {
            return Err(XfinaDataError::Parse(format!("`{s}` is not YYYY-MM")));
        }
        let year = year
            .parse()
            .map_err(|_| XfinaDataError::Parse(format!("`{s}` has a non-numeric year")))?;
        let month = month
            .parse()
            .map_err(|_| XfinaDataError::Parse(format!("`{s}` has a non-numeric month")))?;
        Self::new(year, month)
    }
}

/// What one row of a series is keyed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PeriodKey {
    /// A single day, for a daily series.
    Day(NaiveDate),
    /// A calendar month, for a monthly series.
    Month(YearMonth),
}

impl PeriodKey {
    /// The word used for this kind of key in a CSV header and in errors.
    pub fn column_name(&self) -> &'static str {
        match self {
            PeriodKey::Day(_) => "date",
            PeriodKey::Month(_) => "month",
        }
    }
}

impl fmt::Display for PeriodKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeriodKey::Day(d) => write!(f, "{}", d.format("%Y-%m-%d")),
            PeriodKey::Month(m) => write!(f, "{m}"),
        }
    }
}

impl FromStr for PeriodKey {
    type Err = XfinaDataError;

    /// Parse by shape: ten characters is a day, seven is a month.
    fn from_str(s: &str) -> Result<Self> {
        match s.len() {
            10 => NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map(PeriodKey::Day)
                .map_err(|e| XfinaDataError::Parse(format!("`{s}` is not YYYY-MM-DD: {e}"))),
            7 => s.parse().map(PeriodKey::Month),
            _ => Err(XfinaDataError::Parse(format!(
                "`{s}` is neither YYYY-MM-DD nor YYYY-MM"
            ))),
        }
    }
}

/// What happened when a row was written into a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The period was not present before.
    Added,
    /// The period was present with different values.
    ///
    /// Carries what was there, because a value moving is worth reporting: it
    /// is either a genuine source revision or a parser regression, and the run
    /// that overwrites it should say which rows it touched.
    Updated(Vec<Decimal>),
    /// The period was present with exactly these values.
    Unchanged,
}

/// A summary of merging one series into another.
#[derive(Debug, Default, Clone)]
pub struct MergeReport {
    /// Periods that were not present before.
    pub added: Vec<PeriodKey>,
    /// Periods whose values moved, with their previous values.
    pub updated: Vec<(PeriodKey, Vec<Decimal>)>,
    /// How many periods were already exactly right.
    pub unchanged: usize,
}

impl MergeReport {
    /// Whether anything at all changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty()
    }
}

impl fmt::Display for MergeReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} added, {} updated, {} unchanged",
            self.added.len(),
            self.updated.len(),
            self.unchanged
        )
    }
}

/// A published time series: a header and rows in period order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    columns: Vec<String>,
    rows: BTreeMap<PeriodKey, Vec<Decimal>>,
}

impl Series {
    /// Create an empty series with the given CSV header.
    ///
    /// The first column names the period key; the rest are values.
    pub fn new(columns: Vec<String>) -> Result<Self> {
        if columns.len() < 2 {
            return Err(XfinaDataError::Parse(
                "a series needs a key column and at least one value column".to_string(),
            ));
        }
        Ok(Self {
            columns,
            rows: BTreeMap::new(),
        })
    }

    /// The CSV header, key column first.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// How many value columns each row carries.
    pub fn arity(&self) -> usize {
        self.columns.len() - 1
    }

    /// Rows in period order.
    ///
    /// Double-ended, because "what is the latest value?" is the question
    /// asked most often of a published series — by the sync job deciding
    /// what to fetch, and by `metadata.json` describing the range.
    pub fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&PeriodKey, &Vec<Decimal>)> + ExactSizeIterator {
        self.rows.iter()
    }

    /// The earliest and latest periods, if the series has any rows.
    pub fn range(&self) -> Option<(PeriodKey, PeriodKey)> {
        Some((*self.rows.keys().next()?, *self.rows.keys().next_back()?))
    }

    /// How many rows the series holds.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the series holds no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The values recorded for one period, if any.
    pub fn get(&self, key: &PeriodKey) -> Option<&Vec<Decimal>> {
        self.rows.get(key)
    }

    /// Write one row, reporting what it did to the series.
    ///
    /// Rejects a row whose width does not match the header, and a key of a
    /// different kind from the rows already present — both are bugs in a
    /// source module rather than data problems, and both are cheaper to catch
    /// here than in a published file.
    pub fn upsert(&mut self, key: PeriodKey, values: Vec<Decimal>) -> Result<Change> {
        if values.len() != self.arity() {
            return Err(XfinaDataError::Parse(format!(
                "{key}: expected {} value(s) for {:?}, got {}",
                self.arity(),
                &self.columns[1..],
                values.len()
            )));
        }

        if let Some((existing, _)) = self.rows.iter().next() {
            if std::mem::discriminant(existing) != std::mem::discriminant(&key) {
                return Err(XfinaDataError::Parse(format!(
                    "{key} is a {} key, but this series is keyed by {}",
                    key.column_name(),
                    existing.column_name()
                )));
            }
        }

        match self.rows.insert(key, values.clone()) {
            None => Ok(Change::Added),
            Some(previous) if previous == values => Ok(Change::Unchanged),
            Some(previous) => Ok(Change::Updated(previous)),
        }
    }

    /// Merge another series in, with the incoming values winning.
    ///
    /// Incoming wins because the caller has just derived it from archived
    /// raw, which is the source of truth; the report names every row that
    /// moved so a silent rewrite is impossible.
    pub fn merge(&mut self, other: &Series) -> Result<MergeReport> {
        if other.columns != self.columns {
            return Err(XfinaDataError::Parse(format!(
                "cannot merge: columns {:?} do not match {:?}",
                other.columns, self.columns
            )));
        }

        let mut report = MergeReport::default();
        for (key, values) in other.iter() {
            match self.upsert(*key, values.clone())? {
                Change::Added => report.added.push(*key),
                Change::Updated(previous) => report.updated.push((*key, previous)),
                Change::Unchanged => report.unchanged += 1,
            }
        }
        Ok(report)
    }

    /// Check the properties that must hold whatever the numbers are.
    ///
    /// Ordering and uniqueness come from the map, so what is left is row
    /// width, key consistency, and — for a monthly series — that no month is
    /// missing. A daily series is not checked for gaps: weekends and holidays
    /// are gaps, and demanding otherwise would fail every correct run.
    pub fn check_invariants(&self) -> Result<()> {
        let mut problems = Vec::new();

        for (key, values) in &self.rows {
            if values.len() != self.arity() {
                problems.push(format!(
                    "{key}: has {} value(s), header declares {}",
                    values.len(),
                    self.arity()
                ));
            }
            // Every series here is a rate or an index level, and neither is
            // ever zero or negative. A zero is what a blank cell turns into
            // when something upstream coerces it, which is how the
            // predecessor came to publish 54 of them.
            for (column, value) in self.columns[1..].iter().zip(values) {
                if *value <= Decimal::ZERO {
                    problems.push(format!(
                        "{key}: `{column}` is {value}, not a positive number"
                    ));
                }
            }
        }

        let months: Vec<YearMonth> = self
            .rows
            .keys()
            .filter_map(|k| match k {
                PeriodKey::Month(m) => Some(*m),
                PeriodKey::Day(_) => None,
            })
            .collect();

        if !months.is_empty() && months.len() != self.rows.len() {
            problems.push("series mixes day keys and month keys".to_string());
        } else {
            for pair in months.windows(2) {
                let expected = pair[0].next();
                if pair[1] != expected {
                    problems.push(format!(
                        "monthly series jumps from {} to {}, missing {expected}",
                        pair[0], pair[1]
                    ));
                }
            }
        }

        if problems.is_empty() {
            return Ok(());
        }
        let mut message = format!("{} invariant violation(s):", problems.len());
        for problem in problems {
            message.push_str("\n  - ");
            message.push_str(&problem);
        }
        Err(XfinaDataError::Parse(message))
    }
}
