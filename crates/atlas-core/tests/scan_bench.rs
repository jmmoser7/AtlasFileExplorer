//! Discovery timing on a 20k-file directory: the walk itself versus the owner
//! lookup that used to be inside it.
//!
//! Ignored by default — it writes a 20k-file corpus, which has no business
//! running on every `cargo test`. Run it deliberately:
//!
//!   cargo test -p atlas-core --release --test scan_bench -- --ignored --nocapture

use atlas_core::scanner::{start_scan, ScanMsg};
use crossbeam_channel::unbounded;
use std::path::PathBuf;
use std::time::Instant;

const N: usize = 20_000;

fn build_corpus() -> PathBuf {
    let root = std::env::temp_dir().join("atlas_scan_bench");
    if root.join("done.marker").exists() {
        return root;
    }
    let _ = std::fs::remove_dir_all(&root);
    // Spread over subdirectories the way a real project is.
    for d in 0..20 {
        std::fs::create_dir_all(root.join(format!("dir{d:02}"))).unwrap();
    }
    for i in 0..N {
        let p = root.join(format!("dir{:02}/file{i:05}.jpg", i % 20));
        std::fs::write(p, b"x").unwrap();
    }
    std::fs::write(root.join("done.marker"), b"").unwrap();
    root
}

#[test]
#[ignore = "writes a 20k-file corpus; run explicitly"]
fn discovery_and_owner_pass_are_timed_separately() {
    let t = Instant::now();
    let root = build_corpus();
    println!("corpus of {N} files ready in {:?}", t.elapsed());

    // Warm the OS directory cache so this measures our code, not first touch.
    for _ in 0..2 {
        let (tx, rx) = unbounded();
        let _h = start_scan(root.clone(), 1, tx);
        while let Ok((_, msg)) = rx.recv() {
            if matches!(msg, ScanMsg::Done { .. }) {
                break;
            }
        }
    }

    let (tx, rx) = unbounded();
    let t = Instant::now();
    let _h = start_scan(root.clone(), 2, tx);
    let mut first_batch = None;
    let mut entries = Vec::new();
    loop {
        match rx.recv().unwrap() {
            (_, ScanMsg::Batch(b)) => {
                first_batch.get_or_insert_with(|| t.elapsed());
                entries.extend(b);
            }
            (_, ScanMsg::Dirs(_)) => {}
            (_, ScanMsg::Done { files, elapsed_ms }) => {
                println!(
                    "discovery:        {files} files in {elapsed_ms} ms  (first batch at {:?})",
                    first_batch.unwrap()
                );
                break;
            }
        }
    }
    assert_eq!(entries.len(), N + 1, "N files plus the corpus marker");
    assert!(
        entries.iter().all(|e| e.owner.is_empty()),
        "discovery must not resolve owners"
    );

    // What the walk used to carry inline, now deferred and off the critical path.
    let todo: Vec<(String, PathBuf)> = entries
        .iter()
        .map(|e| (e.rel.clone(), e.path.clone()))
        .collect();
    let (otx, orx) = unbounded();
    let t = Instant::now();
    let _oh = atlas_core::owners::start_owner_pass(todo, 2, otx);
    let mut resolved = 0;
    while let (_, atlas_core::owners::OwnerMsg::Batch(b)) = orx.recv().unwrap() {
        resolved += b.len();
    }
    let owner_time = t.elapsed();
    println!(
        "owner pass:       {resolved} owners in {:?} (was serialized inside discovery)",
        owner_time
    );
    println!(
        "\ndiscovery is now free of {:?} of per-file security queries",
        owner_time
    );
}
