//! Reads and refreshes `docs/audit/deviations.md`.
//!
//! The ledger is the one artifact in this snapshot that is authored by hand and
//! read by machine, so the parser is strict: an unknown `Status` fails the run
//! rather than quietly dropping a row of constitutional debt.

use std::path::{Path, PathBuf};

use crate::{DeviationCounts, MetricsError};

const BEGIN: &str = "<!-- metrics:deviations:begin -->";
const END: &str = "<!-- metrics:deviations:end -->";

pub fn ledger_path(root: &Path) -> PathBuf {
    root.join("docs").join("audit").join("deviations.md")
}

/// Counts the ledger's rows grouped by its `Status` column.
pub fn parse_deviations(path: &Path) -> Result<DeviationCounts, MetricsError> {
    let text = std::fs::read_to_string(path).map_err(|e| MetricsError::io(path, e))?;
    let mut lines = text.lines();

    let header = lines
        .by_ref()
        .find(|line| is_row(line) && columns(line).iter().any(|c| c == "Status"))
        .ok_or_else(|| MetricsError::ledger(path, "no table with a `Status` column"))?;
    let status_col = columns(header)
        .iter()
        .position(|c| c == "Status")
        .expect("the header matched on a Status column");

    let mut counts = DeviationCounts::default();
    for line in lines {
        if !is_row(line) {
            break;
        }
        let cells = columns(line);
        if cells.iter().all(|c| is_separator(c)) {
            continue;
        }
        let status = cells.get(status_col).map(String::as_str).unwrap_or("");
        match status {
            "open" => counts.open += 1,
            "accepted" => counts.accepted += 1,
            "closed" => counts.closed += 1,
            other => {
                return Err(MetricsError::ledger(
                    path,
                    format!(
                        "row `{}` has unknown Status `{other}`",
                        cells.first().map(String::as_str).unwrap_or("?")
                    ),
                ))
            }
        }
    }
    Ok(counts)
}

/// Replaces the text between the `metrics:deviations` markers, leaving every
/// other byte — including the file's line endings — untouched.
pub fn rewrite_deviation_block(path: &Path, counts: &DeviationCounts) -> Result<(), MetricsError> {
    let text = std::fs::read_to_string(path).map_err(|e| MetricsError::io(path, e))?;
    let begin = text
        .find(BEGIN)
        .ok_or_else(|| MetricsError::ledger(path, format!("missing `{BEGIN}`")))?
        + BEGIN.len();
    let end = text[begin..]
        .find(END)
        .ok_or_else(|| MetricsError::ledger(path, format!("missing `{END}`")))?
        + begin;

    let newline = if text[..begin].contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let updated = format!(
        "{}{newline}{}{newline}{}",
        &text[..begin],
        deviation_block(counts),
        &text[end..]
    );
    if updated == text {
        return Ok(());
    }
    std::fs::write(path, updated).map_err(|e| MetricsError::io(path, e))
}

pub fn deviation_block(counts: &DeviationCounts) -> String {
    format!(
        "open: {} · accepted: {} · closed: {}",
        counts.open, counts.accepted, counts.closed
    )
}

fn is_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn is_separator(cell: &str) -> bool {
    !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':')
}

fn columns(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').map(|c| c.trim().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_ledger_rows_by_status() {
        let dir = std::env::temp_dir().join("xtask-ledger-groups");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("deviations.md");
        std::fs::write(
            &path,
            "| ID | Article | Deviation | Status | Opened |\n\
             |---|---|---|---|---|\n\
             | DV-01 | VI | a | open | today |\n\
             | DV-02 | I | b | closed | today |\n\
             | DV-03 | X | c | accepted | today |\n\
             | DV-04 | II | d | open | today |\n\
             \n\
             trailing prose\n",
        )
        .expect("write fixture");
        assert_eq!(
            parse_deviations(&path).expect("parses"),
            DeviationCounts {
                open: 2,
                accepted: 1,
                closed: 1
            }
        );
    }

    #[test]
    fn rewrites_only_the_marked_block() {
        let dir = std::env::temp_dir().join("xtask-ledger-rewrite");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("deviations.md");
        let before =
            format!("intro\n\n{BEGIN}\nopen: 0 · accepted: 0 · closed: 0\n{END}\n\noutro\n");
        std::fs::write(&path, &before).expect("write fixture");
        rewrite_deviation_block(
            &path,
            &DeviationCounts {
                open: 11,
                accepted: 0,
                closed: 0,
            },
        )
        .expect("rewrites");
        let after = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(
            after,
            format!("intro\n\n{BEGIN}\nopen: 11 · accepted: 0 · closed: 0\n{END}\n\noutro\n")
        );
    }
}
