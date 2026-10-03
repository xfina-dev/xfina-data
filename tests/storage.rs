//! The storage model: one object per content, and a budget the archive is
//! measured against on every sync.

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

fn catalog(budget: u64) -> Catalog {
    Catalog::from_yaml(&format!(
        r#"
version: 1
min_tool_version: 0.1.0
archive:
  bucket: xfina-data-raw
  public_url: https://raw.data.xfina.dev
  budget_bytes: {budget}
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
    output:
      path: v1/inflation/in-cpi.csv
      columns: [month, index]
"#
    ))
    .unwrap()
}

/// Archive files the way an old import did: by key, duplicates included.
async fn seed(store: &Store, data: &DataDir, catalog: &Catalog, files: &[(&str, Vec<u8>)]) {
    let dataset = catalog.dataset("in-cpi").unwrap();
    let mut manifest = Manifest::default();
    for (name, bytes) in files {
        let incoming = Incoming {
            key: format!("mospi/cpi/{name}"),
            bytes: bytes.clone(),
            content_type: "application/json",
            origin: Origin::Upstream,
            source_url: "https://example.org".to_string(),
            fetched_at: Utc.with_ymd_and_hms(2026, 10, 2, 10, 0, 0).unwrap(),
        };
        raw::archive(store, &mut manifest, incoming, Dedupe::ByKey)
            .await
            .unwrap();
    }
    manifest.save(&data.manifest(dataset)).unwrap();
}

#[tokio::test]
async fn dedupe_reports_then_removes_only_exact_copies() {
    let dir = scratch("dedupe");
    let store = Store::Dir(dir.join("bucket"));
    let data = DataDir::new(dir.join("data"));
    let catalog = catalog(10_000_000_000);
    let dataset = catalog.dataset("in-cpi").unwrap();
    let back = fixture("back-2024.json");
    seed(
        &store,
        &data,
        &catalog,
        &[
            ("a/back.json", back.clone()),
            ("b/back-again.json", back.clone()),
            ("c/current.json", fixture("current-2025.json")),
        ],
    )
    .await;

    // A report deletes nothing.
    pipeline::dedupe(&catalog, &data, &store, "in-cpi", false)
        .await
        .unwrap();
    assert_eq!(Manifest::load(&data.manifest(dataset)).unwrap().len(), 3);
    assert!(dir.join("bucket/mospi/cpi/b/back-again.json").exists());

    // Applied: the later copy goes, the first stays, the distinct file stays.
    pipeline::dedupe(&catalog, &data, &store, "in-cpi", true)
        .await
        .unwrap();
    let manifest = Manifest::load(&data.manifest(dataset)).unwrap();
    assert_eq!(manifest.len(), 2);
    assert!(manifest.duplicate_groups().is_empty());
    assert!(dir.join("bucket/mospi/cpi/a/back.json").exists());
    assert!(!dir.join("bucket/mospi/cpi/b/back-again.json").exists());
    assert!(dir.join("bucket/mospi/cpi/c/current.json").exists());

    // The series derives exactly as before: nothing it needed was removed.
    pipeline::backfill(&catalog, &data, &store, "in-cpi", false)
        .await
        .unwrap();
    let csv = fs::read_to_string(data.series(dataset)).unwrap();
    assert_eq!(csv.lines().count(), 25, "header, 2024 and 2025");
}

#[tokio::test]
async fn an_import_never_stores_bytes_the_archive_holds() {
    let dir = scratch("import-dedupe");
    let store = Store::Dir(dir.join("bucket"));
    let data = DataDir::new(dir.join("data"));
    let catalog = catalog(10_000_000_000);
    let source = dir.join("upstream");
    fs::create_dir_all(source.join("2024")).unwrap();
    fs::write(
        source.join("2024/2024-09-06.json"),
        fixture("back-2024.json"),
    )
    .unwrap();
    fs::write(
        source.join("2024/2024-09-07.json"),
        fixture("back-2024.json"),
    )
    .unwrap();

    pipeline::import(
        &catalog,
        &data,
        &store,
        "in-cpi",
        &source,
        Origin::Upstream,
        "https://example.org",
        Utc.with_ymd_and_hms(2026, 10, 3, 0, 0, 0).unwrap(),
    )
    .await
    .unwrap();

    let manifest = Manifest::load(&data.manifest(catalog.dataset("in-cpi").unwrap())).unwrap();
    assert_eq!(manifest.len(), 1, "the weekend copy is not stored");
    assert!(!dir.join("bucket/mospi/cpi/2024/2024-09-07.json").exists());
}

#[tokio::test]
async fn the_budget_fails_a_run_near_its_limit() {
    let dir = scratch("budget");
    let store = Store::Dir(dir.join("bucket"));
    let data = DataDir::new(dir.join("data"));
    let bytes = fixture("back-2024.json");
    let size = bytes.len() as u64;

    let roomy = catalog(size * 10);
    seed(&store, &data, &roomy, &[("a.json", bytes)]).await;
    pipeline::check_budget(&roomy, &data).expect("10% of the budget is fine");

    let tight = catalog(size);
    let error = pipeline::check_budget(&tight, &data)
        .unwrap_err()
        .to_string();
    assert!(error.contains("of its"), "{error}");
}
