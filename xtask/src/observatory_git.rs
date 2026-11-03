//! Bounded Git numstat activity; no source snapshots or file contents are read.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::collect::{civil_from_unix, git_output};
use crate::observatory::Package;

const MAX_COMMITS: usize = 2_000;
const RECENT_LIMIT: usize = 60;
const MAX_DAYS: i64 = 180;
const DAY: i64 = 86_400;
const MARKER: &str = "SLATE_COMMIT";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitActivity {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub since: String,
    pub through: String,
    pub date_basis: &'static str,
    pub branch: &'static str,
    pub max_commits: usize,
    pub recent_limit: usize,
    pub truncated: bool,
    pub shallow: Option<bool>,
    pub history_complete: bool,
    pub inspected_commits: Option<usize>,
    pub commits: Option<usize>,
    #[serde(flatten)]
    pub totals: Option<Changes>,
    pub days: Vec<ActivityDay>,
    pub recent: Vec<Commit>,
    pub definitions: BTreeMap<&'static str, &'static str>,
}

#[derive(Clone, Copy, Default, Serialize)]
pub struct Changes {
    pub additions: u64,
    pub deletions: u64,
    pub files: u64,
    pub binary_files: u64,
}

impl Changes {
    fn absorb(&mut self, other: Self) {
        self.additions += other.additions;
        self.deletions += other.deletions;
        self.files += other.files;
        self.binary_files += other.binary_files;
    }
}

#[derive(Serialize)]
pub struct ActivityDay {
    pub date: String,
    pub commits: usize,
    #[serde(flatten)]
    pub changes: Changes,
    pub crates: Vec<CrateChanges>,
}

#[derive(Serialize)]
pub struct CrateChanges {
    pub name: String,
    #[serde(flatten)]
    pub changes: Changes,
}

#[derive(Serialize)]
pub struct Commit {
    pub hash: String,
    pub timestamp: i64,
    pub date: String,
    pub subject: String,
    #[serde(flatten)]
    pub changes: Changes,
    pub crates: Vec<CrateChanges>,
}

struct RawCommit {
    hash: String,
    timestamp: i64,
    subject: String,
    crates: BTreeMap<String, Changes>,
}

pub fn collect(
    root: &Path,
    through: &str,
    packages: &[Package],
    oldest: Option<&str>,
) -> GitActivity {
    let mut activity = GitActivity {
        available: false, error: None, since: through.into(), through: through.into(),
        date_basis: "committer UTC date", branch: "HEAD", max_commits: MAX_COMMITS,
        recent_limit: RECENT_LIMIT, truncated: false, shallow: None,
        history_complete: false, inspected_commits: None, commits: None, totals: None,
        days: Vec::new(), recent: Vec::new(),
        definitions: BTreeMap::from([
            ("scope", "Path-filtered HEAD history for Rust files under src/ and tests/ in today's Slate dependency closure. Excludes merge commits, unrelated paths, working-tree edits and third-party sources. This is committed activity, not runtime activity."),
            ("dates", "Calendar days use each commit's committer timestamp in UTC, not author date. The window ends on the displayed snapshot date. Dates are filtered after bounded inspection, so out-of-order timestamps do not stop traversal. Zero bins mean no observed records; if historyComplete=false, they do not prove no activity."),
            ("changes", "Git numstat added and deleted text lines, including comments and blanks. Additions plus deletions measure churn, not current code size or productivity. Renames are measured as deletion plus addition (--no-renames). Binary line counts are unavailable and excluded; binary_files records those entries."),
            ("commits", "Matching non-merge commits returned by Git's path-filtered HEAD log. A commit touching several in-scope crates is counted once per day. Recent entries sort by committer timestamp, then hash."),
            ("files", "Changed-file entries summed across commits, not unique files. The same file changed in three commits contributes three entries."),
            ("limits", "At most 180 calendar days, beginning at the later of the oldest saved metrics date and that limit. At most 2,000 path-matching non-merge HEAD commits are inspected across all dates, then filtered into the window. A 2,001st record sets truncated=true. Shallow repositories may omit older commits. historyComplete is false for either limitation; all daily values are then partial. Recent details contain at most 60 window commits. Unavailable results contain no activity values."),
        ]),
    };
    let Some(end) = day_index(through) else {
        activity.error = Some("Snapshot date is invalid; Git activity was not collected.".into());
        return activity;
    };
    let start = oldest
        .and_then(day_index)
        .unwrap_or(end - MAX_DAYS + 1)
        .max(end - MAX_DAYS + 1)
        .min(end);
    activity.since = date_at(start);
    let packages: Vec<_> = packages
        .iter()
        .map(|p| (p.metrics.name.clone(), p.path.clone()))
        .collect();
    match read(root, start, end, &packages) {
        Ok((commits, truncated, shallow, inspected)) => {
            populate(&mut activity, commits, start, end);
            activity.truncated = truncated;
            activity.shallow = Some(shallow);
            activity.history_complete = !truncated && !shallow;
            activity.inspected_commits = Some(inspected);
            activity.available = true;
        }
        Err(error) => activity.error = Some(error),
    }
    activity
}

