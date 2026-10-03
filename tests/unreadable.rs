//! Archived documents the catalog lists as unreadable: kept, never read, and
//! said so — the one way past "every document yields its rows or fails".

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use xfina_data::catalog::{Catalog, Origin};
use xfina_data::pipeline::{self, DataDir};
use xfina_data::raw::store::Store;
use xfina_data::raw::{self, Dedupe, Incoming, Manifest};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/mospi")
            .join(name),
    )
    .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xfina-data-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A one-dataset catalog over the MoSPI fixtures, listing `unreadable`.
fn catalog(unreadable: &str) -> Catalog {
    Catalog::from_yaml(&format!(
        r#"
version: 1
min_tool_version: 0.1.0
archive:
  bucket: xfina-data-raw
  public_url: https://raw.data.xfina.dev
  budget_bytes: 10000000000
datasets:
  - id: in-cpi
    title: India CPI
    frequency: monthly
    status: planned
    public: true
    licence: Public document
    source:
      kind: mospi-cpi
      base_year: "2024"
      series: [Back, Current]
      first_year: 2024
      state_code: 1
      sector_code: 3
      division_code: "00"
    raw:
      prefix: mospi/cpi
      origin: auto
{unreadable}
    output:
      path: v1/inflation/in-cpi.csv
      columns: [month, index]
"#
    ))
    .unwrap()
}

fn unreadable(sha: &str, reason: &str) -> String {
    format!("      unreadable:\n        - sha256: {sha}\n          reason: {reason}")
}

#[tokio::test]
async fn a_listed_document_is_skipped_and_its_periods_have_no_rows() {
    let dir = scratch("unreadable");
    let store = Store::Dir(dir.join("bucket"));
    let back = fixture("back-2024.json");
    let current = fixture("current-2025.json");

    let catalog = catalog(&unreadable(&raw::sha256(&current), "a test document"));
    catalog.validate().unwrap();
    let dataset = catalog.dataset("in-cpi").unwrap();
    let data = DataDir::new(dir.join("data"));

    let mut manifest = Manifest::default();
    for (name, bytes) in [("back-2024.json", back), ("current-2025.json", current)] {
        let incoming = Incoming {
            key: format!("mospi/cpi/{name}"),
            bytes,
            content_type: "application/json",
            origin: Origin::Auto,
            source_url: "https://example.org".to_string(),
            fetched_at: Utc.with_ymd_and_hms(2026, 10, 2, 10, 0, 0).unwrap(),
        };
        raw::archive(&store, &mut manifest, incoming, Dedupe::ByKey)
            .await
            .unwrap();
    }
    manifest.save(&data.manifest(dataset)).unwrap();

    pipeline::backfill(&catalog, &data, &store, "in-cpi", false)
        .await
        .unwrap();

    // Only 2024, from the back series: the listed 2025 document was never read.
    let csv = fs::read_to_string(data.series(dataset)).unwrap();
    assert!(csv.contains("2024-12,"), "{csv}");
    assert!(!csv.contains("2025-"), "{csv}");
    assert_eq!(csv.lines().count(), 13, "header and twelve months");
    // And it is still in the archive, untouched.
    assert!(dir.join("bucket/mospi/cpi/current-2025.json").exists());
}

#[test]
fn an_unreadable_entry_needs_a_sha256_and_a_reason() {
    let report = catalog(&unreadable("d1acc012fa7f", "\"\""))
        .validate()
        .unwrap_err()
        .to_string();
    assert!(report.contains("64 lowercase hex digits"), "{report}");
    assert!(report.contains("has no reason"), "{report}");
}

#[test]
fn an_unreadable_entry_is_listed_once() {
    let sha = "d1acc012fa7fed5796c81c415946f5e3caf79bd57b62fcba5c413b0034158098";
    let twice = format!(
        "{}\n        - sha256: {sha}\n          reason: again",
        unreadable(sha, "first")
    );
    let report = catalog(&twice).validate().unwrap_err().to_string();
    assert!(report.contains("listed twice"), "{report}");
}
