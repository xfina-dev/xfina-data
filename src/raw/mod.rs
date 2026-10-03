//! The raw archive: every source document, kept forever, with its provenance.
//!
//! Two halves. The bytes live in a bucket served at `raw.data.xfina.dev`; the
//! record of what each object is lives in a manifest, one CSV per dataset,
//! published next to the series it feeds. A missing or altered object is then
//! detectable, because the manifest carries its sha256.
//!
//! Nothing here ever overwrites. A key that is already taken by different
//! bytes gets a new key instead, and the object that was there stays exactly
//! as it was (rule 3).

pub mod store;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, SubsecRound, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::catalog::Origin;
use crate::error::{Result, XfinaDataError};
use store::{Put, Store};

/// Every object is uploaded with this header, and none is ever rewritten.
///
/// R2's default is four hours, which for an object that can never change
/// only costs reads. Set on the object rather than as a zone rule, so it
/// travels with the object wherever it is served from.
pub const CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// One archived object, as its manifest row records it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RawFile {
    /// Object key in the bucket, e.g. `sbi/forex-card-rates/2026/2026-10-02.pdf`.
    pub key: String,
    /// Lowercase hex sha256 of the bytes.
    pub sha256: String,
    /// Size in bytes.
    pub bytes: u64,
    /// The `Content-Type` the object was stored with.
    pub content_type: String,
    /// When these bytes were obtained from `source_url`, in UTC.
    pub fetched_at: DateTime<Utc>,
    /// How they reached the archive.
    pub origin: Origin,
    /// Where they were obtained from.
    ///
    /// For a file recovered from a mirror this is the mirror, pinned to a
    /// commit, not the original publisher: it is where these exact bytes came
    /// from, and the only claim the record can actually back.
    pub source_url: String,
}

/// A document on its way into the archive.
#[derive(Debug, Clone)]
pub struct Incoming {
    /// The key it should be stored under, if that key is free.
    pub key: String,
    /// The document itself.
    pub bytes: Vec<u8>,
    /// What it is. Required, because the S3 API defaults to
    /// `application/octet-stream` and every PDF would then download as a blob.
    pub content_type: &'static str,
    /// How it reached us.
    pub origin: Origin,
    /// Where it was obtained from.
    pub source_url: String,
    /// When it was obtained.
    pub fetched_at: DateTime<Utc>,
}

/// When an incoming document counts as already archived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dedupe {
    /// Identical bytes anywhere in the manifest. For scheduled fetches: SBI
    /// serves Friday's sheet all weekend, and archiving it three times says
    /// nothing the first copy did not.
    BySha,
    /// Only the same key with the same bytes. For importing an existing
    /// archive, whose duplicates are part of the history being preserved.
    ByKey,
}

/// The provenance record for one dataset's raw files.
#[derive(Debug, Clone, Default)]
pub struct Manifest {
    files: Vec<RawFile>,
}

impl Manifest {
    /// Read a manifest, or start an empty one if the file does not exist yet.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut reader = csv::Reader::from_path(path)
            .map_err(|e| XfinaDataError::Store(format!("{}: {e}", path.display())))?;
        let files = reader
            .deserialize()
            .collect::<std::result::Result<Vec<RawFile>, _>>()
            .map_err(|e| XfinaDataError::Store(format!("{}: {e}", path.display())))?;
        Ok(Self { files })
    }

    /// Write the manifest, oldest fetch first.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut writer = csv::Writer::from_path(path)
            .map_err(|e| XfinaDataError::Store(format!("{}: {e}", path.display())))?;
        for file in self.files() {
            writer
                .serialize(file)
                .map_err(|e| XfinaDataError::Store(format!("{}: {e}", path.display())))?;
        }
        writer
            .flush()
            .map_err(|e| XfinaDataError::Store(format!("{}: {e}", path.display())))?;
        Ok(())
    }

    /// Every file, in the order a series is derived from them: by fetch time,
    /// then by key, so a later fetch of a revised document wins over an
    /// earlier one and ties break the same way on every run.
    pub fn files(&self) -> Vec<&RawFile> {
        let mut files: Vec<&RawFile> = self.files.iter().collect();
        files.sort_by(|a, b| (a.fetched_at, &a.key).cmp(&(b.fetched_at, &b.key)));
        files
    }

    /// How many files the manifest records.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the manifest records no files at all.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn by_key(&self) -> HashMap<&str, &RawFile> {
        self.files.iter().map(|f| (f.key.as_str(), f)).collect()
    }

    /// Total bytes the archive holds for this manifest.
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }

    /// Files that share their bytes with at least one other, grouped by
    /// sha256, each group in derivation order (fetch time, then key).
    pub fn duplicate_groups(&self) -> Vec<Vec<RawFile>> {
        let mut groups: Vec<Vec<RawFile>> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        for file in self.files() {
            match index.get(file.sha256.as_str()) {
                Some(&i) => groups[i].push(file.clone()),
                None => {
                    index.insert(&file.sha256, groups.len());
                    groups.push(vec![file.clone()]);
                }
            }
        }
        groups.retain(|g| g.len() > 1);
        groups
    }

    /// Drop one file's record.
    pub fn remove(&mut self, key: &str) {
        self.files.retain(|f| f.key != key);
    }

    fn has_sha(&self, sha256: &str) -> bool {
        self.files.iter().any(|f| f.sha256 == sha256)
    }
}

