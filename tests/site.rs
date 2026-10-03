//! The built site: what `data.xfina.dev` serves, from a data directory.

use std::fs;
use std::path::Path;

use chrono::{TimeZone, Utc};
use xfina_data::catalog::Catalog;
use xfina_data::pipeline::DataDir;
use xfina_data::publish;

#[test]
fn builds_an_index_and_a_preview_page_per_published_dataset() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = Catalog::load_validated(&root.join("datasets.yaml")).unwrap();

    let scratch = std::env::temp_dir().join(format!("xfina-data-site-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    let data = DataDir::new(scratch.join("data"));
    let out = scratch.join("dist");

    // The CPI fixture's expected output stands in for a published series.
    let csv = data.root().join("v1/inflation/in-cpi.csv");
    fs::create_dir_all(csv.parent().unwrap()).unwrap();
    fs::copy(root.join("tests/data/mospi/expected.csv"), &csv).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 10, 2, 10, 30, 0).unwrap();
    publish::write_metadata(&catalog, &data, now).unwrap();

    publish::build_site(&catalog, &data, &root.join("site"), &out).unwrap();

    let index = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(
        index.contains(r#"href="/datasets/in-cpi/""#),
        "index links the preview"
    );
    for placeholder in ["<!-- HEADER -->", "<!-- GROUPS -->", "<!-- ENDPOINTS -->"] {
        assert!(!index.contains(placeholder), "{placeholder} is filled");
    }
    assert!(
        index.contains(r#"id="theme-toggle""#),
        "the shared header is on the index"
    );
    // Grouped in the catalog's order. SBI is planned, so it is listed under
    // its group and marked, not linked.
    let rates = index.find(">USD/INR Rates<").expect("rates group");
    let inflation = index.find(">Inflation<").expect("inflation group");
    assert!(rates < inflation, "groups follow the catalog");
    assert!(
        index.contains("Not published yet"),
        "a planned dataset says so"
    );
    assert!(
        !index.contains(r#"href="/datasets/sbi-forex-card-usd/""#),
        "a planned dataset has no preview link"
    );
    assert!(
        index.contains("https://data.xfina.dev/v1/inflation/in-cpi.csv"),
        "published endpoints are listed"
    );
    assert!(
        !index.contains("https://data.xfina.dev/v1/fx/sbi-forex-card-usd.csv"),
        "a planned dataset has no endpoint"
    );

    let page = fs::read_to_string(out.join("datasets/in-cpi/index.html")).unwrap();
    assert!(!page.contains("{{"), "every placeholder is filled");
    assert!(
        !page.contains("<!-- HEADER -->"),
        "the shared header is on every page"
    );
    assert!(page.contains(r#""path":"v1/inflation/in-cpi.csv""#));
    assert!(page.contains(r#""views":["year-on-year"]"#));
    assert!(
        page.contains("integrity=\"sha512-"),
        "the chart library is pinned by hash"
    );
    // Chart and calendar share one panel of controls and one period slider,
    // and the CSV's URL sits beside its download in the title row.
    for id in [
        "id=\"views\"",
        "id=\"modes\"",
        "id=\"ranges\"",
        "id=\"stats\"",
        "id=\"navigator\"",
        "id=\"copy-url\"",
        "id=\"from\"",
        "id=\"to\"",
    ] {
        assert!(page.contains(id), "the page has {id}");
    }
    assert!(
        page.contains("https://data.xfina.dev/v1/inflation/in-cpi.csv"),
        "the CSV's URL is shown"
    );

    // SBI is still planned, so it gets no page.
    assert!(!out.join("datasets/sbi-forex-card-usd").exists());
    for asset in [
        "assets/site.css",
        "assets/preview.js",
        "assets/theme.js",
        "assets/logo.svg",
        "_headers",
        "404.html",
        "v1/metadata.json",
    ] {
        assert!(out.join(asset).exists(), "{asset} is deployed");
    }

    let metadata: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("v1/metadata.json")).unwrap()).unwrap();
    assert_eq!(metadata["datasets"][0]["rows"], 24);
    assert_eq!(
        metadata["datasets"][0]["updated_at"],
        "2026-10-02T10:30:00Z"
    );
}