fn read(
    root: &Path,
    start: i64,
    end: i64,
    packages: &[(String, String)],
) -> Result<(Vec<RawCommit>, bool, bool, usize), String> {
    if packages.is_empty() {
        return Err("No in-scope crate source paths; Git activity was not collected.".into());
    }
    let shallow_output = git_output(root, &["rev-parse", "--is-shallow-repository"])
        .map_err(|_| "Git could not be started; activity is unavailable.".to_string())?;
    let shallow = match (
        shallow_output.status.success(),
        shallow_output.stdout.as_slice(),
    ) {
        (true, b"true\n") | (true, b"true\r\n") => true,
        (true, b"false\n") | (true, b"false\r\n") => false,
        _ => {
            return Err(
                "Git repository completeness could not be determined; activity is unavailable."
                    .into(),
            )
        }
    };
    let mut args = vec![
        "log".into(),
        "HEAD".into(),
        "--no-merges".into(),
        "--no-renames".into(),
        "--numstat".into(),
        "-z".into(),
        "--encoding=UTF-8".into(),
        "--format=%x00SLATE_COMMIT%x00%H%x00%ct%x00%s%x00".into(),
        format!("--max-count={}", MAX_COMMITS + 1),
        "--".into(),
    ];
    for (_, path) in packages {
        for directory in ["src", "tests"] {
            args.push(format!(":(top,glob){path}/{directory}/**/*.rs"));
        }
    }
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    let output = git_output(root, &refs)
        .map_err(|_| "Git could not be started; activity is unavailable.".to_string())?;
    if !output.status.success() {
        return Err(format!(
            "Git log failed (exit {:?}); activity is unavailable.",
            output.status.code()
        ));
    }
    let text = String::from_utf8(output.stdout).map_err(|_| {
        "Git activity contains non-UTF-8 data; no partial results were used.".to_string()
    })?;
    let mut commits = parse(&text, packages)?;
    let truncated = commits.len() > MAX_COMMITS;
    commits.truncate(MAX_COMMITS);
    let inspected = commits.len();
    commits.retain(|c| c.timestamp.div_euclid(DAY) >= start && c.timestamp.div_euclid(DAY) <= end);
    commits.sort_by(|a, b| {
        b.timestamp
            .cmp(&a.timestamp)
            .then_with(|| a.hash.cmp(&b.hash))
    });
    Ok((commits, truncated, shallow, inspected))
}

