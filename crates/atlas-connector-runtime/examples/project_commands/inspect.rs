//! Fresh authenticated reopen, with no geometry command or draft save.
use super::*;

pub(super) fn run(args: &[String]) -> Result<()> {
    if args.len() != 5 {
        return Err("usage: project_commands --inspect-existing REGISTRATION PACK APPLICATION SOURCE EVIDENCE_DIR".into());
    }
    let source = PathBuf::from(&args[3]);
    if !source.is_absolute() || !source.is_file() {
        return Err("provide an absolute existing .3dm source".into());
    }
    let source = source.canonicalize()?;
    let registration = Registration::load(&args[0])?;
    let root = registration.approved_root(&source)?;
    let mut evidence = Evidence::new(PathBuf::from(&args[4]))?;
    evidence.record(json!({"event":"user_invoked_inspection","source":source,
        "approved_root":root,
        "scope":"fresh isolated copy; inspection, native opening capture, preserve detach only"}))?;
    let mut channel = connect(
        &registration,
        &args[1],
        &args[2],
        &source,
        &mut evidence,
        "inspect-existing",
    )?;
    bind(
        &mut channel,
        &mut evidence,
        &source,
        &root,
        "bind-inspection-copy",
        "inspection-selected-file".into(),
    )?;
    // Successful Rhino binding has already checked every source ancestor for
    // reparse/cloud/offline flags before opening/copying it. No source bytes
    // are read by this runner before that native locality gate succeeds.
    let result = (|| -> Result<(Value, Value)> {
        let before = file_identity(&source)?;
        let inspected = inspect(&mut channel, &mut evidence, &source, &root, &before)?;
        if before != file_identity(&source)? {
            return Err("selected source changed during native inspection".into());
        }
        Ok((inspected, before))
    })();
    let preserved = detach(&mut channel, &mut evidence, "preserve-inspection");
    let summary = match result {
        Ok((inspect, before)) if preserved.is_ok() => json!({
            "status":"passed","source":source,"source_file":before,
            "source_content_unchanged_after_bind":true,"inspect":inspect,"preserve_detach":true,
            "scope":"fresh authenticated isolated-copy reopen, native opening view and file identity",
            "not_verified":["Slate UI hosting","geometry design criteria","native process exit"]}),
        result => {
            let reason = result.err().map(|e| e.to_string());
            let summary = json!({"status":"failed","source":source,
                "error":reason,
                "preserve_error":preserved.err().map(|e|e.to_string())});
            evidence.save("summary.json", &summary)?;
            return Err(
                "fresh native inspection failed; retained summary and preservation attempt".into(),
            );
        }
    };
    evidence.save("summary.json", &summary)?;
    evidence.record(json!({"event":"inspection_passed","summary":summary}))?;
    println!("Fresh native inspection passed: {}", source.display());
    Ok(())
}

fn file_identity(path: &Path) -> Result<Value> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let before = file.metadata()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let digest = format!("{:x}", hasher.finalize());
    let after = file.metadata()?;
    if before.len() == 0 || before.len() != after.len() || before.modified()? != after.modified()? {
        return Err("model changed during independent file verification".into());
    }
    Ok(json!({"sha256":digest,"byte_length":before.len()}))
}

fn inspect(
    channel: &mut HostChannel,
    evidence: &mut Evidence,
    source: &Path,
    root: &Path,
    source_file: &Value,
) -> Result<Value> {
    verified_window(channel, evidence)?;
    let inspected = invoke(
        channel,
        evidence,
        "inspect-fresh-copy",
        "org.slate.rhino.inspect",
        json!({}),
    )?;
    evidence.save("inspect-reopened.json", &inspected)?;
    if inspected["dirty"] != false || inspected["objects"].as_u64().is_none() {
        return Err("fresh native model is dirty or lacks an object count".into());
    }
    let original_path = inspected["original_path"]
        .as_str()
        .ok_or("inspection lacks original source path")?;
    if Path::new(original_path).canonicalize()? != source
        || inspected["document_id"].as_str()
            != channel.session().binding().map(|b| b.document_id.as_str())
    {
        return Err(
            "inspection identity differs from selected source or bound native document".into(),
        );
    }
    let draft_path = inspected["draft_path"]
        .as_str()
        .ok_or("missing isolated copy path")?;
    let draft_path = Path::new(draft_path).canonicalize()?;
    if draft_path == source
        || draft_path.parent() != Some(root.join(".slate-rhino-drafts").as_path())
    {
        return Err(
            "native inspection did not return a separate copy in the approved draft folder".into(),
        );
    }
    let draft_file = file_identity(&draft_path)?;
    evidence.save("isolated-copy-file.json", &draft_file)?;
    if &draft_file != source_file {
        return Err("fresh isolated model bytes differ from selected source".into());
    }
    let schema = ViewSchema {
        namespace: "org.slate.rhino.view".into(),
        version: 1,
    };
    snapshot(channel, evidence, "view-reopened", &schema)?;
    capture(channel, evidence, "native-reopened", &schema)?;
    Ok(inspected)
}
