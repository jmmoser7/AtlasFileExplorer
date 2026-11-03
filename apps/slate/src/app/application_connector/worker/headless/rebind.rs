//! Exact source preparation shared by accepted output and explicit historical Send.
use super::*;

pub(super) struct SourcePreparation<'a> {
    pub(super) key: &'a BindingKey,
    pub(super) path: &'a Path,
    pub(super) expected: &'a str,
    pub(super) content: Option<&'a str>,
    pub(super) lease: Option<&'a Lease>,
}

pub(super) fn apply(
    draft: &mut HeadlessDraft,
    root: &mut PathBuf,
    serial: &mut u64,
    source: SourcePreparation<'_>,
) -> Result<(), String> {
    let SourcePreparation {
        key,
        path,
        expected,
        content,
        lease,
    } = source;
    let current = snapshot(draft, root, *serial);
    if current.key() != *key || current.binding.source.revision != expected {
        return Err("Headless source changed before scoped preparation".into());
    }
    let bytes = source_snapshot(path)?;
    let revision = model_preview::view_meta::hash_bytes(&bytes);
    if lease.is_some_and(|lease| !lease.live()) {
        return Err("Headless source preparation was cancelled before rebind".into());
    }
    if content.is_some_and(|wanted| wanted != revision) {
        return Err("Accepted headless model hash changed".into());
    }
    let mut source = current.binding.source.clone();
    source.revision = revision;
    source.locator = SourceLocator {
        root_id: None,
        relative: None,
        absolute: Some(path.to_string_lossy().into()),
    };
    for (suffix, operation) in [
        ("unbind", Operation::UnbindSource),
        (
            "bind",
            Operation::BindSource {
                source,
                mode: SourceMode::IsolatedDraft,
                selection_id: "explicit-headless-source-preparation".into(),
            },
        ),
    ] {
        *serial += 1;
        let request_id = format!("prepare-{serial}-{suffix}");
        let caller = CallerAuthority::Human {
            action_id: request_id.clone(),
            target_source: None,
        };
        let source = if suffix == "unbind" {
            Some(current.binding.source.scope())
        } else {
            None
        };
        let reply = draft.handle(
            RequestEnvelope {
                session_id: current.worker.clone(),
                generation: 1,
                request_id,
                caller: caller.clone(),
                source,
                operation,
            },
            &caller,
            &Cancellation::default(),
            |_| {},
        )?;
        if matches!(reply.outcome, ReplyBody::Failure { .. }) {
            return Err("Scoped headless source preparation refused".into());
        }
    }
    draft.set_source_snapshot(bytes)?;
    *root = path
        .parent()
        .ok_or("Model requires a source directory")?
        .into();
    draft.set_source_origin(root.clone());
    Ok(())
}
