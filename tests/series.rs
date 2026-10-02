//! Series behaviour: what a merge does, what cannot be represented, and that
//! the CSV round-trips exactly.
//!
//! The round-trip test is the one with teeth. The published CSV is the
//! product, and a value that changes shape on the way through here changes
//! shape in everyone's copy.

use std::str::FromStr;

use rust_decimal::Decimal;
use xfina_data::series::{csv, Change, PeriodKey, Series, YearMonth};

/// A two-value daily series, as the SBI rates one is shaped.
fn rates() -> Series {
    Series::new(vec!["date".into(), "tt_buy".into(), "tt_sell".into()]).unwrap()
}

/// A one-value monthly series, as the CPI one is shaped.
fn cpi() -> Series {
    Series::new(vec!["month".into(), "index".into()]).unwrap()
}

fn day(s: &str) -> PeriodKey {
    PeriodKey::from_str(s).expect("valid day")
}

fn month(s: &str) -> PeriodKey {
    PeriodKey::from_str(s).expect("valid month")
}

fn values(raw: &[&str]) -> Vec<Decimal> {
    raw.iter().map(|v| Decimal::from_str(v).unwrap()).collect()
}

#[test]
fn upsert_reports_what_it_did() {
    let mut series = rates();

    assert_eq!(
        series
            .upsert(day("2026-01-01"), values(&["89.50", "90.35"]))
            .unwrap(),
        Change::Added
    );
    assert_eq!(
        series
            .upsert(day("2026-01-01"), values(&["89.50", "90.35"]))
            .unwrap(),
        Change::Unchanged
    );

    // A value moving is either a source revision or a parser regression, and
    // either way the run that overwrites it has to be able to say so.
    assert_eq!(
        series
            .upsert(day("2026-01-01"), values(&["89.60", "90.45"]))
            .unwrap(),
        Change::Updated(values(&["89.50", "90.35"]))
    );
    assert_eq!(series.len(), 1);
}

#[test]
fn rows_come_out_in_period_order() {
    let mut series = cpi();
    for m in ["2026-03", "2026-01", "2026-02"] {
        series.upsert(month(m), values(&["100"])).unwrap();
    }
    let order: Vec<String> = series.iter().map(|(k, _)| k.to_string()).collect();
    assert_eq!(order, ["2026-01", "2026-02", "2026-03"]);
}

#[test]
fn rejects_a_row_of_the_wrong_width() {
    let mut series = rates();
    let error = series
        .upsert(day("2026-01-01"), values(&["89.50"]))
        .expect_err("a rates row carries two values");
    assert!(error.to_string().contains("expected 2 value(s)"), "{error}");
}

#[test]
fn rejects_mixing_day_and_month_keys() {
    let mut series = cpi();
    series.upsert(month("2026-01"), values(&["100"])).unwrap();
    let error = series
        .upsert(day("2026-02-01"), values(&["101"]))
        .expect_err("a series is keyed one way");
    assert!(error.to_string().contains("keyed by month"), "{error}");
}

#[test]
fn merge_reports_every_row_it_touched() {
    let mut published = rates();
    published
        .upsert(day("2026-01-01"), values(&["89.50", "90.35"]))
        .unwrap();
    published
        .upsert(day("2026-01-02"), values(&["89.70", "90.55"]))
        .unwrap();

    let mut fresh = rates();
    fresh
        .upsert(day("2026-01-02"), values(&["89.70", "90.55"]))
        .unwrap(); // same
    fresh
        .upsert(day("2026-01-03"), values(&["89.90", "90.75"]))
        .unwrap(); // new
    fresh
        .upsert(day("2026-01-01"), values(&["89.55", "90.40"]))
        .unwrap(); // revised

    let report = published.merge(&fresh).unwrap();

    assert_eq!(report.added, vec![day("2026-01-03")]);
    assert_eq!(report.unchanged, 1);
    assert_eq!(
        report.updated,
        vec![(day("2026-01-01"), values(&["89.50", "90.35"]))]
    );
    assert_eq!(published.len(), 3);
}

