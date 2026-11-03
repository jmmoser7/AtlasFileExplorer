//! Fixed, measured September 27 comparison for source-change effort analysis.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::collect::{civil_from_unix, git_input_output, git_output, line_counts};
use crate::observatory::Package;
use crate::observatory_git::source_path;

const CUTOFF: i64 = 1_790_567_999;
const CUTOFF_UTC: &str = "2026-09-28T03:59:59Z";
const BASELINE_DATE: &str = "2026-09-27";
const MAX_BYTES: u64 = 100_000_000;

#[derive(Serialize)]
pub struct ChangeCost {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub scope: &'static str,
    pub baseline: Baseline,
    pub current: Current,
    pub definitions: BTreeMap<&'static str, &'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub available: bool,
    pub date: &'static str,
    pub commit: Option<String>,
    pub timestamp: Option<i64>,
    #[serde(rename = "committerDateUTC")]
    pub committer_date_utc: Option<String>,
    #[serde(rename = "cutoffUTC")]
    pub cutoff_utc: &'static str,
    pub timezone: &'static str,
    pub totals: Option<Distribution>,
    pub crates: Vec<BaselineCrate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub date: String,
    pub commit: String,
    pub dirty: bool,
    pub tracked_files: Option<usize>,
    pub untracked_files: Option<usize>,
    pub totals: Distribution,
    pub crates: Vec<CurrentCrate>,
}

#[derive(Serialize)]
pub struct BaselineCrate {
    pub name: String,
    pub path: String,
    pub totals: Distribution,
    pub files: Vec<FileMetrics>,
}

#[derive(Serialize)]
pub struct CurrentCrate {
    pub name: String,
    pub path: String,
    pub totals: Distribution,
}

