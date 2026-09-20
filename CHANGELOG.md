# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Repository skeleton:** a Cargo workspace holding the `xfinata` binary and an `xtask` package, with the workspace version as the single source of truth, `cargo xtask` wired through `.cargo/config.toml`, and `AGENTS.md` carrying the project's rules.
- **CLI surface:** `sync`, `backfill`, `raw put`, `raw import`, `reconcile`, `config validate` and `site build`, parsed with clap. Every subcommand currently fails with "not implemented yet" rather than doing nothing quietly, so a half-wired pipeline cannot look like a successful run.
- **CI:** `Check PR` runs `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test`, skips the build when only documentation changed, and requires a changelog entry for any change a consumer of the published data could observe.
