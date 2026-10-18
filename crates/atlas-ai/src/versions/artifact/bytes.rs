//! Byte-exact, reversible source deltas, independent of displayed line counts.
use super::{paths, validate, ARTIFACT_ENTRY, MAX_ARTIFACT_BYTES};
use atlas_agent::{ArtifactRevision, ArtifactRevisionRef, ArtifactStorage};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io, path::PathBuf};

pub(super) fn hash(source: &[u8]) -> String {
    format!("{:x}", Sha256::digest(source))
}

pub(super) fn delta(old: &[u8], new: &[u8], parent_hash: &str) -> ArtifactStorage {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    ArtifactStorage::Delta {
        parent_hash: parent_hash.into(),
        prefix: prefix as u64,
        removed: old[prefix..old.len() - suffix].to_vec(),
        inserted: new[prefix..new.len() - suffix].to_vec(),
    }
}

fn splice(source: &[u8], prefix: u64, remove: &[u8], insert: &[u8]) -> io::Result<Vec<u8>> {
    let start =
        usize::try_from(prefix).map_err(|_| paths::bad("Artifact delta offset overflows"))?;
    let end = start
        .checked_add(remove.len())
        .ok_or_else(|| paths::bad("Artifact delta length overflows"))?;
    if source.get(start..end) != Some(remove) {
        return Err(paths::bad("Artifact delta preimage does not match"));
    }
    let length = source
        .len()
        .checked_sub(remove.len())
        .and_then(|n| n.checked_add(insert.len()))
        .filter(|n| *n as u64 <= MAX_ARTIFACT_BYTES)
        .ok_or_else(|| paths::bad("Artifact delta exceeds source bound"))?;
    let mut result = Vec::with_capacity(length);
    result.extend_from_slice(&source[..start]);
    result.extend_from_slice(insert);
    result.extend_from_slice(&source[end..]);
    Ok(result)
}

pub(super) fn reconstruct(
    reference: &ArtifactRevisionRef,
) -> io::Result<(ArtifactRevision, Vec<u8>)> {
    reconstruct_inner(reference, &mut HashSet::new())
}

fn reconstruct_inner(
    reference: &ArtifactRevisionRef,
    seen: &mut HashSet<(PathBuf, String)>,
) -> io::Result<(ArtifactRevision, Vec<u8>)> {
    paths::validate_reference(reference)?;
    if seen.len() >= 128
        || !seen.insert((reference.source_root.clone(), reference.revision_id.clone()))
    {
        return Err(paths::bad(
            "Artifact history cycles or exceeds reconstruction bound",
        ));
    }
    let dir = reference
        .source_root
        .join("revisions")
        .join(&reference.revision_id);
    let revision: ArtifactRevision = serde_json::from_slice(&paths::checked_read(
        &dir.join("revision.json"),
        MAX_ARTIFACT_BYTES + 4 * 1024 * 1024,
    )?)?;
    if revision.version != 1
        || revision.id != reference.revision_id
        || reference.source_root.file_name().and_then(|n| n.to_str())
            != Some(revision.family_id.as_str())
        || revision.entry != ARTIFACT_ENTRY
        || revision.bytes > MAX_ARTIFACT_BYTES
    {
        return Err(paths::bad(
            "Artifact manifest identity or format is invalid",
        ));
    }
    validate::provenance(&revision.provenance)?;
    let resolved_parent = paths::resolve_parent(&reference.source_root, &revision)?;
    let source = match &revision.storage {
        ArtifactStorage::Base => paths::checked_read(&dir.join("source.html"), MAX_ARTIFACT_BYTES)?,
        ArtifactStorage::Delta {
            parent_hash,
            prefix,
            removed,
            inserted,
        } => {
            let parent = resolved_parent
                .as_ref()
                .ok_or_else(|| paths::bad("Artifact delta is missing its parent"))?;
            let (parent_revision, old) = reconstruct_inner(parent, seen)?;
            if &parent_revision.source_hash != parent_hash
                || parent_revision.generation.checked_add(1) != Some(revision.generation)
            {
                return Err(paths::bad(
                    "Artifact delta parent hash or generation does not match",
                ));
            }
            let new = splice(&old, *prefix, removed, inserted)?;
            if splice(&new, *prefix, inserted, removed)? != old {
                return Err(paths::bad("Artifact delta does not reverse exactly"));
            }
            new
        }
    };
    if source.len() as u64 != revision.bytes || hash(&source) != revision.source_hash {
        return Err(paths::bad("Artifact source hash or length does not match"));
    }
    validate::html(&source)?;
    Ok((revision, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_deltas_round_trip_binary_and_reordered_text() {
        for (a, b) in [
            (b"a\nb\n".as_slice(), b"b\na\n".as_slice()),
            (&[0, 255, 4], &[0, 127, 4]),
            (b"", b"new"),
            (b"old", b""),
        ] {
            let ArtifactStorage::Delta {
                prefix,
                removed,
                inserted,
                ..
            } = delta(a, b, &hash(a))
            else {
                unreachable!()
            };
            assert_eq!(splice(a, prefix, &removed, &inserted).unwrap(), b);
            assert_eq!(splice(b, prefix, &inserted, &removed).unwrap(), a);
        }
        assert!(splice(b"abc", u64::MAX, b"", b"x").is_err());
        assert!(splice(b"abc", 1, b"wrong", b"x").is_err());
    }
}
