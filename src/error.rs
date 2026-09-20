//! The one error type every fallible path in this crate returns.
//!
//! Variants are named for the stage that failed, because that is the first
//! question asked when a nightly run goes red: did the source refuse us, did a
//! document not parse, or did the store reject a write? A message that cannot
//! answer that sends whoever is reading the failure issue to the wrong place.

use thiserror::Error;

/// Everything that can go wrong while syncing or publishing a dataset.
#[derive(Debug, Error)]
pub enum XfinataError {
    /// The catalog is malformed, or asks for something this build cannot do.
    #[error("config: {0}")]
    Config(String),

    /// A source could not be reached, or answered with something unusable.
    #[error("source: {0}")]
    Source(String),

    /// A document reached us intact but did not yield the values it should.
    ///
    /// This is the variant that keeps a bad number out of the published data.
    /// A rate that is zero, blank or outside a plausible band lands here
    /// rather than in a CSV.
    #[error("parse: {0}")]
    Parse(String),

    /// The raw store or the manifest rejected a read or a write.
    #[error("store: {0}")]
    Store(String),

    /// Local filesystem failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, XfinataError>;
