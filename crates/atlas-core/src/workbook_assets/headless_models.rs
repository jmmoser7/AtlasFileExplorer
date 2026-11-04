//! Only immutable SDK outputs explicitly linked by a workbook may be collected.
use super::*;

const KIND: &str = "slate.headless-rhino.model.v1";
pub(super) fn is_collectable_output(data_dir: &Path, source: &Path, child: &str) -> bool {
    is_raster(source)
        || is_capture_video(data_dir, source, child)
        || is_owned_headless_model(data_dir, source)
}
#[cfg(test)]
mod tests;

/// Public output provenance only; no prompt, session, grant or source snapshot.
pub fn write_headless_model_provenance(path: &Path, run: &str) -> io::Result<()> {
    let mut params = BTreeMap::new();
    params.insert("output_kind".into(), KIND.into());
    write_generated_sidecar(
        path,
        &GeneratedSidecar {
            prompt: String::new(),
            model: "Rhino 8".into(),
            seed: None,
            params,
            created: file_mtime_secs(path).unwrap_or(0),
            source_run: run.into(),
        },
    )
}

pub(super) fn is_owned_headless_model(data_dir: &Path, source: &Path) -> bool {
    if source.parent() != Some(data_dir.join("headless-rhino/models").as_path())
        || !source
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("3dm"))
    {
        return false;
    }
    let marker = sidecar_path(source);
    if crate::secrets::is_machine_private(&marker) || cloud_blocked(&marker) {
        return false;
    }
    use std::io::Read;
    let Ok(file) = std::fs::File::open(marker) else {
        return false;
    };
    let mut bytes = Vec::new();
    if file.take(8193).read_to_end(&mut bytes).is_err() || bytes.len() > 8192 {
        return false;
    }
    let Ok(provenance) = serde_json::from_slice::<GeneratedSidecar>(&bytes) else {
        return false;
    };
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    provenance
        .params
        .get("output_kind")
        .is_some_and(|k| k == KIND)
        && provenance.source_run == stem
        && !stem.is_empty()
        && stem.len() <= 128
        && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