fn parse(text: &str, packages: &[(String, String)]) -> Result<Vec<RawCommit>, String> {
    let mut tokens = text.split('\0');
    let mut commits = Vec::new();
    let mut current: Option<RawCommit> = None;
    while let Some(token) = tokens.next() {
        let token = token.trim_start_matches('\n');
        if token.is_empty() {
            continue;
        }
        if token == MARKER {
            if let Some(commit) = current.take() {
                commits.push(commit);
            }
            let hash = tokens.next().ok_or("Missing Git hash")?;
            if ![40, 64].contains(&hash.len()) || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("Malformed Git commit hash; no partial results were used.".into());
            }
            let timestamp = tokens
                .next()
                .and_then(|t| t.parse().ok())
                .ok_or("Malformed Git timestamp")?;
            let subject = tokens.next().ok_or("Missing Git subject")?;
            current = Some(RawCommit {
                hash: hash.into(),
                timestamp,
                subject: subject.into(),
                crates: BTreeMap::new(),
            });
            continue;
        }
        let commit = current
            .as_mut()
            .ok_or("Git numstat record precedes a commit header")?;
        let mut fields = token.splitn(3, '\t');
        let added = fields.next().ok_or("Missing numstat additions")?;
        let deleted = fields.next().ok_or("Missing numstat deletions")?;
        let path = fields
            .next()
            .filter(|p| !p.is_empty())
            .ok_or("Malformed numstat path or unexpected rename record")?;
        let binary = added == "-" && deleted == "-";
        let (additions, deletions) = if binary {
            (0, 0)
        } else {
            (
                added.parse().map_err(|_| "Invalid numstat additions")?,
                deleted.parse().map_err(|_| "Invalid numstat deletions")?,
            )
        };
        if let Some((name, _)) = packages
            .iter()
            .find(|(_, prefix)| source_path(path, prefix))
        {
            commit
                .crates
                .entry(name.clone())
                .or_default()
                .absorb(Changes {
                    additions,
                    deletions,
                    files: 1,
                    binary_files: u64::from(binary),
                });
        }
    }
    if let Some(commit) = current {
        commits.push(commit);
    }
    Ok(commits)
}

pub(crate) fn source_path(path: &str, package: &str) -> bool {
    path.ends_with(".rs")
        && ["src", "tests"]
            .iter()
            .any(|dir| path.starts_with(&format!("{package}/{dir}/")))
}

fn populate(activity: &mut GitActivity, commits: Vec<RawCommit>, start: i64, end: i64) {
    let mut days = BTreeMap::<String, (usize, BTreeMap<String, Changes>)>::new();
    let mut totals = Changes::default();
    for commit in &commits {
        let day = days
            .entry(date_at(commit.timestamp.div_euclid(DAY)))
            .or_default();
        day.0 += 1;
        for (name, changes) in &commit.crates {
            day.1.entry(name.clone()).or_default().absorb(*changes);
            totals.absorb(*changes);
        }
    }
    activity.commits = Some(commits.len());
    activity.totals = Some(totals);
    activity.days = (start..=end)
        .map(|day| {
            let date = date_at(day);
            let (commits, crates) = days.remove(&date).unwrap_or_default();
            let changes = sum_changes(crates.values().copied());
            ActivityDay {
                date,
                commits,
                changes,
                crates: crate_rows(crates),
            }
        })
        .collect();
    activity.recent = commits
        .into_iter()
        .take(RECENT_LIMIT)
        .map(|commit| Commit {
            date: date_at(commit.timestamp.div_euclid(DAY)),
            hash: commit.hash,
            timestamp: commit.timestamp,
            subject: commit.subject,
            changes: sum_changes(commit.crates.values().copied()),
            crates: crate_rows(commit.crates),
        })
        .collect();
}

fn sum_changes(changes: impl Iterator<Item = Changes>) -> Changes {
    changes.fold(Changes::default(), |mut total, next| {
        total.absorb(next);
        total
    })
}

fn crate_rows(crates: BTreeMap<String, Changes>) -> Vec<CrateChanges> {
    crates
        .into_iter()
        .map(|(name, changes)| CrateChanges { name, changes })
        .collect()
}

