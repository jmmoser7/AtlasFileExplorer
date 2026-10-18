use super::*;
use atlas_agent::{AgentTurn, ArtifactCheckpoint};

#[path = "inputs_tests.rs"]
mod inputs_tests;
#[path = "protocol_tests.rs"]
mod protocol_tests;

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "atlas_artifact_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn run(&self, id: &str, parent: Option<ArtifactRevisionRef>) -> ArtifactRun {
        prepare_artifact_run(&self.0, "chart", id, parent).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn provenance() -> ArtifactProvenance {
    ArtifactProvenance {
        prompt: "Make the chart interactive".into(),
        provider: "test".into(),
        model: "test-model".into(),
        checkpoint: ArtifactCheckpoint {
            conversation: "local-conversation".into(),
            through: 2,
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: "Make the chart interactive".into(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "Created chart".into(),
                    at: 2,
                },
            ],
        },
        inputs: vec![],
    }
}
fn page(label: &str) -> Vec<u8> {
    format!("<!doctype html><html><head><style>body{{color:navy}}</style></head><body><p>{label}</p>{}</body></html>", "x".repeat(4096)).into_bytes()
}
fn publish(run: &ArtifactRun, source: &[u8]) -> (ArtifactRevisionRef, ArtifactRevision) {
    std::fs::write(run.staging_output.join(ARTIFACT_ENTRY), source).unwrap();
    publish_artifact_revision(run, provenance()).unwrap()
}

#[test]
fn publication_is_immutable_and_content_lineage_is_independent() {
    let f = Fixture::new("immutable");
    let run = f.run("r1", None);
    validate_prepared_artifact_run(&run).unwrap();
    let one = page("original");
    let (r1, m1) = publish(&run, &one);
    assert_eq!(m1.source_hash, bytes::hash(&one));
    let child = f.run("r2", Some(r1.clone()));
    assert_eq!(
        std::fs::read(child.staging_output.join(ARTIFACT_ENTRY)).unwrap(),
        one
    );
    let (r2, m2) = publish(&child, &page("tweaked"));
    assert!(matches!(m2.storage, ArtifactStorage::Delta { .. }));
    assert_eq!(m2.parent, Some(r1.clone()));
    assert_eq!(std::fs::read(&r1.materialized_entry).unwrap(), one);
    assert_eq!(load_artifact_revision(&r1).unwrap(), m1);
    assert!(publish_artifact_revision(&run, provenance()).is_err());
    let index = super::super::load_versions(&f.0);
    assert_eq!(index.artifacts, vec![r1, r2]);
}

#[test]
fn older_fork_seeds_exact_ancestor_in_separate_owned_family() {
    let f = Fixture::new("fork");
    let first = page("first");
    let (r1, _) = publish(&f.run("r1", None), &first);
    let (r2, _) = publish(&f.run("r2", Some(r1.clone())), &page("later"));
    let fork = prepare_artifact_run(&f.0, "fork", "b1", Some(r1.clone())).unwrap();
    assert_eq!(
        std::fs::read(fork.staging_output.join(ARTIFACT_ENTRY)).unwrap(),
        first
    );
    let (branch, manifest) = publish(&fork, &page("branch"));
    assert_eq!(manifest.parent, Some(r1.clone()));
    assert_ne!(branch.source_root, r1.source_root);
    assert_eq!(
        load_artifact_revision(&r2).unwrap().provenance.checkpoint,
        provenance().checkpoint
    );
    discard_artifact_run(&fork).unwrap();
    assert!(load_artifact_revision(&branch).is_ok());
}

#[test]
fn reconstruction_uses_exact_bytes_and_refuses_corruption() {
    let f = Fixture::new("reconstruct");
    let mut parent = None;
    let mut refs = Vec::new();
    for i in 0..11 {
        let id = format!("r{i}");
        let expected = page(&format!("phase-{i}"));
        let (reference, manifest) = publish(&f.run(&id, parent), &expected);
        if i % CHECKPOINT_EVERY == 0 {
            assert!(matches!(manifest.storage, ArtifactStorage::Base));
        }
        let cache = reference.materialized_entry.parent().unwrap();
        std::fs::remove_dir_all(cache).unwrap();
        let entry = materialize_artifact_revision(&reference).unwrap();
        assert_eq!(std::fs::read(entry).unwrap(), expected);
        parent = Some(reference.clone());
        refs.push(reference);
    }
    let r1 = &refs[1];
    let manifest_path = r1.source_root.join("revisions/r1/revision.json");
    let mut manifest: ArtifactRevision =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    if let ArtifactStorage::Delta { inserted, .. } = &mut manifest.storage {
        inserted[0] ^= 1;
    } else {
        panic!("expected delta");
    }
    std::fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(materialize_artifact_revision(r1).is_err());
    assert!(load_artifact_revision(&refs[2]).is_err());
    assert!(
        load_artifact_revision(&refs[8]).is_ok(),
        "full checkpoint is independent of older delta damage"
    );
}

