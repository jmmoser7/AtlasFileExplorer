use super::{capture, RETAIN_REPORTS};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Most recent first. Paths identify a report directory for reveal/acknowledge.
#[derive(Clone, Debug)]
pub struct ReportSummary {
    pub dir: PathBuf,
    pub created_unix_ms: u64,
    pub exit_code: Option<i32>,
    pub panic_count: usize,
}

pub fn reports_dir(app: &str) -> PathBuf {
    crate::index::data_dir()
        .join("crash-reports")
        .join(app_component(app))
}

pub(super) fn app_component(app: &str) -> String {
    let value: String = app
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    if value.is_empty() {
        "app".to_owned()
    } else {
        value
    }
}

pub fn unread_reports(app: &str) -> io::Result<Vec<ReportSummary>> {
    let root = reports_dir(app);
    super::recovery::recover(&root, &crate::index::data_dir().join("session-log"));
    unread_at(&root)
}

pub(super) fn unread_at(root: &Path) -> io::Result<Vec<ReportSummary>> {
    let mut reports = completed_at(root)?;
    reports.retain(|report| report.dir.join("unread").is_file());
    sort_newest(&mut reports);
    Ok(reports)
}

/// Dismiss the startup notice without deleting evidence the user may inspect.
pub fn acknowledge(report_dir: &Path) -> io::Result<()> {
    match fs::remove_file(report_dir.join("unread")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn completed_at(root: &Path) -> io::Result<Vec<ReportSummary>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut reports = Vec::new();
    for entry in entries.flatten().filter(capture::is_real_dir) {
        let dir = entry.path();
        // A live supervisor owns this directory. Pruning and startup notices
        // never race a worker's panic hook or an app that is still running.
        if dir.join(".active").exists() {
            continue;
        }
        let panic_count = panic_count(&dir);
        if !dir.join("report.json").is_file() && !has_evidence(&dir) {
            continue;
        }
        let completion = capture::read_completion(&dir.join("report.json"));
        let created_unix_ms = completion
            .as_ref()
            .and_then(|report| report.ended_unix_ms)
            .or_else(|| capture::read_run(&dir.join("run.json")).map(|run| run.started_unix_ms))
            .unwrap_or_default();
        reports.push(ReportSummary {
            exit_code: completion.and_then(|report| report.exit_code),
            dir,
            created_unix_ms,
            panic_count,
        });
    }
    Ok(reports)
}

fn sort_newest(reports: &mut [ReportSummary]) {
    reports.sort_by(|left, right| {
        right
            .created_unix_ms
            .cmp(&left.created_unix_ms)
            .then_with(|| right.dir.cmp(&left.dir))
    });
}

pub(super) fn prune(root: &Path) -> io::Result<()> {
    let mut reports = completed_at(root)?;
    sort_newest(&mut reports);
    for report in reports.into_iter().skip(RETAIN_REPORTS) {
        // Recheck ownership immediately before cleanup. All targets came from
        // direct, real child directories; no app-provided paths are deleted.
        if !report.dir.join(".active").exists() {
            fs::remove_dir_all(report.dir)?;
        }
    }
    Ok(())
}

pub(super) fn panic_count(dir: &Path) -> usize {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("panic-")
                && name.ends_with(".json")
                && entry.file_type().is_ok_and(|kind| kind.is_file())
        })
        .count()
}

pub(super) fn has_evidence(dir: &Path) -> bool {
    panic_count(dir) != 0 || has_native_exception(dir)
}

pub(super) fn has_native_exception(dir: &Path) -> bool {
    fs::metadata(dir.join("native-exception.txt"))
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}
