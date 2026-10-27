//! Native captures retain their own view schema, separate from Slate cameras.
//! The bounded hex envelope is safe to carry in PNG/JPEG/WebP XMP without
//! interpreting native JSON as XML or evaluating external entities.
use crate::{validation::*, *};
pub const CONNECTOR_CAPTURE_NS: &str = "https://slate.app/ns/connector-capture/1.0/";
const OPEN: &str = "<slatecc:metadata xmlns:slatecc=\"https://slate.app/ns/connector-capture/1.0/\" encoding=\"hex\">";
const CLOSE: &str = "</slatecc:metadata>";
const MAX_PACKET: usize = MAX_VIEW_BYTES * 2 + 16_384;
fn validate(meta: &CaptureMetadata) -> Result<(), ProtocolError> {
    if !valid_id(&meta.source.source_id)
        || meta.source.revision.is_empty()
        || meta.source.revision.len() > 256
        || meta.source != meta.view.source
    {
        return Err(error(
            ErrorCode::SourceMismatch,
            "capture source and native view must match",
        ));
    }
    let schema = ViewSchema {
        namespace: meta.view.schema.clone(),
        version: meta.view.schema_version,
    };
    validate_view(&meta.view, &meta.source, &[schema])?;
    if meta.width_px == 0
        || meta.height_px == 0
        || meta.width_px > MAX_CAPTURE_DIMENSION
        || meta.height_px > MAX_CAPTURE_DIMENSION
        || !valid_utc_timestamp(&meta.captured_at)
    {
        return Err(error(
            ErrorCode::InvalidResult,
            "invalid capture dimensions or timestamp",
        ));
    }
    Ok(())
}
pub fn capture_xmp(meta: &CaptureMetadata) -> Result<String, ProtocolError> {
    validate(meta)?;
    let bytes = serde_json::to_vec(meta)
        .map_err(|_| error(ErrorCode::InvalidResult, "invalid capture metadata"))?;
    let mut hex = String::with_capacity(bytes.len() * 2);
    use std::fmt::Write;
    for byte in bytes {
        write!(&mut hex, "{byte:02x}").expect("String writes do not fail");
    }
    let packet = format!("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\">{OPEN}{hex}{CLOSE}</rdf:Description></rdf:RDF></x:xmpmeta>");
    if packet.len() > MAX_PACKET {
        return Err(error(ErrorCode::InvalidResult, "capture XMP exceeds bound"));
    }
    Ok(packet)
}
pub fn parse_capture_xmp(packet: &str) -> Result<Option<CaptureMetadata>, ProtocolError> {
    if packet.len() > MAX_PACKET {
        return Err(error(ErrorCode::InvalidResult, "capture XMP exceeds bound"));
    }
    let Some((_, rest)) = packet.split_once(OPEN) else {
        return Ok(None);
    };
    if rest.contains(OPEN) {
        return Err(error(
            ErrorCode::InvalidResult,
            "duplicate capture metadata",
        ));
    }
    let (hex, _) = rest
        .split_once(CLOSE)
        .ok_or_else(|| error(ErrorCode::InvalidResult, "incomplete capture metadata"))?;
    if hex.len() % 2 != 0
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(error(
            ErrorCode::InvalidResult,
            "malformed capture metadata encoding",
        ));
    }
    let bytes: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|v| {
            let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
            digit(v[0]) * 16 + digit(v[1])
        })
        .collect();
    let meta = serde_json::from_slice(&bytes)
        .map_err(|_| error(ErrorCode::InvalidResult, "malformed capture metadata JSON"))?;
    validate(&meta)?;
    Ok(Some(meta))
}

const CONTENT_KEY: &str = "slate_source_content";

/// Add an independently observed, selected-file fingerprint for durable view
/// bookmarks. A metadata path never authorizes file I/O. This is provenance,
/// not caller authority, and does not grant source write-back.
pub fn annotate_capture_content(
    meta: &mut CaptureMetadata,
    sha256: &str,
    byte_length: u64,
) -> Result<(), ProtocolError> {
    validate(meta)?;
    if !valid_digest(sha256) || byte_length == 0 {
        return Err(error(
            ErrorCode::InvalidResult,
            "invalid linked-source fingerprint",
        ));
    }
    let mut view = meta.view.clone();
    let state = view.state.as_object_mut().ok_or_else(|| {
        error(
            ErrorCode::UnsupportedSchema,
            "native view state is not an extensible object",
        )
    })?;
    state.insert(
        CONTENT_KEY.into(),
        serde_json::json!({"sha256":sha256,"byte_length":byte_length}),
    );
    validate_view(
        &view,
        &meta.source,
        &[ViewSchema {
            namespace: view.schema.clone(),
            version: view.schema_version,
        }],
    )?;
    meta.view = view;
    Ok(())
}

/// Prepare a camera-only restore against the current bound source. Opaque
/// runtime revisions can change on reconnect. A different scope is compatible
/// only when the host independently verifies identical current file content.
/// The original captured metadata remains untouched and origin is retained.
/// Caller authority, native schema support and units are still checked by the
/// ordinary Session and application adapter; a hash does not bypass them.
pub fn view_for_restore(
    meta: &CaptureMetadata,
    current: &SourceScope,
    verified_content: Option<(&str, u64)>,
) -> Result<ViewSnapshot, ProtocolError> {
    validate(meta)?;
    if !valid_id(&current.source_id) || current.revision.is_empty() || current.revision.len() > 256
    {
        return Err(error(
            ErrorCode::SourceMismatch,
            "invalid current source scope",
        ));
    }
    if meta.view.fidelity == ViewFidelity::PosterOnly {
        return Err(error(
            ErrorCode::UnsupportedSchema,
            "a poster has no restorable native view",
        ));
    }
    if &meta.source == current {
        return Ok(meta.view.clone());
    }
    let content = &meta.view.state[CONTENT_KEY];
    let compatible = verified_content.is_some_and(|(digest, length)| {
        valid_digest(digest)
            && length > 0
            && content["sha256"].as_str() == Some(digest)
            && content["byte_length"].as_u64() == Some(length)
    });
    if !compatible {
        return Err(error(ErrorCode::SourceMismatch,
            "capture belongs to another source or revision; unchanged selected-file content was not verified"));
    }
    let mut view = meta.view.clone();
    view.source = current.clone();
    view.state
        .as_object_mut()
        .ok_or_else(|| {
            error(
                ErrorCode::UnsupportedSchema,
                "native view state is not an object",
            )
        })?
        .insert(
            "slate_restore_origin".into(),
            serde_json::to_value(&meta.source).expect("serializable source scope"),
        );
    validate_view(
        &view,
        current,
        &[ViewSchema {
            namespace: view.schema.clone(),
            version: view.schema_version,
        }],
    )?;
    Ok(view)
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
