//! Renders `docs/metrics/README.md` from the snapshots on disk.

use std::collections::BTreeSet;
use std::path::Path;

use crate::model::Snapshot;
use crate::MetricsError;

/// How many snapshot columns the README shows. Older snapshots stay on disk as
/// JSON — the table is a trend line, not an archive.
const COLUMNS: usize = 8;

pub fn snapshot_json(snapshot: &Snapshot) -> String {
    let mut text =
        serde_json::to_string_pretty(snapshot).expect("Snapshot is a plain serializable tree");
    text.push('\n');
    text
}

/// Every `<YYYY-MM-DD>.json` in `dir`, newest first.
pub fn load_history(dir: &Path) -> Result<Vec<Snapshot>, MetricsError> {
    let mut dates: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| MetricsError::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| MetricsError::io(dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".json") {
            if is_iso_date(stem) {
                dates.push(stem.to_string());
            }
        }
    }
    dates.sort();
    dates.reverse();

    let mut history = Vec::with_capacity(dates.len());
    for date in dates {
        let path = dir.join(format!("{date}.json"));
        let text = std::fs::read_to_string(&path).map_err(|e| MetricsError::io(&path, e))?;
        let snapshot: Snapshot = serde_json::from_str(&text)
            .map_err(|e| MetricsError::manifest(&path, format!("not a snapshot: {e}")))?;
        history.push(snapshot);
    }
    Ok(history)
}

pub fn render_readme(history: &[Snapshot]) -> String {
    let shown: Vec<&Snapshot> = history.iter().take(COLUMNS).collect();
    let mut out = String::new();
    out.push_str(HEADER);

    if shown.is_empty() {
        out.push_str("\nNo snapshots yet. Run `cargo xtask metrics`.\n");
        return out;
    }

    out.push_str("\n## Totals\n\n");
    header_row(&mut out, "Metric", &shown);
    for (label, cell) in totals_rows() {
        row(&mut out, label, &shown, cell);
    }

    out.push_str("\n## Lines of code per crate\n\n");
    header_row(&mut out, "Crate", &shown);
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for snapshot in &shown {
        names.extend(snapshot.crates.iter().map(|c| c.name.as_str()));
    }
    for name in names {
        row(&mut out, name, &shown, move |snapshot| {
            snapshot
                .crates
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.lines_code.to_string())
        });
    }

    out
}

type Cell = fn(&Snapshot) -> Option<String>;

fn totals_rows() -> Vec<(&'static str, Cell)> {
    vec![
        ("commit", |s| Some(s.commit.clone())),
        ("lines_total", |s| Some(s.totals.lines_total.to_string())),
        ("lines_code", |s| Some(s.totals.lines_code.to_string())),
        ("pure_lines_code", |s| {
            Some(s.totals.pure_lines_code.to_string())
        }),
        ("renderer_lines_code", |s| {
            Some(s.totals.renderer_lines_code.to_string())
        }),
        ("pure_ratio", |s| {
            Some(format!("{:.3}", s.totals.pure_ratio))
        }),
        ("crates", |s| Some(s.totals.crates.to_string())),
        ("tests", |s| Some(s.totals.tests.to_string())),
        ("unsafe_blocks", |s| {
            Some(s.totals.unsafe_blocks.to_string())
        }),
        ("direct_dependencies", |s| {
            Some(s.totals.direct_dependencies.to_string())
        }),
        ("format_version", |s| {
            Some(s.model.format_version.to_string())
        }),
        ("node_kinds", |s| Some(s.model.node_kinds.to_string())),
        ("edge_roles", |s| Some(s.model.edge_roles.to_string())),
        ("scene_cmd_variants", |s| {
            Some(s.model.scene_cmd_variants.to_string())
        }),
        ("commands · slate", |s| Some(s.commands.slate.to_string())),
        ("commands · file-atlas", |s| {
            Some(s.commands.file_atlas.to_string())
        }),
        ("deviations · open", |s| {
            Some(s.deviations.open.to_string())
        }),
        ("deviations · accepted", |s| {
            Some(s.deviations.accepted.to_string())
        }),
        ("deviations · closed", |s| {
            Some(s.deviations.closed.to_string())
        }),
    ]
}

fn header_row(out: &mut String, corner: &str, shown: &[&Snapshot]) {
    out.push_str("| ");
    out.push_str(corner);
    for snapshot in shown {
        out.push_str(" | ");
        out.push_str(&snapshot.date);
    }
    out.push_str(" |\n|---");
    for _ in shown {
        out.push_str("|---");
    }
    out.push_str("|\n");
}

fn row(
    out: &mut String,
    label: &str,
    shown: &[&Snapshot],
    cell: impl Fn(&Snapshot) -> Option<String>,
) {
    out.push_str("| ");
    out.push_str(label);
    for snapshot in shown {
        out.push_str(" | ");
        out.push_str(&cell(snapshot).unwrap_or_else(|| "—".to_string()));
    }
    out.push_str(" |\n");
}

fn is_iso_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
}

const HEADER: &str = "\
# Metrics

Generated by `cargo xtask metrics` — **do not hand-edit**, the next run
overwrites this file. Each run also writes `<YYYY-MM-DD>.json` beside it; the
tables below show the eight most recent snapshots, newest first, and older
snapshots stay on disk as JSON.

The point of this file is decision D6: the next audit diffs numbers instead of
impressions.

## How each number is counted

- `lines_total` — every line of every `.rs` file under a crate's `src/` and
  `tests/`. `lines_code` drops blank lines and lines whose trimmed form starts
  with `//`.
- `renderer` — the crate's own `Cargo.toml` names `egui`/`eframe`, or names a
  workspace crate that does (one level of resolution). `pure_ratio` is
  `pure_lines_code / lines_code` to three decimals, and it is the Article I
  canary: it should not fall.
- `tests`, `unsafe_blocks`, `format_version`, the model enum counts, and the
  per-app command counts are parsed with `syn`. Never with a regex — a regex
  counts comments and string literals, which makes two snapshots
  incomparable.
- `deviations` — the ledger rows in `docs/audit/deviations.md`, grouped by its
  `Status` column.
- `direct_dependencies` — distinct dependency names across every member
  manifest (all dependency tables, including `target.*`) that are not
  themselves workspace members.

Crates sort by name, enum variants keep declaration order, and the file walk
sorts by path, so two runs on an unchanged tree produce byte-identical output.
";
