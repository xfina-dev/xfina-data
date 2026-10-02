//! Deriving series from archived documents.
//!
//! Snapshot tests over real, public documents — two MoSPI API responses and
//! two SBI rate sheets — with the recorded output in a sibling `.expected.csv`.
//! Re-record with `UPDATE_EXPECTED=1 cargo test`, and only after working out
//! why the numbers moved.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use xfina_data::catalog::{Frequency, Origin};
use xfina_data::raw::RawFile;
use xfina_data::series::{self, Series};
use xfina_data::sources::{mospi, sbi, sdmx};

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn file(key: &str, minute: u32) -> RawFile {
    RawFile {
        key: key.to_string(),
        sha256: String::new(),
        bytes: 0,
        content_type: String::new(),
        fetched_at: Utc.with_ymd_and_hms(2026, 10, 2, 10, minute, 0).unwrap(),
        origin: Origin::Auto,
        source_url: String::new(),
    }
}

fn csv_of(series: &Series) -> String {
    let mut out = Vec::new();
    series::csv::write(series, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn snapshot(name: &str, actual: &str) {
    let path = data(name);
    if std::env::var_os("UPDATE_EXPECTED").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} missing; run with UPDATE_EXPECTED=1", path.display()));
    assert_eq!(actual, expected, "{} no longer matches", path.display());
}

fn cpi_columns() -> Vec<String> {
    vec!["month".to_string(), "index".to_string()]
}

fn cpi_series() -> Vec<String> {
    vec!["Back".to_string(), "Current".to_string()]
}

fn mospi_doc(rows: &str) -> Vec<u8> {
    format!(
        r#"{{"data":[{rows}],"meta_data":{{"page":1,"totalRecords":1,"totalPages":1,"recordPerPage":100}},"msg":"ok","statusCode":true}}"#
    )
    .into_bytes()
}

fn mospi_row(series: &str, year: &str, month: &str, index: &str) -> String {
    format!(
        r#"{{"base_year":"2024","series":"{series}","year":"{year}","month":"{month}","state":"All India","sector":"Combined","division":"CPI (General)","index":{index}}}"#
    )
}

#[test]
fn mospi_back_and_current_make_one_monthly_series() {
    let back = file("mospi/cpi/back-2024.json", 0);
    let current = file("mospi/cpi/current-2025.json", 1);
    let docs = vec![
        (&back, fs::read(data("mospi/back-2024.json")).unwrap()),
        (&current, fs::read(data("mospi/current-2025.json")).unwrap()),
    ];

    let derived = mospi::derive("2024", &cpi_series(), &cpi_columns(), &docs).unwrap();
    derived.series.check_invariants().unwrap();
    assert_eq!(derived.series.len(), 24);
    snapshot("mospi/expected.csv", &csv_of(&derived.series));
}

#[test]
fn mospi_current_wins_over_back_whatever_the_fetch_order() {
    // Current fetched first, Back second: the catalog's series order decides,
    // not the order documents happened to arrive in.
    let current = file("a.json", 0);
    let back = file("b.json", 1);
    let docs = vec![
        (
            &current,
            mospi_doc(&mospi_row("Current", "2025", "January", "\"101.67\"")),
        ),
        (
            &back,
            mospi_doc(&mospi_row("Back", "2025", "January", "\"99.00\"")),
        ),
    ];
    let derived = mospi::derive("2024", &cpi_series(), &cpi_columns(), &docs).unwrap();
    assert_eq!(csv_of(&derived.series), "month,index\n2025-01,101.67\n");
}

#[test]
fn mospi_refuses_a_numeric_index() {
    // A JSON number would have to go through f64 to become a Decimal.
    let doc = file("x.json", 0);
    let docs = vec![(
        &doc,
        mospi_doc(&mospi_row("Back", "2013", "January", "55.1")),
    )];
    let error = mospi::derive("2024", &cpi_series(), &cpi_columns(), &docs)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a decimal string"), "{error}");
}

#[test]
fn mospi_skips_a_null_index_with_a_note() {
    let doc = file("x.json", 0);
    let docs = vec![(
        &doc,
        mospi_doc(&mospi_row("Current", "2026", "September", "null")),
    )];
    let derived = mospi::derive("2024", &cpi_series(), &cpi_columns(), &docs).unwrap();
    assert!(derived.series.is_empty());
    assert_eq!(derived.notes.len(), 1);
}

#[test]
fn mospi_rejects_rows_the_filters_should_have_excluded() {
    let doc = file("x.json", 0);
    let twice = format!(
        "{},{}",
        mospi_row("Back", "2013", "January", "\"55.10\""),
        mospi_row("Back", "2013", "January", "\"54.90\"")
    );
    let docs = vec![(&doc, mospi_doc(&twice))];
    let error = mospi::derive("2024", &cpi_series(), &cpi_columns(), &docs)
        .unwrap_err()
        .to_string();
    assert!(error.contains("appears twice"), "{error}");
}

#[test]
fn mospi_rejects_another_base_year() {
    let doc = file("x.json", 0);
    let docs = vec![(
        &doc,
        mospi_doc(&mospi_row("Back", "2013", "January", "\"55.10\"")),
    )];
    let error = mospi::derive("2012", &cpi_series(), &cpi_columns(), &docs)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not 2012"), "{error}");
}

fn tt_columns() -> Vec<String> {
    vec![
        "date".to_string(),
        "tt_buy".to_string(),
        "tt_sell".to_string(),
    ]
}

/// A directory of SBI rate sheets laid out as the archive is, e.g. a copy of
/// `raw.data.xfina.dev/sbi/forex-card-rates`, if `SBI_SHEETS` names one.
///
/// PDFs are never committed to this repository: they live in R2. The parser
/// itself is tested in Xfina against its own fixtures; this checks the
/// mapping from its result to rows, and runs wherever sheets are at hand.
fn sbi_sheets() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("SBI_SHEETS")?);
    dir.is_dir().then_some(dir)
}

