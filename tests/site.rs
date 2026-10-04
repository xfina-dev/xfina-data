//! The site's build input: `site-data.json`, from the catalog and a data
//! directory's `metadata.json`.
//!
//! `site/fixtures/` holds a recorded copy, made from the fixtures here and
//! re-recorded with `UPDATE_EXPECTED=1 cargo test`. It is also what the Vue
//! site in `site/` builds from in CI, so a change to this file's shape fails
//! here first, and the site is built against exactly what this tool writes.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use xfina_data::catalog::Catalog;
use xfina_data::pipeline::DataDir;
use xfina_data::publish::{self, SiteData};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Two published series, one monthly and one daily, standing in for the
/// live data: MoSPI CPI and the BIS rate, from their parser fixtures. SBI and
/// IMF stay unpublished, so the index's "not published yet" path is covered.
const PUBLISHED: [(&str, &str); 2] = [
    ("in-cpi", "tests/data/mospi/expected.csv"),
    ("bis-usd-inr", "tests/data/sdmx/bis.expected.csv"),
];

/// A parser fixture's rows under the published header. The SDMX fixtures
/// name their value column `value`; the published BIS file calls it
/// `inr_per_usd`, and the site reads it by that name.
fn published_csv(columns: &[String], fixture: &str) -> String {
    let text = fs::read_to_string(root().join(fixture)).unwrap();
    let (_, rows) = text.split_once('\n').unwrap();
    format!("{}\n{rows}", columns.join(","))
}

fn published_data(name: &str) -> (Catalog, DataDir) {
    let catalog = Catalog::load_validated(&root().join("datasets.yaml")).unwrap();
    let scratch: PathBuf =
        std::env::temp_dir().join(format!("xfina-data-site-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    let data = DataDir::new(scratch.join("data"));
    for (id, fixture) in PUBLISHED {
        let dataset = catalog.dataset(id).unwrap();
        let csv = data.series(dataset);
        fs::create_dir_all(csv.parent().unwrap()).unwrap();
        fs::write(&csv, published_csv(&dataset.output.columns, fixture)).unwrap();
    }
    let now = Utc.with_ymd_and_hms(2026, 10, 2, 10, 30, 0).unwrap();
    publish::write_metadata(&catalog, &data, now).unwrap();
    (catalog, data)
}

#[test]
fn site_data_matches_its_recorded_fixture() {
    let (catalog, data) = published_data("snapshot");
    let expected = root().join("site/fixtures/site-data.json");
    if std::env::var_os("UPDATE_EXPECTED").is_some() {
        publish::write_site_data(&catalog, &data, &expected).unwrap();
        for (id, _) in PUBLISHED {
            let dataset = catalog.dataset(id).unwrap();
            let copy = root().join("site/fixtures/data").join(&dataset.output.path);
            fs::create_dir_all(copy.parent().unwrap()).unwrap();
            fs::copy(data.series(dataset), copy).unwrap();
        }
    }
    let written = data.root().join("site-data.json");
    publish::write_site_data(&catalog, &data, &written).unwrap();
    assert_eq!(
        fs::read_to_string(&written).unwrap(),
        fs::read_to_string(&expected).unwrap(),
        "site-data.json moved; re-record with UPDATE_EXPECTED=1 once the change is understood, \
         and check the site still builds from it"
    );
}

#[test]
fn groups_follow_the_catalog_and_only_published_datasets_carry_facts() {
    let (catalog, data) = published_data("groups");
    let site: SiteData = publish::site_data(&catalog, &data).unwrap();

    let groups: Vec<&str> = site.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(groups, ["USD/INR Rates", "Inflation"]);

    for group in &site.groups {
        for dataset in &group.datasets {
            let published = PUBLISHED.iter().any(|(id, _)| *id == dataset.id);
            assert_eq!(dataset.published.is_some(), published, "{}", dataset.id);
            assert!(
                !dataset.name.is_empty() && dataset.name.len() <= 24,
                "{} has a short name",
                dataset.id
            );
        }
    }

    let cpi = site.groups[1]
        .datasets
        .iter()
        .find(|d| d.id == "in-cpi")
        .unwrap();
    let facts = cpi.published.as_ref().unwrap();
    assert_eq!(facts.path, "v1/inflation/in-cpi.csv");
    assert_eq!(facts.updated, "2026-10-02");
    assert!(facts.source.url.starts_with("https://"));
}

#[test]
fn without_metadata_nothing_is_published() {
    let catalog = Catalog::load_validated(&root().join("datasets.yaml")).unwrap();
    let empty = DataDir::new(std::env::temp_dir().join("xfina-data-site-empty-does-not-exist"));
    let site = publish::site_data(&catalog, &empty).unwrap();
    assert!(site
        .groups
        .iter()
        .flat_map(|g| &g.datasets)
        .all(|d| d.published.is_none()));
}

/// The site's fixture CSVs, which its dev server and explorer tests read,
/// must be exactly what this tool publishes from the fixtures above.
#[test]
fn the_sites_fixture_csvs_are_what_would_be_published() {
    // While re-recording, the snapshot test is writing these files in
    // parallel; the run after it checks them.
    if std::env::var_os("UPDATE_EXPECTED").is_some() {
        return;
    }
    let (catalog, data) = published_data("csvs");
    for (id, _) in PUBLISHED {
        let dataset = catalog.dataset(id).unwrap();
        let copy = root().join("site/fixtures/data").join(&dataset.output.path);
        assert_eq!(
            fs::read(&copy).unwrap_or_else(|e| panic!("{}: {e}", copy.display())),
            fs::read(data.series(dataset)).unwrap(),
            "{} is stale; re-record with UPDATE_EXPECTED=1",
            copy.display()
        );
    }
}