#[test]
fn merge_refuses_a_different_header() {
    let mut series = rates();
    let other = Series::new(vec!["date".into(), "tt_buy".into()]).unwrap();
    let error = series.merge(&other).expect_err("headers must match");
    assert!(error.to_string().contains("do not match"), "{error}");
}

#[test]
fn monthly_gaps_are_a_defect() {
    let mut series = cpi();
    series.upsert(month("2026-01"), values(&["100"])).unwrap();
    series.upsert(month("2026-03"), values(&["102"])).unwrap();

    let error = series
        .check_invariants()
        .expect_err("a missing month is a hole in the series");
    assert!(error.to_string().contains("missing 2026-02"), "{error}");
}

#[test]
fn zero_is_never_a_value() {
    // The predecessor's 54 rows of 0.0 were blank cells coerced to a number.
    let mut series = rates();
    series
        .upsert(day("2021-01-26"), values(&["0.0", "0.0"]))
        .unwrap();
    let error = series
        .check_invariants()
        .expect_err("a zero rate is a parse failure, not a data point");
    assert!(
        error.to_string().contains("not a positive number"),
        "{error}"
    );
}

#[test]
fn monthly_series_crosses_a_year_boundary() {
    let mut series = cpi();
    for m in ["2025-11", "2025-12", "2026-01"] {
        series.upsert(month(m), values(&["100"])).unwrap();
    }
    series
        .check_invariants()
        .expect("December to January is not a gap");
}

#[test]
fn daily_gaps_are_normal() {
    // Weekends and holidays. Demanding a value every day would fail every
    // correct run the SBI series ever does.
    let mut series = rates();
    series
        .upsert(day("2026-01-02"), values(&["89.50", "90.35"]))
        .unwrap();
    series
        .upsert(day("2026-01-05"), values(&["89.70", "90.55"]))
        .unwrap();
    series
        .check_invariants()
        .expect("a weekend is not a defect");
}

#[test]
fn csv_round_trips_and_keeps_the_source_precision() {
    let mut series = rates();
    // 90.50 must not come back as 90.5: the source published two decimals,
    // and trimming one is as much an edit as adding one.
    series
        .upsert(day("2026-01-01"), values(&["89.50", "90.50"]))
        .unwrap();
    series
        .upsert(day("2026-01-02"), values(&["89.7", "90.554"]))
        .unwrap();

    let mut written = Vec::new();
    csv::write(&series, &mut written).unwrap();
    let text = String::from_utf8(written).unwrap();

    assert_eq!(
        text,
        "date,tt_buy,tt_sell\n2026-01-01,89.50,90.50\n2026-01-02,89.7,90.554\n"
    );
    assert_eq!(csv::read(text.as_bytes()).unwrap(), series);
}

#[test]
fn csv_refuses_a_blank_cell() {
    // The bug this project exists because of: the predecessor turned blank
    // upstream cells into 0.0 and published 54 of them as real rates.
    let text = "date,tt_buy,tt_sell\n2026-01-01,89.50,\n";
    let error = csv::read(text.as_bytes()).expect_err("a blank is not a number");
    assert!(error.to_string().contains("blank `tt_sell`"), "{error}");
}

#[test]
fn csv_refuses_an_unparseable_period() {
    let text = "month,index\n2026-1,100\n";
    let error = csv::read(text.as_bytes()).expect_err("YYYY-M is not a month");
    assert!(error.to_string().contains("neither"), "{error}");
}

#[test]
fn range_reports_the_first_and_last_period() {
    // What the sync job asks before fetching, and what metadata.json prints.
    let mut series = rates();
    assert_eq!(series.range(), None);

    for d in ["2026-01-05", "2026-01-02", "2026-01-09"] {
        series.upsert(day(d), values(&["89.50", "90.35"])).unwrap();
    }
    assert_eq!(series.range(), Some((day("2026-01-02"), day("2026-01-09"))));
}

#[test]
fn year_month_knows_its_successor() {
    assert_eq!(
        YearMonth::new(2026, 12).unwrap().next(),
        YearMonth::new(2027, 1).unwrap()
    );
    assert!(YearMonth::new(2026, 13).is_err());
}
