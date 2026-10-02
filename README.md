# [xfina-data](https://github.com/xfina-dev/xfina-data)

[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)](LICENSE)
[![Data](https://img.shields.io/badge/data-data.xfina.dev-1f6f5f.svg)](https://data.xfina.dev)

**xfina-data** is the public data backbone behind the Xfina projects: it fetches open Indian financial data, keeps every source document, and publishes clean time series that anyone can read.

```
Source  ──▶  Raw archive  ──▶  Parse  ──▶  Series  ──▶  data.xfina.dev
 (SBI,        (immutable,      (Xfina)     (merge +      (CSV + metadata)
  MoSPI)       provenance)                  validate)
```

## Why it exists

1. **Benchmarks need history, not a snapshot.** A portfolio tool comparing against the Nifty, gold or the USD/INR rate needs the whole series, not today's number.
2. **The sources are not archives.** SBI publishes its forex card rates by overwriting a single PDF URL every day. Miss a day and it is gone. The value of this project is that it was there yesterday, and kept the file.
3. **Reproducibility is the product.** Every published number traces back to a source document that is still on disk, so a parser fix can re-derive history instead of asking a bank for it.

## Datasets

| | Dataset | Frequency | Published at | Status |
|---|---|---|---|---|
| 💱 | SBI forex card rates (USD TT buy/sell) | Daily | `v1/fx/sbi-forex-card-usd.csv` | Raw archived daily; published once [xfina#91](https://github.com/xfina-dev/xfina/issues/91) ships |
| 📉 | India CPI (all-India combined, general index) | Monthly | `v1/inflation/in-cpi.csv` | **Published** |
| 🌐 | USD/INR exchange rate, from the BIS | Daily, 1973→ | `v1/fx/bis-usd-inr.csv` | **Published** |
| 📜 | India CPI, monthly since 1957, from the IMF | Monthly | `v1/inflation/in-cpi-imf.csv` | **Published** |

Every published dataset has a preview page at `data.xfina.dev/datasets/<id>/`, with a chart or a calendar view (a GitHub-style year of days for daily series, a years × months grid for monthly ones), configured by the `preview` block of its catalog entry.

CSVs are served Brotli- or gzip-compressed to any client that sends `Accept-Encoding` (`curl --compressed`), about a quarter of their plain size. Everything is CSV, with one `v1/metadata.json` describing every series: its schema, source, licence, row count, range, sha256 and last update. Each dataset's raw manifest, `v1/manifests/<id>.csv`, lists every source document with its sha256, fetch time, source URL and origin; the document itself is at `https://raw.data.xfina.dev/<key>`.

## Architecture

```
xfina-data/
  datasets.yaml   the catalog: what exists, where it goes, what it promises
  src/
    catalog/      loading and validating the catalog
    sources/      per source: fetch (network, clock) and derive (bytes only)
    raw/          the immutable archive, its manifest, and the R2 store
    series/       rows, merge, invariants, CSV
    pipeline.rs   sync, backfill, raw import, raw put
    publish.rs    metadata.json and the site
  site/           landing page template and Cloudflare headers
  xtask/          release and maintenance tasks
```

Two branches: `main` holds the code, and `data` holds only what is published — series, `metadata.json` and manifests — so its history is the audit trail of every number that ever changed.

Document parsing is **not** here. Formats live in [Xfina](https://github.com/xfina-dev/xfina), which this crate depends on: it hands Xfina the bytes and gets structured records back. xfina-data's job is everything around that — fetching, archiving, merging into a series and publishing.

## How it runs

The tool is not a local utility. **It runs in GitHub Actions**, and what it does there is update published data. `Sync` runs at 16:00 and 22:00 IST, and on every merge to `main`:

```yaml
- uses: ./.github/actions/data-branch                 # the data branch, in ./data
- run: cargo build --release --locked                 # the tool, from this commit
- run: xfina-data sync --data data                       # fetch, archive to R2, rebuild
- run: git -C data commit && git -C data push         # "Produced by xfina-data@<sha>"
- run: xfina-data site build --data data --out dist      # page, headers, CSVs
- uses: cloudflare/wrangler-action@v4                 # deploy data.xfina.dev
```

Each data commit names the code commit that produced it. A dataset that fails does not hold back the others, but the run still ends red and opens (or comments on) a `sync-failure` issue. `Import Raw` is a manual workflow that brings an existing archive into R2 from a public repository at a pinned commit.

R2 credentials live only in GitHub Secrets (`R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`, `CLOUDFLARE_ACCOUNT_ID`, plus `CLOUDFLARE_API_TOKEN` for deploying). Reading the archive needs nothing, so anyone can re-derive a series:

```bash
git clone -b data https://github.com/xfina-dev/xfina-data data
xfina-data backfill --data data --dataset in-cpi --check
```

The full command surface:

```
xfina-data sync            [--dataset id] [--dry-run]
xfina-data backfill        --dataset id [--from-dir path] [--check]
xfina-data raw put         --dataset id --name <key> --source-url <url> [--origin manual] <file>
xfina-data raw import      --dataset id --from <dir> --source-url-base <url> --origin upstream
xfina-data reconcile       --dataset id
xfina-data config validate [--base <file>]
xfina-data site build      [--site site] [--out dist]
```

Every command takes `--config` (default `datasets.yaml`) and `--data` (default `data`); `--local-archive <dir>` swaps R2 for a local directory, for working without credentials.

## Development

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three are gates in `Check PR`, which carries the correctness burden precisely because nobody runs the tool by hand before it ships. `cargo xtask` holds the release and maintenance tasks.

## Status

Early. The pipeline runs end to end; datasets are being brought up one at a time.

## License

Apache-2.0. The data itself comes from public sources, each credited in `metadata.json` with its own terms.
