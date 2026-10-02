//! The raw archive's promises: nothing is overwritten, every object is
//! recorded, and a file that changed after archiving is caught on read.
//!
//! Run against the local-directory store, which follows the same rules as R2.

use std::fs;

use chrono::{TimeZone, Utc};
use xfina_data::catalog::Origin;
use xfina_data::raw::store::Store;
use xfina_data::raw::{self, Dedupe, Incoming, Manifest};

fn incoming(key: &str, bytes: &[u8]) -> Incoming {
    Incoming {
        key: key.to_string(),
        bytes: bytes.to_vec(),
        content_type: "application/pdf",
        origin: Origin::Auto,
        source_url: "https://example.org/sheet.pdf".to_string(),
        fetched_at: Utc.with_ymd_and_hms(2026, 10, 2, 10, 30, 0).unwrap(),
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("xfina-data-raw-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn archives_once_and_records_provenance() {
    let dir = scratch("once");
    let store = Store::Dir(dir.join("bucket"));
    let mut manifest = Manifest::default();

    let first = raw::archive(
        &store,
        &mut manifest,
        incoming("a/1.pdf", b"one"),
        Dedupe::ByKey,
    )
    .await
    .unwrap()
    .expect("first write archives");
    assert_eq!(first.sha256, raw::sha256(b"one"));
    assert_eq!(first.bytes, 3);
    assert_eq!(fs::read(dir.join("bucket/a/1.pdf")).unwrap(), b"one");

    let again = raw::archive(
        &store,
        &mut manifest,
        incoming("a/1.pdf", b"one"),
        Dedupe::ByKey,
    )
    .await
    .unwrap();
    assert!(again.is_none());
    assert_eq!(manifest.len(), 1);
}

#[tokio::test]
async fn never_overwrites_a_key_holding_different_bytes() {
    let dir = scratch("overwrite");
    let store = Store::Dir(dir.join("bucket"));
    let mut manifest = Manifest::default();

    raw::archive(
        &store,
        &mut manifest,
        incoming("a/1.pdf", b"one"),
        Dedupe::ByKey,
    )
    .await
    .unwrap();
    let revised = raw::archive(
        &store,
        &mut manifest,
        incoming("a/1.pdf", b"two"),
        Dedupe::ByKey,
    )
    .await
    .unwrap()
    .expect("revised bytes are kept too");

    assert_eq!(
        revised.key,
        format!("a/1-{}.pdf", &raw::sha256(b"two")[..8])
    );
    assert_eq!(fs::read(dir.join("bucket/a/1.pdf")).unwrap(), b"one");
    assert_eq!(manifest.len(), 2);
}

#[tokio::test]
async fn by_sha_skips_bytes_already_held_under_another_key() {
    let dir = scratch("sha");
    let store = Store::Dir(dir.join("bucket"));
    let mut manifest = Manifest::default();

    raw::archive(
        &store,
        &mut manifest,
        incoming("a/fri.pdf", b"same"),
        Dedupe::BySha,
    )
    .await
    .unwrap();
    let saturday = raw::archive(
        &store,
        &mut manifest,
        incoming("a/sat.pdf", b"same"),
        Dedupe::BySha,
    )
    .await
    .unwrap();
    assert!(saturday.is_none());
    assert!(!dir.join("bucket/a/sat.pdf").exists());
}

#[tokio::test]
async fn adopts_an_upload_whose_manifest_row_was_lost() {
    // A run that died between uploading and saving its manifest.
    let dir = scratch("adopt");
    fs::create_dir_all(dir.join("bucket/a")).unwrap();
    fs::write(dir.join("bucket/a/1.pdf"), b"one").unwrap();
    let store = Store::Dir(dir.join("bucket"));
    let mut manifest = Manifest::default();

    let adopted = raw::archive(
        &store,
        &mut manifest,
        incoming("a/1.pdf", b"one"),
        Dedupe::ByKey,
    )
    .await
    .unwrap();
    assert!(adopted.is_some());
    assert_eq!(manifest.len(), 1);
}

#[tokio::test]
async fn refuses_an_unrecorded_object_with_different_bytes() {
    let dir = scratch("conflict");
    fs::create_dir_all(dir.join("bucket/a")).unwrap();
    fs::write(dir.join("bucket/a/1.pdf"), b"something else").unwrap();
    let store = Store::Dir(dir.join("bucket"));
    let mut manifest = Manifest::default();

    let error = raw::archive(
        &store,
        &mut manifest,
        incoming("a/1.pdf", b"one"),
        Dedupe::ByKey,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("refusing to guess"), "{error}");
    assert_eq!(
        fs::read(dir.join("bucket/a/1.pdf")).unwrap(),
        b"something else"
    );
    assert!(manifest.is_empty());
}

#[tokio::test]
async fn manifest_round_trips_and_reads_are_verified() {
    let dir = scratch("verify");
    let store = Store::Dir(dir.join("bucket"));
    let mut manifest = Manifest::default();
    raw::archive(
        &store,
        &mut manifest,
        incoming("a/1.pdf", b"one"),
        Dedupe::ByKey,
    )
    .await
    .unwrap();

    let path = dir.join("manifest.csv");
    manifest.save(&path).unwrap();
    let loaded = Manifest::load(&path).unwrap();
    assert_eq!(loaded.files(), manifest.files());

    let file = loaded.files()[0].clone();
    assert_eq!(raw::read_verified(&store, &file).await.unwrap(), b"one");

    fs::write(dir.join("bucket/a/1.pdf"), b"tampered").unwrap();
    let error = raw::read_verified(&store, &file)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("the manifest records"), "{error}");
}
