//! Where raw bytes are kept, and how they are read back.
//!
//! Writing goes to R2 and needs credentials, which exist only in GitHub
//! Actions. Reading goes through the public URL and needs nothing, which is
//! what makes every published series re-derivable by anyone, not only by us.

use std::fs;
use std::path::PathBuf;

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::primitives::ByteStream;

use super::{sha256, CACHE_CONTROL};
use crate::error::{Result, XfinaDataError};

/// The object metadata key under which every upload records its sha256.
///
/// Lets a conflicting write be told apart from a repeated one without
/// downloading the object that is already there.
const SHA_METADATA: &str = "sha256";

/// What happened to a write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Put {
    /// The object did not exist and now does.
    Created,
    /// An object already held this key and was left untouched.
    Exists {
        /// Its sha256, when the store can say without reading it.
        sha256: Option<String>,
    },
}

/// A raw store. An enum rather than a trait object, because there are exactly
/// these four and each one is a different answer to "where do the bytes go?".
pub enum Store {
    /// The production archive. Writes need credentials; reads go through the
    /// public URL like everyone else's.
    R2 {
        /// S3 client for the bucket.
        client: aws_sdk_s3::Client,
        /// Bucket name.
        bucket: String,
        /// Public reader for the same bucket.
        public: Public,
    },
    /// A local directory laid out like the bucket. For tests, and for
    /// re-deriving from a copy of the archive held on disk.
    Dir(PathBuf),
    /// Read-only access through the public URL.
    Public(Public),
    /// Accepts writes and discards them, reading through the public URL. What
    /// `--dry-run` uses, so a dry run walks exactly the same path.
    DryRun(Public),
}

/// The bucket as anyone on the internet sees it.
pub struct Public {
    http: reqwest::Client,
    base_url: String,
}

impl Public {
    /// A reader for `base_url`, e.g. `https://raw.data.xfina.dev`.
    pub fn new(http: reqwest::Client, base_url: &str) -> Self {
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let url = format!("{}/{key}", self.base_url);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| XfinaDataError::Store(format!("reading {key}: {e}")))?;
        if !response.status().is_success() {
            return Err(XfinaDataError::Store(format!(
                "reading {key}: HTTP {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|e| XfinaDataError::Store(format!("reading {key}: {e}")))?;
        Ok(bytes.to_vec())
    }
}

impl Store {
    /// The production store, with credentials from the environment.
    ///
    /// `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` and `CLOUDFLARE_ACCOUNT_ID`
    /// come from GitHub Secrets and nowhere else. None of them is ever
    /// printed: an error says which variable is missing, never what it held.
    pub fn r2_from_env(bucket: &str, public: Public) -> Result<Self> {
        let var = |name: &str| {
            std::env::var(name)
                .ok()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| XfinaDataError::Config(format!("{name} is not set")))
        };
        let account = var("CLOUDFLARE_ACCOUNT_ID")?;
        let credentials = Credentials::new(
            var("R2_ACCESS_KEY_ID")?,
            var("R2_SECRET_ACCESS_KEY")?,
            None,
            None,
            "environment",
        );
        let config = aws_sdk_s3::Config::builder()
            .credentials_provider(credentials)
            // R2 ignores the region but the signer needs one; `auto` is what
            // Cloudflare documents.
            .region(Region::new("auto"))
            .endpoint_url(format!("https://{account}.r2.cloudflarestorage.com"))
            .force_path_style(true)
            .build();
        Ok(Store::R2 {
            client: aws_sdk_s3::Client::from_conf(config),
            bucket: bucket.to_string(),
            public,
        })
    }

    /// Store `bytes` under `key` only if nothing is there yet.
    ///
    /// The content type is a required argument and the cache header is not an
    /// argument at all: the upload path cannot omit either.
    pub async fn put_new(
        &self,
        key: &str,
        bytes: &[u8],
        content_type: &str,
        sha256_hex: &str,
    ) -> Result<Put> {
        match self {
            Store::R2 { client, bucket, .. } => {
                let result = client
                    .put_object()
                    .bucket(bucket)
                    .key(key)
                    .body(ByteStream::from(bytes.to_vec()))
                    .content_type(content_type)
                    .cache_control(CACHE_CONTROL)
                    .metadata(SHA_METADATA, sha256_hex)
                    // The archive's immutability is enforced by the store,
                    // not only by this code checking first: two runs racing
                    // for one key cannot both win.
                    .if_none_match("*")
                    .send()
                    .await;
                match result {
                    Ok(_) => Ok(Put::Created),
                    Err(e) if e.raw_response().map(|r| r.status().as_u16()) == Some(412) => {
                        let head = client
                            .head_object()
                            .bucket(bucket)
                            .key(key)
                            .send()
                            .await
                            .map_err(|e| store_error("inspecting", key, &e))?;
                        Ok(Put::Exists {
                            sha256: head.metadata().and_then(|m| m.get(SHA_METADATA)).cloned(),
                        })
                    }
                    Err(e) => Err(store_error("writing", key, &e)),
                }
            }
            Store::Dir(root) => {
                let path = root.join(key);
                if path.exists() {
                    return Ok(Put::Exists {
                        sha256: Some(sha256(&fs::read(&path)?)),
                    });
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, bytes)?;
                Ok(Put::Created)
            }
            Store::Public(_) => Err(XfinaDataError::Store(format!(
                "cannot write {key}: the public archive is read-only"
            ))),
            Store::DryRun(_) => Ok(Put::Created),
        }
    }

    /// Delete an object.
    ///
    /// The archive never deletes, with one exception: removing an exact
    /// duplicate whose bytes another key still holds, during the one-time
    /// cleanup `raw dedupe` performs. Nothing else calls this.
    pub async fn delete(&self, key: &str) -> Result<()> {
        match self {
            Store::R2 { client, bucket, .. } => {
                client
                    .delete_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await
                    .map_err(|e| store_error("deleting", key, &e))?;
                Ok(())
            }
            Store::Dir(root) => fs::remove_file(root.join(key))
                .map_err(|e| XfinaDataError::Store(format!("deleting {key}: {e}"))),
            Store::Public(_) => Err(XfinaDataError::Store(format!(
                "cannot delete {key}: the public archive is read-only"
            ))),
            Store::DryRun(_) => Ok(()),
        }
    }

    /// Read an object's bytes.
    pub async fn get(&self, key: &str) -> Result<Vec<u8>> {
        match self {
            Store::R2 { public, .. } | Store::Public(public) | Store::DryRun(public) => {
                public.get(key).await
            }
            Store::Dir(root) => fs::read(root.join(key))
                .map_err(|e| XfinaDataError::Store(format!("reading {key}: {e}"))),
        }
    }
}

/// An SDK error, reduced to the key and the service's own code and message.
///
/// Deliberately not the full debug form: requests carry an authorization
/// header, and a public repository's workflow log is readable by anyone.
fn store_error<E>(action: &str, key: &str, error: &E) -> XfinaDataError
where
    E: ProvideErrorMetadata + std::fmt::Display,
{
    XfinaDataError::Store(format!(
        "{action} {key}: {error} ({} {})",
        error.code().unwrap_or("no code"),
        error.message().unwrap_or("")
    ))
}