fn date_at(day: i64) -> String {
    let (year, month, day) = civil_from_unix(day * DAY);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Find the day through the collector's existing calendar implementation,
/// keeping calendar arithmetic in one owner rather than copying its inverse.
fn day_index(date: &str) -> Option<i64> {
    if date.len() != 10 || date.as_bytes()[4] != b'-' || date.as_bytes()[7] != b'-' {
        return None;
    }
    let target = (
        date.get(..4)?.parse::<i64>().ok()?,
        date.get(5..7)?.parse::<i64>().ok()?,
        date.get(8..)?.parse::<i64>().ok()?,
    );
    let (mut low, mut high) = (-719_528, 2_932_897);
    while low < high {
        let middle = low + (high - low) / 2;
        if civil_from_unix(middle * DAY) < target {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    (civil_from_unix(low * DAY) == target).then_some(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(hash: char, timestamp: i64, subject: &str) -> String {
        format!(
            "\0{MARKER}\0{}\0{timestamp}\0{subject}\0\0\n",
            hash.to_string().repeat(40)
        )
    }

    fn packages() -> Vec<(String, String)> {
        vec![
            ("slate".into(), "apps/slate".into()),
            ("core".into(), "crates/core".into()),
        ]
    }

    #[test]
    fn numstat_preserves_spaces_tabs_and_counts_disabled_renames_and_binary_entries() {
        let text = header('a', 1, "Subject with spaces")
            + "3\t1\tapps/slate/src/old name.rs\0"
            + "0\t8\tapps/slate/src/renamed.rs\0"
            + "8\t0\tapps/slate/src/new\tname.rs\0"
            + "-\t-\tcrates/core/tests/binary.rs\0"
            + "9\t2\tapps/slate/src/not-rust.txt\0";
        let parsed = parse(&text, &packages()).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].subject, "Subject with spaces");
        let slate = parsed[0].crates["slate"];
        assert_eq!((slate.additions, slate.deletions, slate.files), (11, 9, 3));
        assert_eq!(parsed[0].crates["core"].binary_files, 1);
        assert_eq!(parsed[0].crates["core"].additions, 0);
    }

    #[test]
    fn malformed_records_fail_instead_of_producing_partial_totals() {
        for stat in [
            "3\t2\t\0old.rs\0new.rs\0",
            "-\t2\tapps/slate/src/a.rs\0",
            "broken\0",
        ] {
            assert!(parse(&(header('a', 1, "x") + stat), &packages()).is_err());
        }
        assert!(parse("3\t1\tapps/slate/src/a.rs\0", &packages()).is_err());
    }

    #[test]
    fn utc_dates_handle_leap_days_and_invalid_dates() {
        assert_eq!(day_index("1970-01-01"), Some(0));
        assert!(day_index("2026-02-29").is_none());
        assert_eq!(date_at(day_index("2024-02-28").unwrap() + 1), "2024-02-29");
        assert_eq!(date_at(day_index("2024-02-29").unwrap() + 1), "2024-03-01");
        assert!(day_index("2026-13-01").is_none());
    }

    #[test]
    fn daily_rollups_match_commit_totals_and_empty_days_remain_explicit() {
        let root = Path::new(".");
        let mut activity = collect(root, "invalid-date", &[], None);
        assert!(!activity.available && activity.days.is_empty() && activity.commits.is_none());
        let text = header('a', DAY, "first")
            + "3\t1\tapps/slate/src/a.rs\0"
            + &header('b', DAY + 1, "second")
            + "2\t4\tcrates/core/tests/a.rs\0";
        populate(&mut activity, parse(&text, &packages()).unwrap(), 0, 2);
        assert_eq!(activity.days.len(), 3);
        assert_eq!(activity.days.iter().map(|d| d.commits).sum::<usize>(), 2);
        assert_eq!(activity.days[0].commits, 0);
        assert_eq!(activity.days[2].changes.files, 0);
        let total = activity.totals.unwrap();
        assert_eq!((total.additions, total.deletions, total.files), (5, 5, 2));
        assert_eq!(
            activity
                .days
                .iter()
                .map(|d| d.changes.additions)
                .sum::<u64>(),
            total.additions
        );
        assert_eq!(
            activity.days[1]
                .crates
                .iter()
                .map(|c| c.changes.deletions)
                .sum::<u64>(),
            total.deletions
        );
    }
}