/// Lowercase hex sha256 of some bytes.
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Archive one document, returning its record if it was not already held.
///
/// The record is added to `manifest` only once the object is in the store, so
/// a manifest never names a file the archive does not have. The reverse can
/// happen — a run dying between the upload and saving the manifest — and is
/// repaired here: an object already holding exactly these bytes is adopted
/// rather than treated as a conflict.
pub async fn archive(
    store: &Store,
    manifest: &mut Manifest,
    incoming: Incoming,
    dedupe: Dedupe,
) -> Result<Option<RawFile>> {
    let sha = sha256(&incoming.bytes);

    if dedupe == Dedupe::BySha && manifest.has_sha(&sha) {
        return Ok(None);
    }

    let key = match manifest.by_key().get(incoming.key.as_str()) {
        None => incoming.key.clone(),
        Some(held) if held.sha256 == sha => return Ok(None),
        // The key is taken by different bytes: a source revised a document
        // within one day, say. Both are kept, side by side.
        Some(_) => with_suffix(&incoming.key, &sha[..8]),
    };

    match store
        .put_new(&key, &incoming.bytes, incoming.content_type, &sha)
        .await?
    {
        Put::Created => {}
        Put::Exists { sha256 } if sha256.as_deref() == Some(sha.as_str()) => {}
        Put::Exists { sha256 } => {
            return Err(XfinaDataError::Store(format!(
                "{key} is already in the archive with {} content, and the manifest does \
                 not record it; refusing to guess which is right",
                match sha256 {
                    Some(other) => format!("different ({other})"),
                    None => "unverifiable".to_string(),
                }
            )))
        }
    }

    let file = RawFile {
        key,
        sha256: sha,
        bytes: incoming.bytes.len() as u64,
        content_type: incoming.content_type.to_string(),
        // Whole seconds: the manifest is read by people, and nothing here is
        // fetched twice within one.
        fetched_at: incoming.fetched_at.trunc_subsecs(0),
        origin: incoming.origin,
        source_url: incoming.source_url,
    };
    manifest.files.push(file.clone());
    Ok(Some(file))
}

/// Read a file back and check it is still what the manifest says it is.
pub async fn read_verified(store: &Store, file: &RawFile) -> Result<Vec<u8>> {
    let bytes = store.get(&file.key).await?;
    let actual = sha256(&bytes);
    if actual != file.sha256 {
        return Err(XfinaDataError::Store(format!(
            "{} has sha256 {actual}, the manifest records {}",
            file.key, file.sha256
        )));
    }
    Ok(bytes)
}

/// `a/b/2026-10-02.pdf` with suffix `1a2b3c4d` becomes `a/b/2026-10-02-1a2b3c4d.pdf`.
fn with_suffix(key: &str, suffix: &str) -> String {
    let (dir, name) = key.rsplit_once('/').unwrap_or(("", key));
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) => (stem, format!(".{ext}")),
        None => (name, String::new()),
    };
    if dir.is_empty() {
        format!("{stem}-{suffix}{ext}")
    } else {
        format!("{dir}/{stem}-{suffix}{ext}")
    }
}

/// The content type to store a file under, from its extension.
///
/// Only the formats sources actually produce. Anything else is refused rather
/// than stored as an octet stream nobody's browser will open.
pub fn content_type_for(path: &str) -> Result<&'static str> {
    match path
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
    {
        Some(ext) if ext == "pdf" => Ok("application/pdf"),
        Some(ext) if ext == "json" => Ok("application/json"),
        Some(ext) if ext == "csv" => Ok("text/csv"),
        _ => Err(XfinaDataError::Store(format!(
            "{path}: no known content type for this extension"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_goes_before_the_extension() {
        assert_eq!(
            with_suffix("sbi/forex-card-rates/2026/2026-10-02.pdf", "1a2b3c4d"),
            "sbi/forex-card-rates/2026/2026-10-02-1a2b3c4d.pdf"
        );
        assert_eq!(with_suffix("plain", "ab"), "plain-ab");
    }
}
