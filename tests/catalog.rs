//! Catalog validation, and the contract it keeps with published data.
//!
//! The contract half is the one that matters: these tests are what stands
//! between a careless edit and a URL going away under someone else's code.

use std::path::Path;

use xfina_data::catalog::{validate, Catalog};

/// A catalog with one dataset, rendered from the parts a test wants to vary.
///
/// Written as YAML rather than built as structs on purpose, so these tests
/// also cover deserialization — a renamed field is a config break, and it
/// should fail here rather than in a nightly run.
fn one_dataset(id: &str, status: &str, frequency: &str, key: &str, path: &str) -> String {
    format!(
        r#"
version: 1
min_tool_version: 0.1.0
archive:
  bucket: xfina-data-raw
  public_url: https://raw.data.xfina.dev
datasets:
  - id: {id}
    title: A series
    frequency: {frequency}
    status: {status}
    public: true
    licence: Public document
    source:
      kind: mospi-cpi
      base_year: "2024"
      series: [Current]
      first_year: 2025
      state_code: 1
      sector_code: 3
      division_code: "00"
    raw:
      prefix: mospi/cpi
      origin: auto
    output:
      path: {path}
      columns: [{key}, index]
"#
    )
}

fn parse(yaml: &str) -> Catalog {
    Catalog::from_yaml(yaml).expect("catalog should parse")
}

/// The problems a failed validation reported, as one lowercase string.
fn problems(catalog: &Catalog) -> String {
    catalog
        .validate()
        .expect_err("catalog should be rejected")
        .to_string()
        .to_lowercase()
}

#[test]
fn shipped_catalog_is_valid() {
    // Guards the file that actually ships. Without this, a broken catalog is
    // only discovered by the job that needed it.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("datasets.yaml");
    let catalog = Catalog::load(&path).expect("datasets.yaml should parse");
    catalog.validate().expect("datasets.yaml should validate");
    assert!(!catalog.datasets.is_empty());
}

#[test]
fn accepts_a_well_formed_dataset() {
    let catalog = parse(&one_dataset(
        "in-cpi",
        "planned",
        "monthly",
        "month",
        "v1/inflation/in-cpi.csv",
    ));
    catalog.validate().expect("should be valid");
}

#[test]
fn rejects_duplicate_ids_and_paths() {
    let yaml = format!(
        "{}{}",
        one_dataset("in-cpi", "planned", "monthly", "month", "v1/a.csv"),
        one_dataset("in-cpi", "planned", "monthly", "month", "v1/a.csv")
            .lines()
            .skip(7) // drop the document's header, down to and including `datasets:`
            .collect::<Vec<_>>()
            .join("\n")
    );
    let report = problems(&parse(&yaml));
    assert!(report.contains("duplicate dataset id"), "{report}");
    assert!(report.contains("both publish to"), "{report}");
}

#[test]
fn rejects_a_monthly_series_keyed_by_date() {
    // A monthly reading keyed `date` invites a consumer to read it as a day.
    let report = problems(&parse(&one_dataset(
        "in-cpi", "planned", "monthly", "date", "v1/a.csv",
    )));
    assert!(
        report.contains("must name its first column `month`"),
        "{report}"
    );
}

#[test]
fn rejects_a_path_that_escapes_the_published_root() {
    let report = problems(&parse(&one_dataset(
        "in-cpi",
        "planned",
        "monthly",
        "month",
        "v1/../../etc/passwd.csv",
    )));
    assert!(report.contains("not a clean relative path"), "{report}");
}

#[test]
fn rejects_a_catalog_newer_than_the_tool() {
    let yaml = one_dataset("in-cpi", "planned", "monthly", "month", "v1/a.csv")
        .replace("min_tool_version: 0.1.0", "min_tool_version: 99.0.0");
    let report = problems(&parse(&yaml));
    assert!(report.contains("needs tool version 99.0.0"), "{report}");
}

#[test]
fn rejects_an_archive_url_that_would_mangle_every_raw_link() {
    let yaml = one_dataset("in-cpi", "planned", "monthly", "month", "v1/a.csv").replace(
        "public_url: https://raw.data.xfina.dev",
        "public_url: http://raw.data.xfina.dev/",
    );
    let report = problems(&parse(&yaml));
    assert!(
        report.contains("must be https:// with no trailing slash"),
        "{report}"
    );
}

/// The single-dataset catalog with a preview block appended.
fn with_preview(yaml: &str, preview: &str) -> String {
    format!("{yaml}    preview:\n{preview}")
}

