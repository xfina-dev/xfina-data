# xfina-data — Agent Context & Guidelines

xfina-data publishes open Indian financial datasets at [data.xfina.dev](https://data.xfina.dev), with the raw source documents kept at `raw.data.xfina.dev`. Other people's tools read these files. That is the whole job, and it is what every rule below protects.

## TOP PRIORITY: never publish a number you cannot reproduce

Anything served from `data.xfina.dev` must be derivable, today and in five years, from a raw file this project archived. That outranks every other guideline here: if following another one would put an unreproducible number in a published file, do not follow it.

This is not hypothetical. The predecessor repository published a CPI series that was pasted in by hand, with no code able to regenerate it, and a rates series carrying 54 rows of `0.0` — blank cells in an upstream CSV that a script had quietly turned into a number. Both were served for months as real data.

**So:**

1. **Every published value comes from archived raw.** No hand-edited data files, ever. If a number cannot be produced by running the tool, it does not get published.
2. **Never emit a row you could not parse.** Zero is not a rate. Blank is not an index. A value outside a plausible band is a parser failure, not a data point. Fail the run, open the issue, fix the parser — a red nightly job is cheap, and a wrong number that someone files taxes against is not.
3. **Raw is immutable.** Never overwrite or delete an object in the archive. A correction is a new derivation from the same raw file, and the file that caused the mistake stays exactly where it was.
4. **Provenance is recorded per file**, never per dataset: sha256, fetch time, source URL, and origin `auto | manual | upstream`. One series routinely mixes all three, and "where did this come from" must be answerable for any single day.

## Never commit a credential

This repository is **public**, and the pipeline it drives holds write access to an R2 bucket and a Cloudflare account. A leaked key here does not expose private data — everything published is public by design — it lets someone else rewrite what the public reads.

5. **Credentials never enter the repository.** R2 keys and the Cloudflare API token live in GitHub Secrets and reach the tool through the environment. Not in a `.env` file, not in a workflow, not in a test fixture, not in a comment "temporarily". Because every write path runs in Actions, no credential ever needs to exist on a developer machine at all.
6. **Never log a credential or a signed URL.** A presigned URL carries its signature in the query string, and a workflow log on a public repository is readable by anyone. Log the object key, never the URL.
7. **Only publishable sources go into the public archive.** A source whose licence forbids redistribution is marked `public: false` in the catalog and never reaches a bucket or prefix that serves traffic. Publishing something we had no right to republish is the one mistake here that cannot be fixed by re-deriving it.
8. **Never commit a personal financial document.** Fixtures are public reference documents — a published rate card, an API response. If you reproduced a bug using your own statement, that file does not go in `tests/data/`, and its contents do not go in a commit message or an issue. This is Xfina's rule, and the reason its corpus lives outside that repository.

**Before every commit, PR or issue**, re-read what you are about to publish and check it against the four above. Amending a commit is cheap; a merged commit message is permanent, and a pushed key is compromised whether or not it is later removed.

## Hard rules

9. **No ambient clock in a parser.** The date comes from inside the document, never from `Utc::now()`. A run at 00:05 IST must produce the same records as a rerun at noon.
10. **Indian sources are IST.** Parse with `Asia/Kolkata` before any UTC conversion, matching Xfina and Xsteer.
11. **Rates and index values are `Decimal`, never `f64`.** Money and index levels do not round-trip through binary floating point, and these files are what other people's returns are computed from.
12. **Keep the source's precision.** Formatting a three-decimal source to two decimals is not tidying, it is inventing a number.
13. **The CSV header is public API.** Column names, column order and the path a series lives at change only under a new `/vN` prefix. Published paths are append-only within a version, and CI blocks a PR that removes or renames one.
14. **Errors, never silent skips.** No `|| true`, no swallowed failures, no "continue on error". A dataset that did not update must say so loudly.
15. **Document parsing lives in Xfina, not here.** This crate fetches, archives, merges and publishes. When a document format needs work, it is a change to Xfina.
16. **Xfina is a published dependency, never a path dependency.** Needing an unreleased parser change means cutting an Xfina release first — the same rule Xsteer follows, for the same reason: what we build against is what other people can build against.
17. **Every upload goes through `Store::put_new`.** It writes with `If-None-Match: *`, so the bucket itself refuses an overwrite; it takes the content type as a required argument, because the S3 API otherwise stores a PDF as `application/octet-stream`; and it sets `Cache-Control: public, max-age=31536000, immutable`, because R2's four-hour default only costs reads for an object that can never change. No other code path writes to the archive.

## Conventions

- **Dates** are ISO `YYYY-MM-DD` in files and in code. Monthly series use `YYYY-MM`, and the column is named `month`, not `date`, because it is not a day.
- **Dataset ids** are lowercase and hyphenated, and match the catalog key: `sbi-forex-card-usd`, `in-cpi`.
- **CSV is the published format.** One file per series, full history, canonical. `metadata.json` describes every series: schema, source, licence, row count, last update, sha256. There are no per-year file splits — a consumer should not have to stitch a series together.
- **The catalog drives the tool.** Adding a dataset is a row in `datasets.yaml` plus a source module, not a new command.
- **Comments explain why.** Name the failure mode being avoided, and the incident if there was one. What the code does is already on the line below.

## Testing

- **Snapshot tests are the house pattern:** inputs in a file, recorded output in a sibling file, re-recorded with `UPDATE_EXPECTED=1 cargo test`. Never re-record to make a failing test pass without first understanding why the numbers moved.
- **Invariant tests sit alongside them:** properties that hold whatever the numbers are — dates strictly ascending, no duplicate keys, `tt_sell > tt_buy`, no gaps in a monthly sequence.
- **No PDFs in the repository.** Source documents live in R2 and nowhere else; `.gitignore` and Check PR both refuse one. Tests that need rate sheets read them from the directory `SBI_SHEETS` names and skip cleanly when it is unset. The parser itself is tested in Xfina, against Xfina's fixtures.
- **The full archive is not in CI.** `cargo xtask parity` checks a parser against hundreds of megabytes of raw documents and is run locally, pointing at a directory that may be absent and skipping cleanly when it is.

## Build

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three are CI gates. `cargo xtask` runs the release and maintenance tasks.
