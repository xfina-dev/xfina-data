//! Catalog validation, and the published contract check.
//!
//! Both collect every problem before returning, because a config error found
//! one at a time costs a CI round trip each. The messages name the dataset id,
//! since that is what the reader will search the file for.

use std::collections::{HashMap, HashSet};

use semver::Version;

use super::{Catalog, Dataset, Frequency, Status, View, TOOL_VERSION};
use crate::error::{Result, XfinaDataError};

/// The catalog schema version this build understands.
const SUPPORTED_VERSION: u32 = 1;

/// The prefix every published path currently lives under.
const CURRENT_ROOT: &str = "v1/";

/// Check a catalog against every rule, reporting all violations together.
pub fn check(catalog: &Catalog) -> Result<()> {
    let mut problems = Vec::new();

    if catalog.version != SUPPORTED_VERSION {
        problems.push(format!(
            "catalog version {} is not supported (this build understands {SUPPORTED_VERSION})",
            catalog.version
        ));
    }

    check_tool_version(catalog, &mut problems);
    check_archive(catalog, &mut problems);

    if catalog.datasets.is_empty() {
        problems.push("catalog has no datasets".to_string());
    }

    let mut seen_ids: HashSet<&str> = HashSet::new();
    let mut seen_paths: HashMap<&str, &str> = HashMap::new();

    for dataset in &catalog.datasets {
        check_dataset(dataset, &mut problems);

        if !seen_ids.insert(&dataset.id) {
            problems.push(format!("duplicate dataset id `{}`", dataset.id));
        }

        if let Some(other) = seen_paths.insert(&dataset.output.path, &dataset.id) {
            problems.push(format!(
                "`{}` and `{}` both publish to `{}`",
                other, dataset.id, dataset.output.path
            ));
        }
    }

    finish(problems)
}

fn check_tool_version(catalog: &Catalog, problems: &mut Vec<String>) {
    match Version::parse(TOOL_VERSION) {
        Ok(current) if catalog.min_tool_version > current => problems.push(format!(
            "catalog needs tool version {} or newer, this build is {current}",
            catalog.min_tool_version
        )),
        Ok(_) => {}
        // The crate's own version failing to parse means the manifest is
        // broken, which is worth saying plainly rather than ignoring.
        Err(e) => problems.push(format!("tool version `{TOOL_VERSION}` is not semver: {e}")),
    }
}

fn check_archive(catalog: &Catalog, problems: &mut Vec<String>) {
    let archive = &catalog.archive;

    if archive.bucket.trim().is_empty() {
        problems.push("archive bucket is empty".to_string());
    }

    // Keys are joined onto this with a single slash, and every manifest row's
    // link is built that way, so a trailing slash would double up in all of
    // them. Plain http would let anyone on the path alter a raw file in transit.
    if archive.budget_bytes == 0 {
        problems.push("archive budget_bytes must be more than zero".to_string());
    }

    if !archive.public_url.starts_with("https://") || archive.public_url.ends_with('/') {
        problems.push(format!(
            "archive public_url `{}` must be https:// with no trailing slash",
            archive.public_url
        ));
    }
}

fn check_dataset(dataset: &Dataset, problems: &mut Vec<String>) {
    let id = &dataset.id;

    if !is_kebab_case(id) {
        problems.push(format!(
            "`{id}`: id must be lowercase words joined by single hyphens"
        ));
    }

    if dataset.title.trim().is_empty() {
        problems.push(format!("`{id}`: title is empty"));
    }

    if dataset.licence.trim().is_empty() {
        problems.push(format!(
            "`{id}`: licence is empty — every published series states its terms"
        ));
    }

    // Rule 7: a source we may not redistribute can be fetched and archived,
    // but publishing it is the one mistake that cannot be undone by
    // re-deriving, so the catalog refuses to describe it as live.
    if !dataset.public && dataset.status == Status::Published {
        problems.push(format!(
            "`{id}`: marked public: false but status: published — a source we may not \
             redistribute cannot be served"
        ));
    }

    check_path(dataset, problems);
    check_columns(dataset, problems);
    check_prefix(dataset, problems);
    check_preview(dataset, problems);
}

/// The chart palette is validated for colour-blind separation at this many
/// series; a third needs a third validated colour, not a generated one.
const MAX_LINES: usize = 2;

fn check_preview(dataset: &Dataset, problems: &mut Vec<String>) {
    let id = &dataset.id;
    let Some(preview) = &dataset.preview else {
        if dataset.status == Status::Published {
            problems.push(format!(
                "`{id}`: a published dataset needs a `preview` so it can be seen before it is used"
            ));
        }
        return;
    };

    if preview.group.trim().is_empty()
        || preview.summary.trim().is_empty()
        || preview.unit.trim().is_empty()
    {
        problems.push(format!(
            "`{id}`: preview needs a group, a summary and a unit"
        ));
    }
    if preview.lines.is_empty() || preview.lines.len() > MAX_LINES {
        problems.push(format!(
            "`{id}`: preview draws {} line(s); between 1 and {MAX_LINES} are supported",
            preview.lines.len()
        ));
    }
    let values = &dataset.output.columns[1.min(dataset.output.columns.len())..];
    for line in &preview.lines {
        if !values.contains(&line.column) {
            problems.push(format!(
                "`{id}`: preview draws `{}`, which is not a value column of {:?}",
                line.column, values
            ));
        }
    }
    // A year-on-year change needs the same period a year earlier to exist,
    // which only a monthly series guarantees.
    if preview.views.contains(&View::YearOnYear) && dataset.frequency != Frequency::Monthly {
        problems.push(format!(
            "`{id}`: the year-on-year view is only offered for monthly series"
        ));
    }
}