#[test]
fn a_published_dataset_needs_a_preview() {
    let report = problems(&parse(&one_dataset(
        "in-cpi",
        "published",
        "monthly",
        "month",
        "v1/a.csv",
    )));
    assert!(report.contains("needs a `preview`"), "{report}");
}

#[test]
fn accepts_a_published_dataset_with_a_preview() {
    let yaml = with_preview(
        &one_dataset("in-cpi", "published", "monthly", "month", "v1/a.csv"),
        "      group: Inflation\n      summary: India's CPI.\n      unit: Index\n      lines:\n        - { column: index, label: CPI }\n      views: [year-on-year]\n",
    );
    parse(&yaml).validate().expect("should be valid");
}

#[test]
fn a_preview_can_only_draw_published_columns() {
    let yaml = with_preview(
        &one_dataset("in-cpi", "planned", "monthly", "month", "v1/a.csv"),
        "      group: Inflation\n      summary: India's CPI.\n      unit: Index\n      lines:\n        - { column: inflation, label: Inflation }\n",
    );
    let report = problems(&parse(&yaml));
    assert!(report.contains("not a value column"), "{report}");
}

#[test]
fn year_on_year_needs_a_monthly_series() {
    let yaml = with_preview(
        &one_dataset("usd", "planned", "daily", "date", "v1/a.csv"),
        "      group: Rates\n      summary: A rate.\n      unit: INR\n      lines:\n        - { column: index, label: Rate }\n      views: [year-on-year]\n",
    );
    let report = problems(&parse(&yaml));
    assert!(report.contains("only offered for monthly"), "{report}");
}

#[test]
fn refuses_to_publish_a_source_we_may_not_redistribute() {
    let yaml = one_dataset("in-cpi", "published", "monthly", "month", "v1/a.csv")
        .replace("public: true", "public: false");
    let report = problems(&parse(&yaml));
    assert!(report.contains("cannot be served"), "{report}");
}

#[test]
fn reports_every_problem_at_once() {
    // One problem per CI round trip is the failure mode this avoids.
    let yaml = one_dataset("In_CPI", "planned", "monthly", "date", "inflation.json")
        .replace("licence: Public document", r#"licence: """#);
    let report = problems(&parse(&yaml));
    for expected in [
        "id must be lowercase",
        "licence is empty",
        "`month`",
        "must start with",
    ] {
        assert!(
            report.contains(expected),
            "missing {expected:?} in: {report}"
        );
    }
}

#[test]
fn contract_allows_changing_a_planned_dataset() {
    // The point of `planned`: a series can be described, and reshaped, before
    // anyone can depend on it.
    let base = parse(&one_dataset(
        "in-cpi",
        "planned",
        "monthly",
        "month",
        "v1/old.csv",
    ));
    let current = parse(&one_dataset(
        "in-cpi",
        "planned",
        "monthly",
        "month",
        "v1/new.csv",
    ));
    validate::check_contract(&current, &base).expect("planned datasets may move");
}

#[test]
fn contract_rejects_moving_a_published_path() {
    let base = parse(&one_dataset(
        "in-cpi",
        "published",
        "monthly",
        "month",
        "v1/old.csv",
    ));
    let current = parse(&one_dataset(
        "in-cpi",
        "published",
        "monthly",
        "month",
        "v1/new.csv",
    ));
    let report = validate::check_contract(&current, &base)
        .expect_err("a published path is frozen")
        .to_string();
    assert!(report.contains("published path changed"), "{report}");
}

#[test]
fn contract_rejects_removing_a_published_dataset() {
    let base = parse(&one_dataset(
        "in-cpi",
        "published",
        "monthly",
        "month",
        "v1/a.csv",
    ));
    let current = parse(&one_dataset(
        "other",
        "published",
        "monthly",
        "month",
        "v1/b.csv",
    ));
    let report = validate::check_contract(&current, &base)
        .expect_err("a published dataset cannot vanish")
        .to_string();
    assert!(report.contains("has been removed"), "{report}");
}

#[test]
fn contract_rejects_changing_published_columns() {
    let base = parse(&one_dataset(
        "in-cpi",
        "published",
        "monthly",
        "month",
        "v1/a.csv",
    ));
    let yaml = one_dataset("in-cpi", "published", "monthly", "month", "v1/a.csv")
        .replace("columns: [month, index]", "columns: [month, cpi]");
    let report = validate::check_contract(&parse(&yaml), &base)
        .expect_err("a published header is frozen")
        .to_string();
    assert!(report.contains("published columns changed"), "{report}");
}
