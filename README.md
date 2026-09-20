# [xfinata](https://github.com/xfina-dev/xfinata)

[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)](LICENSE)
[![Data](https://img.shields.io/badge/data-data.xfina.dev-1f6f5f.svg)](https://data.xfina.dev)

**Xfinata** is the public data backbone behind the Xfina projects: it fetches open Indian financial data, keeps every source document, and publishes clean time series that anyone can read.

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
| 💱 | SBI forex card rates (USD TT buy/sell) | Daily | `v1/fx/sbi-forex-card-usd.csv` | **In progress** |
| 📉 | India CPI (all-India combined, general index) | Monthly | `v1/inflation/in-cpi.csv` | **In progress** |

Everything is CSV, with one `metadata.json` describing every series: its schema, source, licence, row count and last update.

## Architecture

```
xfinata/
  src/
    catalog/    datasets.yaml — what exists, where it goes, what it promises
    series/     records, merge, invariants, CSV
    sources/    fetching: SBI, MoSPI
    rawstore/   the immutable archive and its manifest
    publish/    the built site
  xtask/        release and maintenance tasks
```

Document parsing is **not** here. Formats live in [Xfina](https://github.com/sakthipriyan/xfina), which this crate depends on: it hands Xfina the bytes and gets structured records back. Xfinata's job is everything around that — fetching, archiving, merging into a series and publishing.

## How it runs

The tool is not a local utility. **It runs in GitHub Actions, on a schedule**, and what it does there is update published data:

```yaml
# sync.yml, once a day
- run: gh release download "$XFINATA_VERSION" --pattern xfinata      # pinned, not built
- run: xfinata sync --config datasets.yaml                           # fetch, archive, rebuild
- run: xfinata site build --out dist                                 # render
- uses: cloudflare/wrangler-action@v4                                # publish
```

The binary is downloaded rather than compiled, so a data update never waits on a build, and every run records which released version produced it. Because nothing runs off a laptop, the R2 write credentials live only in GitHub Secrets.

The full command surface:

```
xfinata sync            --config datasets.yaml [--dataset id] [--dry-run]
xfinata backfill        --dataset id [--from-dir path]
xfinata raw put         --dataset id --origin manual <file>
xfinata raw import      --dataset id --from <dir> --origin upstream
xfinata reconcile       --dataset id
xfinata config validate [--base <ref>]
xfinata site build      --out dist
```

## Development

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three are gates in `Check PR`, which carries the correctness burden precisely because nobody runs the tool by hand before it ships. `cargo xtask` holds the release and maintenance tasks.

## Status

Early. The repository skeleton and CLI surface are in place; the datasets above are being brought up one at a time.

## License

Apache-2.0. The data itself comes from public sources, each credited in `metadata.json` with its own terms.