fn check_path(dataset: &Dataset, problems: &mut Vec<String>) {
    let id = &dataset.id;
    let path = &dataset.output.path;

    if !path.starts_with(CURRENT_ROOT) {
        problems.push(format!(
            "`{id}`: output path `{path}` must start with `{CURRENT_ROOT}`"
        ));
    }

    if !path.ends_with(".csv") {
        problems.push(format!(
            "`{id}`: output path `{path}` must end in .csv — CSV is the published format"
        ));
    }

    // The path is joined onto an output directory and used as an object key.
    // Anything that can climb out of that directory is refused here rather
    // than trusted to whatever runs later.
    if path.contains("..") || path.starts_with('/') || path.contains("//") {
        problems.push(format!(
            "`{id}`: output path `{path}` is not a clean relative path"
        ));
    }
}

fn check_columns(dataset: &Dataset, problems: &mut Vec<String>) {
    let id = &dataset.id;
    let columns = &dataset.output.columns;

    if columns.len() < 2 {
        problems.push(format!(
            "`{id}`: needs a key column and at least one value column"
        ));
        return;
    }

    // The key column's name is part of the published contract and says what a
    // row means. A monthly series keyed `date` invites a consumer to parse it
    // as a day, which is how "2026-03" quietly becomes "2026-03-01".
    let expected_key = match dataset.frequency {
        Frequency::Daily => "date",
        Frequency::Monthly => "month",
    };
    if columns[0] != expected_key {
        problems.push(format!(
            "`{id}`: {:?} series must name its first column `{expected_key}`, found `{}`",
            dataset.frequency, columns[0]
        ));
    }

    for column in columns {
        if !is_snake_case(column) {
            problems.push(format!(
                "`{id}`: column `{column}` must be lowercase words joined by single underscores"
            ));
        }
    }

    let mut seen = HashSet::new();
    for column in columns {
        if !seen.insert(column) {
            problems.push(format!("`{id}`: duplicate column `{column}`"));
        }
    }
}

fn check_prefix(dataset: &Dataset, problems: &mut Vec<String>) {
    let id = &dataset.id;
    let prefix = &dataset.raw.prefix;

    if prefix.is_empty() {
        problems.push(format!("`{id}`: raw prefix is empty"));
    }

    if prefix.starts_with('/') || prefix.ends_with('/') || prefix.contains("..") {
        problems.push(format!(
            "`{id}`: raw prefix `{prefix}` must be a clean relative key prefix"
        ));
    }

    let mut seen = HashSet::new();
    for entry in &dataset.raw.unreadable {
        let sha = &entry.sha256;
        if sha.len() != 64
            || !sha
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            problems.push(format!(
                "`{id}`: unreadable sha256 `{sha}` must be 64 lowercase hex digits"
            ));
        }
        if entry.reason.trim().is_empty() {
            problems.push(format!(
                "`{id}`: unreadable {sha} has no reason — the gap it leaves must be explained"
            ));
        }
        if !seen.insert(sha) {
            problems.push(format!("`{id}`: unreadable {sha} is listed twice"));
        }
    }
}

/// Check that this catalog keeps every promise the base one made.
///
/// Published paths and their columns are append-only: people we cannot see
/// have written code against them. Planned datasets are exempt, which is what
/// makes it safe to describe a series in the catalog before it exists.
pub fn check_contract(current: &Catalog, base: &Catalog) -> Result<()> {
    let mut problems = Vec::new();

    for old in base
        .datasets
        .iter()
        .filter(|d| d.status == Status::Published)
    {
        let Some(new) = current.dataset(&old.id) else {
            problems.push(format!(
                "`{}` was published and has been removed — publish a new /vN instead",
                old.id
            ));
            continue;
        };

        if new.output.path != old.output.path {
            problems.push(format!(
                "`{}`: published path changed from `{}` to `{}`",
                old.id, old.output.path, new.output.path
            ));
        }

        if new.output.columns != old.output.columns {
            problems.push(format!(
                "`{}`: published columns changed from {:?} to {:?}",
                old.id, old.output.columns, new.output.columns
            ));
        }

        if new.frequency != old.frequency {
            problems.push(format!(
                "`{}`: frequency changed from {:?} to {:?}, which changes what a row means",
                old.id, old.frequency, new.frequency
            ));
        }
    }

    finish(problems)
}

fn finish(problems: Vec<String>) -> Result<()> {
    if problems.is_empty() {
        return Ok(());
    }
    let mut message = format!("{} problem(s):", problems.len());
    for problem in problems {
        message.push_str("\n  - ");
        message.push_str(&problem);
    }
    Err(XfinaDataError::Config(message))
}

fn is_kebab_case(s: &str) -> bool {
    is_joined_lowercase(s, '-')
}

fn is_snake_case(s: &str) -> bool {
    is_joined_lowercase(s, '_')
}

/// Lowercase alphanumeric words joined by single separators, e.g. `in-cpi`.
///
/// An empty part means a leading, trailing or doubled separator, so this
/// rejects `-in-cpi`, `in--cpi` and `in-cpi-` without special-casing them.
fn is_joined_lowercase(s: &str, separator: char) -> bool {
    !s.is_empty()
        && s.split(separator).all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}