#[test]
fn incomplete_output_and_checkpoints_never_publish() {
    let f = Fixture::new("invalid");
    let run = f.run("r1", None);
    assert!(publish_artifact_revision(&run, provenance()).is_err());
    for invalid in [
        "not HTML",
        "<html><body><img src=missing.png></body></html>",
        "<html><head><style>body{background:url(https://example.test/x)}</style></head></html>",
        "<html><head><style>body{background:URL('https://example.test/x')}</style></head></html>",
        "<html><head><style>body{background:uRl(\"missing.png\")}</style></head></html>",
        "<html><body style=\"background:url('//example.test/x')\"></body></html>",
        "<html><script src='chart.js'></script></html>",
        "<html><script>fetch('https://example.test')</script></html>",
        "<html><img srcset='data:image/png;base64,abc 1x, https://example.test/x 2x'></html>",
    ] {
        std::fs::write(run.staging_output.join(ARTIFACT_ENTRY), invalid).unwrap();
        assert!(
            publish_artifact_revision(&run, provenance()).is_err(),
            "{invalid}"
        );
    }
    std::fs::write(run.staging_output.join(ARTIFACT_ENTRY), page("valid")).unwrap();
    let mut partial = provenance();
    partial.checkpoint.through += 1;
    assert!(publish_artifact_revision(&run, partial).is_err());
    std::fs::write(run.staging_output.join("data.csv"), "a,b\n1,2").unwrap();
    assert!(publish_artifact_revision(&run, provenance()).is_err());
    assert!(!run.source_root.join("revisions/r1").exists());
}

#[test]
fn offline_canvas_blob_png_export_publishes_the_exact_interactive_source() {
    let f = Fixture::new("canvas-blob-export");
    let run = f.run("png-export", None);
    let source = br##"<!doctype html><html><head><style>
        body{background:navy}
        </style></head><body><button id="export">Export PNG</button><script>
        const exportButton=document.getElementById('export');
        exportButton.addEventListener('click',async()=>{
          const image=document.createElement('canvas');image.width=1500;image.height=300;
          const imageCtx=image.getContext('2d');
          const gradient=imageCtx.createLinearGradient(0,0,image.width,0);
          gradient.addColorStop(0,'blue');gradient.addColorStop(1,'red');
          imageCtx.fillStyle=gradient;imageCtx.fillRect(0,0,image.width,image.height);
          const blob=await new Promise((resolve,reject)=>image.toBlob(result=>result?resolve(result):reject(new Error('PNG encoding failed')),'image/png'));
          const url=URL.createObjectURL(blob),link=document.createElement('a');
          link.href=url;link.download='gradient.png';document.body.appendChild(link);
          link.click();link.remove();setTimeout(()=>URL.revokeObjectURL(url),30000);
        });
        </script></body></html>"##;
    let (reference, _) = publish(&run, source);
    assert_eq!(
        std::fs::read(&reference.materialized_entry).unwrap(),
        source
    );
    verify_artifact_source(&reference.materialized_entry, &hash_artifact_input(source)).unwrap();
}

#[test]
fn reference_layout_and_input_hash_are_pure_and_stable() {
    let root = std::env::temp_dir().join("nonexistent-artifact-history/chart");
    let reference = artifact_revision_reference(&root, "r1").unwrap();
    assert_eq!(
        reference.materialized_entry,
        root.join("materialized/r1/index.html")
    );
    assert_eq!(
        hash_artifact_input(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert!(artifact_revision_reference(&root, "../escape").is_err());
}

#[test]
fn identities_stage_paths_and_source_bounds_are_enforced() {
    let f = Fixture::new("paths");
    for id in ["../escape", "a/b", "a\\b", "CON", "", "r:stream"] {
        assert!(prepare_artifact_run(&f.0, "chart", id, None).is_err());
    }
    let run = f.run("r1", None);
    let mut wrong = run.clone();
    wrong.staging_output = f.0.clone();
    assert!(validate_prepared_artifact_run(&wrong).is_err());
    let file = std::fs::File::create(run.staging_output.join(ARTIFACT_ENTRY)).unwrap();
    file.set_len(MAX_ARTIFACT_BYTES + 1).unwrap();
    assert!(publish_artifact_revision(&run, provenance()).is_err());
    assert!(!run.source_root.join("revisions/r1").exists());
    assert!(
        prepare_artifact_run(&f.0, "chart", "r1", None).is_err(),
        "run directory cannot be reused"
    );
}

#[test]
fn tampered_materialization_and_reference_are_not_accepted() {
    let f = Fixture::new("refs");
    let (reference, _) = publish(&f.run("r1", None), &page("one"));
    let mut escaped = reference.clone();
    escaped.materialized_entry = f.0.join(ARTIFACT_ENTRY);
    assert!(load_artifact_revision(&escaped).is_err());
    std::fs::write(&reference.materialized_entry, page("tampered")).unwrap();
    assert!(materialize_artifact_revision(&reference).is_err());
}

#[test]
fn moved_source_container_resolves_content_parents_relative_first() {
    let f = Fixture::new("relocate");
    let (first, _) = publish(&f.run("r1", None), &page("one"));
    let (second, _) = publish(&f.run("r2", Some(first)), &page("two"));
    let moved_link = f.0.join("relocated");
    std::fs::create_dir(&moved_link).unwrap();
    std::fs::rename(f.0.join("versions"), moved_link.join("versions")).unwrap();
    let new_root = moved_link
        .canonicalize()
        .unwrap()
        .join("versions/artifacts/chart");
    let reference = artifact_revision_reference(&new_root, &second.revision_id).unwrap();
    assert_eq!(bytes::reconstruct(&reference).unwrap().1, page("two"));
    assert_eq!(
        std::fs::read(materialize_artifact_revision(&reference).unwrap()).unwrap(),
        page("two")
    );
}

#[cfg(unix)]
#[test]
fn symbolic_links_in_staged_output_are_rejected() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new("symlink");
    let run = f.run("r1", None);
    let ancestor = f.0.join("outside.html");
    std::fs::write(&ancestor, page("ancestor")).unwrap();
    symlink(&ancestor, run.staging_output.join(ARTIFACT_ENTRY)).unwrap();
    assert!(publish_artifact_revision(&run, provenance()).is_err());
    assert_eq!(std::fs::read(ancestor).unwrap(), page("ancestor"));
}