#[derive(Clone, Serialize)]
pub struct FileMetrics {
    pub path: String,
    pub lines: u32,
    pub lines_code: u32,
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct Distribution {
    pub files: usize,
    pub lines_total: u64,
    pub lines_code: u64,
    pub bytes: u64,
    pub over_500: usize,
    pub over_2000: usize,
    pub over_4000: usize,
    pub largest_file: Option<FileMetrics>,
}

struct TreeEntry {
    path: String,
    hash: String,
    bytes: u64,
}

pub fn tracked_paths(root: &Path) -> Option<BTreeSet<String>> {
    let output = git_output(root, &["ls-files", "-z"]).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(
        text.split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

pub fn collect(
    root: &Path,
    date: &str,
    commit: &str,
    dirty: bool,
    packages: &[Package],
) -> ChangeCost {
    let mut baseline = Baseline {
        available: false,
        date: BASELINE_DATE,
        commit: None,
        timestamp: None,
        committer_date_utc: None,
        cutoff_utc: CUTOFF_UTC,
        timezone: "America/New_York",
        totals: None,
        crates: Vec::new(),
    };
    let error = load_baseline(root, packages, &mut baseline).err();
    let current_files: Vec<_> = packages.iter().flat_map(|p| p.files.iter()).collect();
    let tracking_known = current_files.iter().all(|f| f.tracked.is_some());
    let current = Current {
        date: date.into(),
        commit: commit.into(),
        dirty,
        tracked_files: tracking_known.then(|| {
            current_files
                .iter()
                .filter(|f| f.tracked == Some(true))
                .count()
        }),
        untracked_files: tracking_known.then(|| {
            current_files
                .iter()
                .filter(|f| f.tracked == Some(false))
                .count()
        }),
        totals: distribution(current_files.iter().map(|f| FileMetrics {
            path: f.path.clone(),
            lines: f.lines,
            lines_code: f.lines_code,
            bytes: f.bytes,
        })),
        crates: packages
            .iter()
            .map(|p| CurrentCrate {
                name: p.metrics.name.clone(),
                path: p.path.clone(),
                totals: distribution(p.files.iter().map(|f| FileMetrics {
                    path: f.path.clone(),
                    lines: f.lines,
                    lines_code: f.lines_code,
                    bytes: f.bytes,
                })),
            })
            .collect(),
    };
    ChangeCost { available: baseline.available, error, scope: "fixed-current-Slate-source-paths", baseline, current,
        definitions: BTreeMap::from([
            ("scope", "Both sides measure Rust files under src/ and tests/ at today's Slate dependency-closure paths. Crate membership is fixed to today's names/paths, not a reconstruction of the historical product. Third-party sources, examples and build.rs are excluded."),
            ("baseline", "Fixed requested comparison: September 27, 2026 end-of-day in America/New_York, cutoff 2026-09-28T03:59:59Z. The reachable HEAD commit with the latest committer timestamp at/before that instant is selected from metadata. Only that commit's matching Git blobs are measured. This is a fixed baseline, not a rolling automatic date."),
            ("current", "Current working-tree source, including modified and non-indexed Rust files. The baseline is committed source; historical uncommitted edits cannot be reconstructed. Current file details remain in crates[].files, including bytes and tracked."),
            ("bytes", "Baseline bytes are immutable Git blob sizes; current bytes are the UTF-8 source bytes read from disk. Line endings and Unicode affect byte totals. Bytes are not model tokens, latency, or executable size."),
            ("lines", "Both sides call the canonical xtask line counter: lines_total counts Rust text lines; lines_code excludes blank and //-prefixed lines. Baseline source is not AST-reparsed for tests or unsafe counts."),
            ("thresholds", "Counts of files strictly over 500, 2,000 and 4,000 canonical physical lines. These groups overlap. They use the metrics line definition, not the size audit's separate trailing-row convention, and do not represent a lint verdict."),
            ("tracked", "tracked means the path is listed in the current Git index, not that its current contents are committed. False means not indexed and may include ignored source; null means Git tracking status was unavailable."),
            ("interpretation", "These are measured source-footprint and file-concentration changes. They can explain more search, reading, editing and validation work; they do not measure Codex latency or prove a single cause. Build/test wall time and agent usage require separate evidence."),
        ]) }
}

fn load_baseline(root: &Path, packages: &[Package], baseline: &mut Baseline) -> Result<(), String> {
    let shallow = successful_git(root, &["rev-parse", "--is-shallow-repository"])?;
    if shallow.trim_ascii() != b"false" {
        return Err(
            "A complete local Git history is required to select an exact comparison baseline."
                .into(),
        );
    }
    let log = successful_git(root, &["log", "HEAD", "--format=%ct %H"])?;
    let log = std::str::from_utf8(&log).map_err(|_| "Git commit metadata is not UTF-8")?;
    let (timestamp, hash) = nearest_commit(log, CUTOFF)?;
    baseline.commit = Some(hash.clone());
    baseline.timestamp = Some(timestamp);
    baseline.committer_date_utc = Some(iso_utc(timestamp));
    let mut args = vec!["ls-tree", "-r", "-z", "-l", hash.as_str(), "--"];
    let prefixes: Vec<_> = packages
        .iter()
        .flat_map(|p| [format!("{}/src", p.path), format!("{}/tests", p.path)])
        .collect();
    args.extend(prefixes.iter().map(String::as_str));
    let tree = successful_git(root, &args)?;
    let tree = std::str::from_utf8(&tree).map_err(|_| "Git source paths are not UTF-8")?;
    let entries = parse_tree(tree)?
        .into_iter()
        .filter(|e| packages.iter().any(|p| source_path(&e.path, &p.path)))
        .collect::<Vec<_>>();
    if entries.iter().map(|e| e.bytes).sum::<u64>() > MAX_BYTES {
        return Err(
            "Historical source exceeds the 100 MB extraction limit; no partial totals were used."
                .into(),
        );
    }
    let unique: BTreeSet<_> = entries.iter().map(|e| e.hash.clone()).collect();
    let request = unique
        .iter()
        .map(|h| format!("{h}\n"))
        .collect::<String>()
        .into_bytes();
    let output = git_input_output(root, &["cat-file", "--batch"], request)
        .map_err(|_| "Git blob batch could not run")?;
    if !output.status.success() {
        return Err("Git blob batch failed; no historical totals were used.".into());
    }
    let blobs = parse_batch(&output.stdout, &unique)?;
    baseline.crates = packages
        .iter()
        .map(|p| {
            let mut files = Vec::new();
            for entry in entries.iter().filter(|e| source_path(&e.path, &p.path)) {
                let (lines, lines_code, bytes) =
                    blobs.get(&entry.hash).ok_or("Missing historical blob")?;
                if *bytes != entry.bytes {
                    return Err("Git tree/blob size mismatch".to_string());
                }
                files.push(FileMetrics {
                    path: entry.path.clone(),
                    lines: *lines,
                    lines_code: *lines_code,
                    bytes: *bytes,
                });
            }
            files.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(BaselineCrate {
                name: p.metrics.name.clone(),
                path: p.path.clone(),
                totals: distribution(files.iter().cloned()),
                files,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    baseline.totals = Some(distribution(
        baseline.crates.iter().flat_map(|p| p.files.iter().cloned()),
    ));
    baseline.available = true;
    Ok(())
}

fn successful_git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = git_output(root, args).map_err(|_| "Git could not be started")?;
    if !output.status.success() {
        return Err("Git comparison metadata could not be read.".into());
    }
    Ok(output.stdout)
}

fn nearest_commit(log: &str, cutoff: i64) -> Result<(i64, String), String> {
    let mut selected: Option<(i64, String)> = None;
    for line in log.lines() {
        let (time, hash) = line
            .split_once(' ')
            .ok_or("Malformed Git commit metadata")?;
        let time = time
            .parse::<i64>()
            .map_err(|_| "Malformed Git commit timestamp")?;
        if !valid_hash(hash) {
            return Err("Malformed Git commit identifier".into());
        }
        if time <= cutoff
            && selected
                .as_ref()
                .is_none_or(|s| time > s.0 || (time == s.0 && hash < s.1.as_str()))
        {
            selected = Some((time, hash.to_string()));
        }
    }
    selected
        .ok_or_else(|| "No reachable commit exists at/before the requested baseline cutoff.".into())
}

fn parse_tree(tree: &str) -> Result<Vec<TreeEntry>, String> {
    let mut entries = Vec::new();
    for record in tree.split('\0').filter(|r| !r.is_empty()) {
        let (metadata, path) = record.split_once('\t').ok_or("Malformed Git tree record")?;
        let fields: Vec<_> = metadata.split_whitespace().collect();
        if fields.len() != 4 {
            return Err("Malformed Git tree metadata".into());
        }
        if !path.ends_with(".rs") {
            continue;
        }
        if !["100644", "100755"].contains(&fields[0])
            || fields[1] != "blob"
            || !valid_hash(fields[2])
        {
            return Err(
                "Historical Rust source contains a non-regular blob; no partial totals were used."
                    .into(),
            );
        }
        entries.push(TreeEntry {
            path: path.into(),
            hash: fields[2].into(),
            bytes: fields[3].parse().map_err(|_| "Malformed Git blob size")?,
        });
    }
    Ok(entries)
}

fn parse_batch(
    output: &[u8],
    expected: &BTreeSet<String>,
) -> Result<BTreeMap<String, (u32, u32, u64)>, String> {
    let mut remaining = output;
    let mut blobs = BTreeMap::new();
    while !remaining.is_empty() {
        let end = remaining
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("Missing Git batch header terminator")?;
        let header =
            std::str::from_utf8(&remaining[..end]).map_err(|_| "Invalid Git batch header")?;
        let fields: Vec<_> = header.split(' ').collect();
        if fields.len() != 3 || fields[1] != "blob" || !expected.contains(fields[0]) {
            return Err("Unexpected Git batch object".into());
        }
        let size = fields[2]
            .parse::<usize>()
            .map_err(|_| "Invalid Git batch object size")?;
        let start = end + 1;
        let finish = start
            .checked_add(size)
            .ok_or("Git batch object size overflow")?;
        if output.len() > MAX_BYTES as usize + expected.len() * 100 {
            return Err("Git batch response exceeds the extraction limit".into());
        }
        if remaining.get(finish) != Some(&b'\n') {
            return Err("Truncated Git batch blob".into());
        }
        let source = std::str::from_utf8(&remaining[start..finish])
            .map_err(|_| "Historical Rust source is not UTF-8; no partial totals were used")?;
        let (lines, code) = line_counts(source);
        if blobs
            .insert(fields[0].into(), (lines, code, size as u64))
            .is_some()
        {
            return Err("Duplicate Git batch object".into());
        }
        remaining = &remaining[finish + 1..];
    }
    if blobs.len() != expected.len() {
        return Err("Missing historical Git blobs".into());
    }
    Ok(blobs)
}

fn valid_hash(hash: &str) -> bool {
    [40, 64].contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn distribution(files: impl Iterator<Item = FileMetrics>) -> Distribution {
    let files: Vec<_> = files.collect();
    Distribution {
        files: files.len(),
        lines_total: files.iter().map(|f| u64::from(f.lines)).sum(),
        lines_code: files.iter().map(|f| u64::from(f.lines_code)).sum(),
        bytes: files.iter().map(|f| f.bytes).sum(),
        over_500: files.iter().filter(|f| f.lines > 500).count(),
        over_2000: files.iter().filter(|f| f.lines > 2000).count(),
        over_4000: files.iter().filter(|f| f.lines > 4000).count(),
        largest_file: files
            .iter()
            .max_by(|a, b| a.lines.cmp(&b.lines).then_with(|| b.path.cmp(&a.path)))
            .cloned(),
    }
}

fn iso_utc(timestamp: i64) -> String {
    let (year, month, day) = civil_from_unix(timestamp);
    let seconds = timestamp.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_selection_uses_timestamps_not_log_order_and_includes_cutoff() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let c = "c".repeat(40);
        let log = format!("90 {a}\n110 {b}\n100 {c}\n");
        assert_eq!(nearest_commit(&log, 100).unwrap(), (100, c));
        assert!(nearest_commit(&log, 80).is_err());
        assert_eq!(iso_utc(CUTOFF), CUTOFF_UTC);
    }

    #[test]
    fn tree_and_batch_parsers_preserve_paths_and_count_canonical_utf8_lines() {
        let hash = "a".repeat(40);
        let source = "//! Example\r\n\nfn café() {}\n";
        let tree = format!(
            "100644 blob {hash} {}\tapps/slate/src/a name\t.rs\0",
            source.len()
        );
        let entries = parse_tree(&tree).unwrap();
        assert_eq!(entries[0].path, "apps/slate/src/a name\t.rs");
        let batch = format!("{hash} blob {}\n{source}\n", source.len());
        let expected = BTreeSet::from([hash.clone()]);
        let parsed = parse_batch(batch.as_bytes(), &expected).unwrap();
        assert_eq!(parsed[&hash], (3, 1, source.len() as u64));
        assert!(parse_batch(&batch.as_bytes()[..batch.len() - 1], &expected).is_err());
        assert!(parse_batch(b"", &expected).is_err());
    }

    #[test]
    fn file_distribution_thresholds_overlap_and_largest_ties_are_stable() {
        let files = [500, 501, 2001, 4001]
            .into_iter()
            .enumerate()
            .map(|(i, lines)| FileMetrics {
                path: i.to_string(),
                lines,
                lines_code: lines - 1,
                bytes: u64::from(lines) * 2,
            });
        let totals = distribution(files);
        assert_eq!(
            (
                totals.files,
                totals.over_500,
                totals.over_2000,
                totals.over_4000
            ),
            (4, 3, 2, 1)
        );
        assert_eq!(totals.lines_total, 7003);
        assert_eq!(totals.largest_file.unwrap().path, "3");
        let tied = distribution(["z", "a"].into_iter().map(|path| FileMetrics {
            path: path.into(),
            lines: 10,
            lines_code: 9,
            bytes: 20,
        }));
        assert_eq!(tied.largest_file.unwrap().path, "a");
        assert!(distribution(std::iter::empty()).largest_file.is_none());
    }
}
