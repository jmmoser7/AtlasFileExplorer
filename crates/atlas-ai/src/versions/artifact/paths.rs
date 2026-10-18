//! Containment, ownership and bounded local reads for the version owner.
use super::{ARTIFACT_ENTRY, MAX_ARTIFACT_BYTES};
use atlas_agent::ArtifactRevisionRef;
use std::{
    io::{self, Read, Write},
    path::Path,
};

pub(super) fn bad(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn io_context(operation: &str, path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("Could not {operation} {}: {error}", path.display()),
    )
}

pub(super) fn identifier(id: &str) -> io::Result<()> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(bad("Artifact identity contains unsafe path characters"));
    }
    let upper = id.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit())
    {
        return Err(bad("Artifact identity is a reserved filename"));
    }
    Ok(())
}

/// Reject links/junctions in every existing path component before reads/writes.
pub(super) fn ordinary(path: &Path) -> io::Result<()> {
    if !normalized_absolute(path) {
        return Err(bad("Artifact paths must be absolute and normalized"));
    }
    // The shared asset guard owns redirect/private-path classification. Windows
    // cloud reparse points are ordinary material when locally available; the
    // byte-reading boundary below separately refuses dehydrated placeholders.
    atlas_core::workbook_assets::require_copy_destination(path)
        .map_err(|error| io_context("validate artifact path", path, error))
}

fn normalized_absolute(path: &Path) -> bool {
    path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
}

pub(super) fn create_directory(path: &Path) -> io::Result<()> {
    ordinary(path)?;
    std::fs::create_dir_all(path)
        .map_err(|error| io_context("create artifact directory", path, error))?;
    ordinary(path)
}

pub(super) fn checked_read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    ordinary(path)?;
    if atlas_core::cloud::is_dehydrated(path) {
        return Err(bad("Artifact source is cloud-only or unavailable"));
    }
    let meta = std::fs::symlink_metadata(path)
        .map_err(|error| io_context("inspect artifact file", path, error))?;
    if !meta.is_file() || meta.len() > limit {
        return Err(bad("Artifact file exceeds its size bound or is not a file"));
    }
    let mut bytes = Vec::with_capacity(meta.len().min(MAX_ARTIFACT_BYTES) as usize);
    std::fs::File::open(path)
        .map_err(|error| io_context("open artifact file", path, error))?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_context("read artifact file", path, error))?;
    if bytes.len() as u64 > limit {
        return Err(bad("Artifact file grew past its size bound"));
    }
    Ok(bytes)
}

pub(super) fn write_new(path: &Path, source: &[u8]) -> io::Result<()> {
    ordinary(path)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_context("create artifact file", path, error))?;
    file.write_all(source)
        .map_err(|error| io_context("write artifact file", path, error))?;
    file.sync_all()
        .map_err(|error| io_context("flush artifact file", path, error))
}

pub(super) fn validate_family(root: &Path, family: &str) -> io::Result<()> {
    identifier(family)?;
    ordinary(root)?;
    if root.file_name().and_then(|n| n.to_str()) != Some(family)
        || root
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
            != Some("artifacts")
        || root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
            != Some("versions")
        || root
            .canonicalize()
            .map_err(|error| io_context("resolve artifact family", root, error))?
            != root
    {
        return Err(bad("Artifact source root is not an owned canonical family"));
    }
    let recorded: String = serde_json::from_slice(&checked_read(&root.join("family.json"), 1024)?)?;
    if recorded != family {
        return Err(bad("Artifact family marker mismatch"));
    }
    Ok(())
}

pub(super) fn reference(root: &Path, id: &str) -> ArtifactRevisionRef {
    ArtifactRevisionRef {
        source_root: root.to_path_buf(),
        revision_id: id.into(),
        materialized_entry: root.join("materialized").join(id).join(ARTIFACT_ENTRY),
    }
}

pub(super) fn validate_reference(r: &ArtifactRevisionRef) -> io::Result<()> {
    identifier(&r.revision_id)?;
    let family = r
        .source_root
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| bad("Artifact family is unnamed"))?;
    validate_family(&r.source_root, family)?;
    if &reference(&r.source_root, &r.revision_id) != r {
        return Err(bad("Artifact entry escapes its pinned revision"));
    }
    ordinary(&r.materialized_entry)
}

/// Relative-first parent links stay inside the detected artifacts container.
pub(super) fn relative_parent(root: &Path, parent: &Path) -> Option<std::path::PathBuf> {
    let container = root.parent()?;
    let tail = parent.strip_prefix(container).ok()?;
    (tail.components().count() == 1).then(|| tail.to_path_buf())
}

pub(super) fn resolve_parent(
    root: &Path,
    revision: &atlas_agent::ArtifactRevision,
) -> io::Result<Option<ArtifactRevisionRef>> {
    let Some(parent) = &revision.parent else {
        if revision.parent_source_relative.is_some() {
            return Err(bad("Artifact relative parent is missing its identity"));
        }
        return Ok(None);
    };
    if let Some(relative) = &revision.parent_source_relative {
        if relative.components().count() != 1
            || !matches!(
                relative.components().next(),
                Some(std::path::Component::Normal(_))
            )
        {
            return Err(bad("Artifact relative parent escapes its source container"));
        }
        let family = relative
            .to_str()
            .ok_or_else(|| bad("Artifact relative family is invalid"))?;
        identifier(family)?;
        let container = root
            .parent()
            .ok_or_else(|| bad("Artifact source has no container"))?;
        let relative_root = container.join(relative);
        if relative_root.exists() {
            let resolved = reference(&relative_root, &parent.revision_id);
            validate_reference(&resolved)?;
            return Ok(Some(resolved));
        }
    }
    validate_reference(parent)?;
    Ok(Some(parent.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containment_validation_is_pure_and_refuses_parent_components() {
        let root = std::env::temp_dir().join("artifact-pure-containment");
        assert!(normalized_absolute(&root.join("versions/artifacts/chart")));
        assert!(!normalized_absolute(Path::new("relative/index.html")));
        assert!(!normalized_absolute(&root.join("../outside/index.html")));
    }

    #[test]
    fn cloud_reparse_metadata_requires_residency_flags_to_refuse_bytes() {
        // FILE_ATTRIBUTE_REPARSE_POINT belongs to hydrated OneDrive entries as
        // well as redirects; it alone never establishes cloud unavailability.
        let reparse_point = 0x400;
        assert!(!atlas_core::cloud::attrs_are_dehydrated(reparse_point));
        for unavailable in [
            atlas_core::cloud::OFFLINE,
            atlas_core::cloud::RECALL_ON_OPEN,
            atlas_core::cloud::RECALL_ON_DATA_ACCESS,
        ] {
            assert!(atlas_core::cloud::attrs_are_dehydrated(
                reparse_point | unavailable
            ));
        }
    }
}
