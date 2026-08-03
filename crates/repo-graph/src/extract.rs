use crate::model::{
    Author, Commit, Oid, RefKind, RefSelection, Remote, RepoGraph, RepoQuery, RepoRef, TimeWindow,
};
use gix::bstr::ByteSlice;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub enum RepoError {
    NotARepository { path: PathBuf },
    Unreadable { path: PathBuf, message: String },
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoError::NotARepository { path } => {
                write!(f, "not a git repository: {}", path.display())
            }
            RepoError::Unreadable { path, message } => {
                write!(f, "could not read {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for RepoError {}

pub fn extract_repository(path: &Path, query: &RepoQuery) -> Result<RepoGraph, RepoError> {
    let mut repo = gix::discover(path).map_err(|_| RepoError::NotARepository {
        path: path.to_path_buf(),
    })?;
    repo.object_cache_size_if_unset(16 * 1024 * 1024);

    let repo_name = repo
        .workdir()
        .or_else(|| repo.path().parent())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    let shallow = read_shallow_count(repo.common_dir());
    let remotes = read_remotes(&repo);
    let refs = read_refs(&repo, query)?;
    let tips = selected_tips(&repo, &refs, query)?;
    let commits = walk_commits(&repo, tips, query)?;

    Ok(RepoGraph {
        repository_name: repo_name,
        commits,
        refs,
        remotes,
        shallow,
        generated_at: now_secs(),
    })
}

fn read_remotes(repo: &gix::Repository) -> Vec<Remote> {
    repo.remote_names()
        .into_iter()
        .filter_map(|name| {
            let remote = repo.find_remote(name.as_bstr()).ok()?;
            let url = remote
                .url(gix::remote::Direction::Fetch)
                .map(|u| u.to_bstring().to_string())
                .unwrap_or_default();
            Some(Remote {
                name: name.to_string(),
                url,
            })
        })
        .collect()
}

fn read_refs(repo: &gix::Repository, query: &RepoQuery) -> Result<Vec<RepoRef>, RepoError> {
    let mut out = Vec::new();
    if let Ok(head) = repo.head_id() {
        out.push(RepoRef {
            name: "HEAD".into(),
            kind: RefKind::Head,
            target: head.to_string(),
        });
    }

    let refs = repo.references().map_err(|e| RepoError::Unreadable {
        path: repo.path().to_path_buf(),
        message: e.to_string(),
    })?;
    let iter = refs.all().map_err(|e| RepoError::Unreadable {
        path: repo.path().to_path_buf(),
        message: e.to_string(),
    })?;

    for reference in iter {
        let mut reference = reference.map_err(|e| RepoError::Unreadable {
            path: repo.path().to_path_buf(),
            message: e.to_string(),
        })?;
        let name = reference.name().as_bstr().to_string();
        if query.hidden.iter().any(|h| h == &name) {
            continue;
        }
        let Some(kind) = classify_ref(&name, query.include_remotes) else {
            continue;
        };
        let target = reference
            .peel_to_id()
            .map_err(|e| RepoError::Unreadable {
                path: repo.path().to_path_buf(),
                message: e.to_string(),
            })?
            .to_string();
        out.push(RepoRef { name, kind, target });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.target.cmp(&b.target)));
    out.dedup_by(|a, b| a.name == b.name && a.target == b.target);
    Ok(out)
}

fn classify_ref(name: &str, include_remotes: bool) -> Option<RefKind> {
    if name.starts_with("refs/heads/") {
        Some(RefKind::LocalBranch)
    } else if name.starts_with("refs/remotes/") && include_remotes {
        let remote = name
            .trim_start_matches("refs/remotes/")
            .split('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        Some(RefKind::RemoteBranch { remote })
    } else if name.starts_with("refs/tags/") {
        Some(RefKind::Tag)
    } else {
        None
    }
}

fn selected_tips(
    repo: &gix::Repository,
    refs: &[RepoRef],
    query: &RepoQuery,
) -> Result<Vec<Oid>, RepoError> {
    let mut tips = BTreeSet::new();
    for r in refs {
        let selected = match &query.refs {
            RefSelection::All => true,
            RefSelection::Heads => {
                matches!(r.kind, RefKind::LocalBranch | RefKind::RemoteBranch { .. })
            }
            RefSelection::Named(names) => names.iter().any(|n| n == &r.name),
        };
        if selected && r.name != "HEAD" {
            tips.insert(r.target.clone());
        }
    }
    if tips.is_empty() {
        if let Ok(head) = repo.head_id() {
            tips.insert(head.to_string());
        }
    }
    Ok(tips.into_iter().collect())
}

fn walk_commits(
    repo: &gix::Repository,
    tips: Vec<Oid>,
    query: &RepoQuery,
) -> Result<Vec<Commit>, RepoError> {
    let tip_ids = tips
        .iter()
        .filter_map(|t| gix::ObjectId::from_hex(t.as_bytes()).ok())
        .collect::<Vec<_>>();
    let mut commits = Vec::new();
    let mut seen = BTreeSet::new();
    let max = query.max_commits.max(1) as usize;

    let walk = repo
        .rev_walk(tip_ids)
        .all()
        .map_err(|e| RepoError::Unreadable {
            path: repo.path().to_path_buf(),
            message: e.to_string(),
        })?;

    for info in walk {
        if commits.len() >= max {
            break;
        }
        let info = info.map_err(|e| RepoError::Unreadable {
            path: repo.path().to_path_buf(),
            message: e.to_string(),
        })?;
        let oid = info.id.to_string();
        if !seen.insert(oid.clone()) {
            continue;
        }
        let object = info.object().map_err(|e| RepoError::Unreadable {
            path: repo.path().to_path_buf(),
            message: e.to_string(),
        })?;
        let time = object
            .time()
            .map(|t| t.seconds)
            .unwrap_or_else(|_| info.commit_time.unwrap_or_default());
        if !window_includes(time, &query.window) {
            continue;
        }
        let author = object.author().ok();
        let summary = object
            .message_raw_sloppy()
            .lines()
            .next()
            .map(|line| line.as_bstr().to_string())
            .unwrap_or_default();
        commits.push(Commit {
            oid,
            parents: object.parent_ids().map(|p| p.to_string()).collect(),
            author: Author {
                name: author
                    .as_ref()
                    .map(|a| a.name.to_string())
                    .unwrap_or_default(),
                email: author
                    .as_ref()
                    .map(|a| a.email.to_string())
                    .unwrap_or_default(),
            },
            time,
            summary,
        });
    }
    commits.sort_by(|a, b| a.time.cmp(&b.time).then_with(|| a.oid.cmp(&b.oid)));
    Ok(commits)
}

fn window_includes(time: i64, window: &TimeWindow) -> bool {
    match *window {
        TimeWindow::Last(_) => true,
        TimeWindow::Since(start) => time >= start,
        TimeWindow::Range { start, end } => (start..=end).contains(&time),
    }
}

fn read_shallow_count(common_dir: &Path) -> Option<u32> {
    std::fs::read_to_string(common_dir.join("shallow"))
        .ok()
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u32)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
