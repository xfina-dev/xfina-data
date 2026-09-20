# Contributing to Xfinata

Read [`AGENTS.md`](AGENTS.md) first. It holds the rules this project is built on, and the top one — never publish a number you cannot reproduce — is not negotiable.

## Before you push

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three run in `Check PR`, so running them locally only saves you a round trip.

## Adding a dataset

A dataset is a row in `datasets.yaml` plus a source module. In order:

1. **Add the catalog entry.** Its `id`, the source it comes from, how often it updates, and the path it publishes to. The path and its column list are a promise: once merged, they only change under a new `/vN`.
2. **Write the source module** under `src/sources/`. It fetches bytes and turns them into records. It does not write files, does not decide what is due, and does not look at the clock.
3. **Hand documents to Xfina.** If the source is a PDF or a statement-shaped file, the parser belongs in [Xfina](https://github.com/sakthipriyan/xfina), released, and depended on by version. This crate never parses a document itself.
4. **Add snapshot tests** with fixtures in `tests/data/`, plus invariants that hold whatever the numbers are.
5. **Backfill from raw, not from someone else's derived data.** Import the source documents into the archive, then derive the series from them. A published series that was copied from a third party's CSV inherits their mistakes, which is exactly how the predecessor project ended up serving 54 rows of zeros.

## Changelog

Any change a consumer of `data.xfina.dev` could observe needs a `CHANGELOG.md` entry under `## [Unreleased]`, and CI fails the pull request without one. That covers `src/`, `datasets.yaml`, `site/`, `wrangler.jsonc` and the manifests; it does not cover tests or `xtask/`.

Entries are bold-prefixed by area and explain cause and consequence:

```markdown
- **SBI forex card rates:** dates between 2023-01 and 2024-12 that had an archived PDF but no published row are now derived and published, adding 181 rows.
```

Two branches open at once will conflict over that block. Resolving it takes seconds, and it is the price of the entry being written by whoever made the change.

## Releasing

Versions are SemVer, and this is 0.x: bump `minor` for anything breaking or feature-shaped, `patch` for fixes. The workspace version in the root `Cargo.toml` is the single source of truth, and the tag is derived from it rather than typed.

On a branch:

```bash
cargo xtask prepare-release minor    # bump, date the changelog section, commit
# open a PR, squash merge
cargo xtask tag-release              # tag the merged commit on main
```

Releasing from `main` directly is refused, and so is a tag that is not on `main`. The tag builds the release binaries that the nightly job downloads, so a release is what puts a parser fix into production data.

## Data corrections

When a published number turns out to be wrong:

1. Fix the parser, in whichever repository it lives.
2. Re-derive from the archived raw with `xfinata backfill`. Never edit a published file by hand.
3. Note it in the changelog, including what was wrong and which rows moved. People have already downloaded the old numbers.

The raw file that produced the wrong value stays in the archive untouched. It is the evidence.
