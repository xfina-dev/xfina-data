# xfina-data — Agent Context & Guidelines

xfina-data publishes open Indian financial datasets at [data.xfina.dev](https://data.xfina.dev), with the raw source documents kept at `raw.data.xfina.dev`. Other people's tools read these files. That is the whole job, and it is what every rule below protects.

## TOP PRIORITY: never publish a number you cannot reproduce

Anything served from `data.xfina.dev` must be derivable, today and in five years, from a raw file this project archived. That outranks every other guideline here: if following another one would put an unreproducible number in a published file, do not follow it.

This is not hypothetical. The predecessor repository published a CPI series that was pasted in by hand, with no code able to regenerate it, and a rates series carrying 54 rows of `0.0` — blank cells in an upstream CSV that a script had quietly turned into a number. Both were served for months as real data.

**So:**

1. **Every published value comes from archived raw.** No hand-edited data files, ever. If a number cannot be produced by running the tool, it does not get published.
2. **Never emit a row you could not parse.** Zero is not a rate. Blank is not an index. A value outside a plausible band is a parser failure, not a data point. Fail the run, open the issue, fix the parser — a red nightly job is cheap, and a wrong number that someone files taxes against is not.
3. **Raw is immutable.** Never overwrite or delete an object in the archive. A correction is a new derivation from the same raw file, and the file that caused the mistake stays exactly where it was. (The single exception, removing exact duplicates once, is described under *Storage model*.)
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
14. **Errors, never silent skips.** No `|| true`, no swallowed failures, no "continue on error". A dataset that did not update must say so loudly. The one sanctioned skip is a document the catalog lists under `raw.unreadable`, by sha256 and with a reason. It stays in the archive, is never read, and is named in the run log and in `metadata.json`; the periods it would have dated simply have no row. List a document there only when no parser can read it, never to get past a parser bug.
15. **Document parsing lives in Xfina, not here.** This crate fetches, archives, merges and publishes. When a document format needs work, it is a change to Xfina.
16. **Xfina is a published dependency, never a path dependency.** Needing an unreleased parser change means cutting an Xfina release first — the same rule Xsteer follows, for the same reason: what we build against is what other people can build against.
17. **Every upload goes through `Store::put_new`.** It writes with `If-None-Match: *`, so the bucket itself refuses an overwrite; it takes the content type as a required argument, because the S3 API otherwise stores a PDF as `application/octet-stream`; and it sets `Cache-Control: public, max-age=31536000, immutable`, because R2's four-hour default only costs reads for an object that can never change. No other code path writes to the archive.

## Storage model

The archive lives in R2's free tier: **10 GB, and it only ever grows.** At four datasets it holds about 0.6 GB and adds about 50 MB a year, almost all of it SBI's PDFs. That is centuries of room for API datasets and decades for a handful more PDF ones — as long as every dataset keeps to these rules.

18. **One object per content.** A document whose bytes the archive already holds, under any key, is not stored again: SBI serves Friday's sheet all weekend, and the archive keeps it once. Sync and Import both deduplicate by sha256.
19. **Keys are organised by time:** `<source>/<series>/<YYYY>/<YYYY-MM-DD>[-<name>].<ext>`, dated by the first fetch, with a short sha256 suffix when one day yields two different documents under one name.
20. **Fetch only what is new.** History is fetched once; after that, only a window wide enough to catch revisions (`refetch_days` — 60 days for BIS, two years for MoSPI and IMF). No dataset re-archives its whole series on a schedule.
21. **Store what the source served, byte for byte.** No recompressing: the sha256 in the manifest is of the bytes served at `raw.data.xfina.dev`, PDFs are already compressed, and text is too small to matter. Readers get compression on the wire instead.
22. **The budget is watched.** `archive.budget_bytes` in the catalog is the limit. Every sync prints usage per dataset and in total, warns at 80% and fails at 95%; `metadata.json` publishes the sizes.

The one deletion the archive has ever needed is `raw dedupe`, run once (October 2026) to remove 193 exact copies an early import stored before imports deduplicated by content. It deletes only files whose bytes another file still holds, keeping the copy named after the date the document prints — that upstream archive had stored some sheets under day/month-swapped names too. It is not a tool for anything else.

## Conventions

- **Dates** are ISO `YYYY-MM-DD` in files and in code. Monthly series use `YYYY-MM`, and the column is named `month`, not `date`, because it is not a day.
- **Dataset ids** are lowercase and hyphenated, and match the catalog key: `sbi-forex-card-usd`, `in-cpi`.
- **CSV is the published format.** One file per series, full history, canonical. `metadata.json` describes every series: schema, source, licence, row count, last update, sha256. There are no per-year file splits — a consumer should not have to stitch a series together.
- **The catalog drives the tool.** Adding a dataset is a row in `datasets.yaml` plus a source module, not a new command.
- **The site's look comes from xfina-ui.** Colours, light and dark, the page column, the header, the footer and the dataset picker are [xfina-ui](https://github.com/xfina-dev/xfina-ui)'s, shared with xfina.dev and labs.xfina.dev. `site/vendor/xfina-ui/` is a tagged release copied in unchanged, and a test checks it against its `SHA256SUMS`. Upgrade by copying a newer release, never by editing the copy. `site.css` uses the tokens (`hsl(var(--name))`) and never defines a colour of its own: one this site needs goes into xfina-ui, for every site.
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
