//! Xfinata publishes open Indian financial datasets at [data.xfina.dev].
//!
//! The shape of the thing is deliberately small:
//!
//! ```text
//! fetch  ──▶  archive raw  ──▶  parse  ──▶  merge into a series  ──▶  publish
//! ```
//!
//! Every published number is derived from a raw source file that is kept
//! forever, so any row can be traced back to the document it came from and
//! re-derived when a parser improves. Documents are parsed by [`xfina`], which
//! is where document formats live; this crate fetches them, keeps them, turns
//! them into time series and publishes the result.
//!
//! [data.xfina.dev]: https://data.xfina.dev
//! [`xfina`]: https://crates.io/crates/xfina

#![warn(missing_docs)]

pub mod error;
