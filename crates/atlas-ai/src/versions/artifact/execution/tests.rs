use super::*;
use crate::versions::{freeze_artifact_inputs, prepare_artifact_run, validate_artifact_return};
use atlas_agent::{ContextItem, InputSnapshot};
use std::collections::HashMap;

struct Fixture {
    root: PathBuf,
    request: AgentRequest,
    link: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(atlas_agent::request_id());
        fs::create_dir_all(&root).unwrap();
        let run = prepare_artifact_run(&root, "scope", "run", None).unwrap();
        let link = root.join("link");
        fs::create_dir(&link).unwrap();
        let request = AgentRequest {
            id: run.run_id.clone(),
            output_dir: Some(run.staging_output.to_string_lossy().into_owned()),
            artifact: Some(run),
            ..Default::default()
        };
        Self {
            root,
            request,
            link,
        }
    }
    fn freeze(&mut self, snapshot: InputSnapshot) {
        self.request.inputs = freeze_artifact_inputs(
            self.request.artifact.as_ref().unwrap(),
            &snapshot,
            &HashMap::new(),
        )
        .unwrap()
        .0;
    }
    fn scope(&self) -> ArtifactExecutionScope {
        ArtifactExecutionScope::prepare(&self.request, &self.link).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn live() -> Result<(), String> {
    Ok(())
}
fn write(path: &str, content: &str) -> ArtifactOperation {
    ArtifactOperation::WriteFile {
        path: path.into(),
        content: content.into(),
    }
}

#[test]
fn exact_artifact_writes_preserve_the_existing_strict_return_protocol() {
    let mut f = Fixture::new();
    f.freeze(InputSnapshot::default());
    let scope = f.scope();
    let entry = scope.entry.to_string_lossy().into_owned();
    scope
        .execute(
            &write(&entry, "<html><body><button>Toggle</button></body></html>"),
            &live,
        )
        .unwrap();
    let manifest = json!({"id":"run","items":[{"path":scope.entry,"as":"graphic"}]}).to_string();
    scope
        .execute(&write("return.json", &manifest), &live)
        .unwrap();
    validate_artifact_return(f.request.artifact.as_ref().unwrap(), &f.link).unwrap();
    scope
        .execute(
            &write("return.json", &manifest.replace("\"run\"", "\"wrong\"")),
            &live,
        )
        .unwrap();
    assert!(validate_artifact_return(f.request.artifact.as_ref().unwrap(), &f.link).is_err());
}

#[test]
fn scope_refuses_stale_paths_and_both_byte_bounds_before_truncating() {
    let mut f = Fixture::new();
    f.freeze(InputSnapshot::default());
    let scope = f.scope();
    scope
        .execute(&write("index.html", "original"), &live)
        .unwrap();
    for path in [
        "../index.html",
        "sub/index.html",
        "./index.html",
        "C:escape",
        "index.html:stream",
    ] {
        assert!(scope.execute(&write(path, "bad"), &live).is_err(), "{path}");
    }
    for path in [
        &f.root.join("outside.html"),
        &scope.run.source_root.join("index.html"),
    ] {
        assert!(scope
            .execute(&write(&path.to_string_lossy(), "bad"), &live)
            .is_err());
    }
    assert!(scope
        .execute(
            &write("index.html", &"x".repeat(MAX_WRITE_BYTES + 1)),
            &live
        )
        .is_err());
    // Decoded UTF-8 is within 32 KiB but JSON escaping exceeds the envelope.
    assert!(scope
        .execute(&write("index.html", &"\u{0001}".repeat(20_000)), &live)
        .is_err());
    assert!(scope
        .execute(&write("index.html", "cancelled"), &|| Err(
            "stale request".into()
        ))
        .is_err());
    assert_eq!(fs::read(&scope.entry).unwrap(), b"original");
    let mut wrong = f.request.clone();
    wrong.id = "other".into();
    assert!(ArtifactExecutionScope::prepare(&wrong, &f.link).is_err());
}

#[test]
fn revocation_after_an_admitted_write_retains_staged_bytes_and_reports_uncertainty() {
    let mut f = Fixture::new();
    f.freeze(InputSnapshot::default());
    let scope = f.scope();
    let checks = std::cell::Cell::new(0);
    let witness = || {
        let at = checks.get();
        checks.set(at + 1);
        if at >= 2 {
            Err("Stopped".into())
        } else {
            Ok(())
        }
    };
    let error = scope
        .execute(&write("index.html", "admitted output"), &witness)
        .unwrap_err();
    assert!(error.contains("write completed") && error.contains("remains staged"));
    assert_eq!(fs::read(&scope.entry).unwrap(), b"admitted output");
    assert!(!scope.returned.exists());
}

#[test]
fn frozen_png_requires_exact_manifest_and_hash_and_never_writes_the_source() {
    let mut f = Fixture::new();
    let png: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0,
        0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44,
        0x41, 0x54, 0x78, 0x9c, 0x63, 0xd0, 0xa8, 0x38, 0xf1, 0x1f, 0, 4, 0x9c, 2, 0x68, 0x6e,
        0x1b, 0xe5, 0xca, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    let source = f.root.join("source.png");
    fs::write(&source, png).unwrap();
    f.freeze(InputSnapshot {
        wired: vec![ContextItem {
            node: 1,
            text: String::new(),
            images: vec![source.to_string_lossy().into_owned()],
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: None,
        }],
        ..Default::default()
    });
    let scope = f.scope();
    let frozen = scope.inputs.keys().next().unwrap();
    scope.validate_request(&f.request).unwrap();
    let mut external = f.request.clone();
    external.inputs.wired[0].images[0] = source.to_string_lossy().into_owned();
    external.inputs.context.clone_from(&external.inputs.wired);
    assert!(
        ArtifactExecutionScope::prepare(&external, &f.link).is_err(),
        "even identical image bytes at an original/external path are not a frozen reference"
    );
    let mut external_depth = f.request.clone();
    external_depth.inputs.wired[0].depth = Some(source.to_string_lossy().into_owned());
    external_depth
        .inputs
        .context
        .clone_from(&external_depth.inputs.wired);
    assert!(ArtifactExecutionScope::prepare(&external_depth, &f.link).is_err());
    let mut ambient = f.request.clone();
    ambient.inputs.context.push(ambient.inputs.wired[0].clone());
    assert!(ArtifactExecutionScope::prepare(&ambient, &f.link).is_err());
    let mut residual = f.request.clone();
    residual.inputs.wired[0]
        .outputs
        .insert("old".into(), source.to_string_lossy().into_owned());
    residual.inputs.wired[0].active = Some("old".into());
    residual.inputs.context.clone_from(&residual.inputs.wired);
    assert!(ArtifactExecutionScope::prepare(&residual, &f.link).is_err());
    let unlisted = frozen.parent().unwrap().join("unlisted.txt");
    fs::write(&unlisted, b"not a declared source").unwrap();
    let mut wrong_ref = f.request.clone();
    wrong_ref.inputs.wired[0].images[0] = unlisted.to_string_lossy().into_owned();
    wrong_ref.inputs.context.clone_from(&wrong_ref.inputs.wired);
    assert!(ArtifactExecutionScope::prepare(&wrong_ref, &f.link).is_err());
    assert!(scope
        .execute(
            &ArtifactOperation::ReadFile {
                path: unlisted.to_string_lossy().into_owned()
            },
            &live
        )
        .is_err());
    let reply = scope
        .execute(
            &ArtifactOperation::ReadFile {
                path: frozen.to_string_lossy().into_owned(),
            },
            &live,
        )
        .unwrap();
    let image = reply.image.unwrap();
    assert_eq!(image.path, *frozen);
    assert_eq!(image.sha256, hash_artifact_input(png));
    assert!(scope
        .execute(&write(&frozen.to_string_lossy(), "bad"), &live)
        .is_err());
    assert!(scope
        .execute(
            &ArtifactOperation::ReadFile {
                path: source.to_string_lossy().into_owned()
            },
            &live
        )
        .is_err());
    fs::write(frozen, b"changed").unwrap();
    assert!(scope
        .execute(
            &ArtifactOperation::ReadFile {
                path: frozen.to_string_lossy().into_owned()
            },
            &live
        )
        .is_err());
    assert!(ArtifactExecutionScope::prepare(&f.request, &f.link).is_err());
    assert_eq!(fs::read(source).unwrap(), png);
}

#[cfg(unix)]
#[test]
fn scope_rejects_symlink_destinations_before_touching_their_targets() {
    let mut f = Fixture::new();
    f.freeze(InputSnapshot::default());
    let scope = f.scope();
    let outside = f.root.join("outside");
    fs::write(&outside, b"keep").unwrap();
    std::os::unix::fs::symlink(&outside, &scope.entry).unwrap();
    assert!(scope.execute(&write("index.html", "bad"), &live).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"keep");
}
