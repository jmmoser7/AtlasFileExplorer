use repo_graph::{extract_repository, layout_graph, RefKind, RepoError, RepoQuery, Size};
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn extracts_refs_commits_and_remotes() {
    let fixture = Fixture::new("repo_graph_extract");
    let repo = fixture_repo(&fixture);

    let graph = extract_repository(&repo, &RepoQuery::default()).expect("extract");
    assert!(graph.commits.len() >= 4, "commits: {}", graph.commits.len());
    assert!(graph
        .refs
        .iter()
        .any(|r| r.name == "refs/heads/main" && matches!(r.kind, RefKind::LocalBranch)));
    assert!(graph.refs.iter().any(|r| r.name == "refs/tags/v1"));
    assert!(graph.remotes.iter().any(|r| r.name == "origin"));
    assert_eq!(graph.shallow, None);
}

#[test]
fn fingerprint_ignores_generated_at() {
    let fixture = Fixture::new("repo_graph_fingerprint");
    let repo = fixture_repo(&fixture);
    let query = RepoQuery::default();
    let mut a = extract_repository(&repo, &query).expect("extract a");
    let mut b = extract_repository(&repo, &query).expect("extract b");

    a.generated_at = 1;
    b.generated_at = 999;
    assert_eq!(a.fingerprint(), b.fingerprint());
}

#[test]
fn layout_is_deterministic_and_labels_refs() {
    let fixture = Fixture::new("repo_graph_layout");
    let repo = fixture_repo(&fixture);
    let graph = extract_repository(&repo, &RepoQuery::default()).expect("extract");
    let query = RepoQuery::default();
    let a = layout_graph(&graph, &query, Size { w: 960.0, h: 540.0 });
    let b = layout_graph(&graph, &query, Size { w: 960.0, h: 540.0 });

    assert_eq!(a, b);
    assert_eq!(a.fingerprint, graph.fingerprint());
    assert!(a.labels.iter().any(|l| l.name == "refs/tags/v1"));
    assert!(!a.placed.is_empty());
}

#[test]
fn missing_repo_is_not_a_repository() {
    let fixture = Fixture::new("repo_graph_missing");
    let missing = fixture.root.join("missing");
    let err = extract_repository(&missing, &RepoQuery::default()).expect_err("missing repo");
    assert!(matches!(err, RepoError::NotARepository { .. }));
}

fn fixture_repo(fixture: &Fixture) -> PathBuf {
    let repo = fixture.root.join("repo");
    run(&fixture.root, &["git", "init", "-b", "main", "repo"]);
    run(&repo, &["git", "config", "user.name", "Atlas Test"]);
    run(
        &repo,
        &["git", "config", "user.email", "atlas@example.invalid"],
    );

    write_file(&repo.join("README.md"), "root\n");
    run(&repo, &["git", "add", "."]);
    run(&repo, &["git", "commit", "-m", "root"]);

    run(&repo, &["git", "checkout", "-b", "feature/a"]);
    write_file(&repo.join("a.txt"), "a\n");
    run(&repo, &["git", "add", "."]);
    run(&repo, &["git", "commit", "-m", "feature a"]);

    run(&repo, &["git", "checkout", "main"]);
    write_file(&repo.join("main.txt"), "main\n");
    run(&repo, &["git", "add", "."]);
    run(&repo, &["git", "commit", "-m", "main work"]);

    run(
        &repo,
        &[
            "git",
            "merge",
            "--no-ff",
            "feature/a",
            "-m",
            "merge feature a",
        ],
    );
    run(&repo, &["git", "tag", "v1"]);

    let bare = fixture.root.join("remote.git");
    run(&fixture.root, &["git", "init", "--bare", "remote.git"]);
    run(
        &repo,
        &["git", "remote", "add", "origin", bare.to_str().unwrap()],
    );
    run(
        &repo,
        &["git", "remote", "add", "upstream", bare.to_str().unwrap()],
    );
    run(&repo, &["git", "push", "origin", "main"]);

    repo
}

fn run(cwd: &Path, args: &[&str]) {
    let (program, rest) = args.split_first().expect("program");
    let output = Command::new(program)
        .args(rest)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {args:?}: {e}"));
    assert!(
        output.status.success(),
        "{args:?} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write fixture file");
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture root");
        Self { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