#[test]
fn sbi_sheets_become_rows_dated_by_the_sheet() {
    let Some(sheets) = sbi_sheets() else {
        eprintln!("SBI_SHEETS not set; skipping the SBI rate sheet test");
        return;
    };
    let holiday = file("sbi/forex-card-rates/2020/2020-01-04.pdf", 0);
    let normal = file("sbi/forex-card-rates/2026/2026-10-01.pdf", 1);
    // The same sheet archived twice, as SBI's weekend copies are, must still
    // give one row.
    let copy = file("sbi/forex-card-rates/2026/2026-10-02.pdf", 2);
    let docs = vec![
        (
            &holiday,
            fs::read(sheets.join("2020/2020-01-04.pdf")).unwrap(),
        ),
        (
            &normal,
            fs::read(sheets.join("2026/2026-10-01.pdf")).unwrap(),
        ),
        (&copy, fs::read(sheets.join("2026/2026-10-01.pdf")).unwrap()),
    ];

    let derived = sbi::derive("USD", &tt_columns(), &docs).unwrap();
    derived.series.check_invariants().unwrap();
    // The holiday sheet prints the columns but no USD quote: no row, and a
    // note saying why rather than a zero.
    assert_eq!(derived.notes.len(), 1, "{:?}", derived.notes);
    assert!(derived.notes[0].contains("2020-01-04.pdf"));
    snapshot("sbi/expected.csv", &csv_of(&derived.series));
}

#[test]
fn sbi_names_a_document_that_is_not_a_rate_sheet() {
    let bogus = file("sbi/forex-card-rates/2026/2026-10-03.pdf", 0);
    let docs = vec![(&bogus, fs::read(data("mospi/back-2024.json")).unwrap())];
    let error = sbi::derive("USD", &tt_columns(), &docs)
        .unwrap_err()
        .to_string();
    assert!(error.contains("2026-10-03.pdf"), "{error}");
}

fn one_column(key: &str) -> Vec<String> {
    vec![key.to_string(), "value".to_string()]
}

#[test]
fn sdmx_bis_skips_missing_days_and_keeps_the_printed_precision() {
    let doc = file("bis/ws-xru/bis.csv", 0);
    let docs = vec![(&doc, fs::read(data("sdmx/bis.csv")).unwrap())];
    let derived = sdmx::derive(Frequency::Daily, &one_column("date"), &docs).unwrap();
    // 1973-02-12 is NaN with status M: a missing day, not a zero.
    assert_eq!(derived.series.len(), 3);
    assert!(derived.notes[0].contains("1 period(s) carry no value"));
    snapshot("sdmx/bis.expected.csv", &csv_of(&derived.series));
}

#[test]
fn sdmx_imf_reads_its_month_format_and_skips_empty_periods() {
    let doc = file("imf/cpi/imf.csv", 0);
    let docs = vec![(&doc, fs::read(data("sdmx/imf.csv")).unwrap())];
    let derived = sdmx::derive(Frequency::Monthly, &one_column("month"), &docs).unwrap();
    assert_eq!(derived.series.len(), 5);
    snapshot("sdmx/imf.expected.csv", &csv_of(&derived.series));
}

fn sdmx_doc(rows: &[&str]) -> Vec<u8> {
    let mut text = String::from("FREQ,TIME_PERIOD,OBS_VALUE,OBS_STATUS\n");
    for row in rows {
        text.push_str(row);
        text.push('\n');
    }
    text.into_bytes()
}

#[test]
fn sdmx_a_later_fetch_revises_an_earlier_one() {
    let first = file("a.csv", 0);
    let later = file("b.csv", 1);
    let docs = vec![
        (
            &first,
            sdmx_doc(&["D,2026-09-29,95.98,A", "D,2026-09-30,96.00,A"]),
        ),
        (&later, sdmx_doc(&["D,2026-09-30,96.10,A"])),
    ];
    let derived = sdmx::derive(Frequency::Daily, &one_column("date"), &docs).unwrap();
    assert_eq!(
        csv_of(&derived.series),
        "date,value\n2026-09-29,95.98\n2026-09-30,96.10\n"
    );
}

#[test]
fn sdmx_rejects_two_series_in_one_response() {
    let doc = file("x.csv", 0);
    let docs = vec![(
        &doc,
        sdmx_doc(&["D,2026-09-29,95.98,A", "D,2026-09-29,1.10,A"]),
    )];
    let error = sdmx::derive(Frequency::Daily, &one_column("date"), &docs)
        .unwrap_err()
        .to_string();
    assert!(error.contains("appears twice"), "{error}");
}

#[test]
fn sdmx_refuses_a_value_that_is_not_a_number() {
    let doc = file("x.csv", 0);
    let docs = vec![(&doc, sdmx_doc(&["D,2026-09-29,n/a,A"]))];
    let error = sdmx::derive(Frequency::Daily, &one_column("date"), &docs)
        .unwrap_err()
        .to_string();
    assert!(error.contains("value `n/a`"), "{error}");
}
