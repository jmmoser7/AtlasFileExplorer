use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

pub type Oid = String;
pub type CommitIx = usize;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Commit {
    pub oid: Oid,
    pub parents: Vec<Oid>,
    pub author: Author,
    pub time: i64,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoRef {
    pub name: String,
    pub kind: RefKind,
    pub target: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RefKind {
    LocalBranch,
    RemoteBranch { remote: String },
    Tag,
    Head,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoGraph {
    pub repository_name: String,
    pub commits: Vec<Commit>,
    pub refs: Vec<RepoRef>,
    pub remotes: Vec<Remote>,
    pub shallow: Option<u32>,
    pub generated_at: u64,
}

impl RepoGraph {
    /// Stable content fingerprint used by portal layout and beacon writes.
    ///
    /// `generated_at` is deliberately excluded: two extractions over identical
    /// object data must produce the same fingerprint.
    pub fn fingerprint(&self) -> String {
        let mut h = StableHasher::default();
        self.repository_name.hash(&mut h);
        self.commits.hash(&mut h);
        self.refs.hash(&mut h);
        self.remotes.hash(&mut h);
        self.shallow.hash(&mut h);
        format!("{:016x}", h.finish())
    }

    pub fn commit_index(&self) -> BTreeMap<&str, CommitIx> {
        self.commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.oid.as_str(), i))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoQuery {
    pub refs: RefSelection,
    pub include_remotes: bool,
    pub hidden: Vec<String>,
    pub window: TimeWindow,
    pub as_of: Option<AsOf>,
    pub axis: TimeAxis,
    pub trunk: Option<String>,
    pub max_commits: u32,
}

impl Default for RepoQuery {
    fn default() -> Self {
        Self {
            refs: RefSelection::All,
            include_remotes: true,
            hidden: Vec::new(),
            window: TimeWindow::Last(2000),
            as_of: None,
            axis: TimeAxis::Topological,
            trunk: None,
            max_commits: 2000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefSelection {
    All,
    Heads,
    Named(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeWindow {
    Last(u32),
    Since(i64),
    Range { start: i64, end: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsOf {
    Commit(Oid),
    Date(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeAxis {
    Topological,
    Chronological,
}

#[derive(Default)]
struct StableHasher(u64);

impl Hasher for StableHasher {
    fn write(&mut self, bytes: &[u8]) {
        // FNV-1a: small, deterministic, and enough for cache invalidation keys.
        if self.0 == 0 {
            self.0 = 0xcbf29ce484222325;
        }
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}
