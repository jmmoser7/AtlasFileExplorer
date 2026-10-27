//! One native still capture/restore implementation, shared by export and wired inputs.
use super::*;

/// A file fingerprint belongs to the bytes selected when this native session
/// opened. A later file at the same path cannot relabel the live document.
pub(super) struct SourceContent {
    path: PathBuf,
    opened: Option<(String, u64)>,
}
impl SourceContent {
    pub(super) fn open(path: PathBuf) -> Result<Self, String> {
        let opened = selected_content(&path)?;
        Ok(Self { path, opened })
    }
    pub(super) fn revision(&self) -> String {
        self.opened
            .as_ref()
            .map(|(digest, _)| digest.clone())
            .unwrap_or_else(|| model_preview::view_meta::hash_bytes(&[]))
    }
    pub(super) fn confirm_opened(&mut self) -> Result<(), String> {
        if self.verified()?.is_none() {
            self.opened = None;
        }
        Ok(())
    }
    fn verified(&self) -> Result<Option<(String, u64)>, String> {
        let current = selected_content(&self.path)?;
        Ok(self
            .opened
            .as_ref()
            .filter(|opened| Some(*opened) == current.as_ref())
            .cloned())
    }
    fn verified_view(&self, view: &ViewSnapshot) -> Result<Option<(String, u64)>, String> {
        if view.source.revision != self.revision()
            || view.state["dirty"] != false
            || view.state["linked_source_continuity"] != true
        {
            return Ok(None);
        }
        self.verified()
    }
}

pub(super) fn still(
    peer: &mut Exchange,
    address: &PipeAddress,
    pid: u32,
    path: &Path,
    content: &SourceContent,
    author: &slate_doc::scene::CmdAuthor,
) -> Result<[u32; 2], String> {
    let result = request_pixels(peer, author)?;
    write_result(peer, address, pid, path, content, result)
}

fn request_pixels(
    peer: &mut Exchange,
    author: &slate_doc::scene::CmdAuthor,
) -> Result<CaptureResult, String> {
    match peer.authored(
        Operation::Capture {
            schema: ViewSchema {
                namespace: SCHEMA.into(),
                version: 1,
            },
        },
        author,
    )? {
        ReplyBody::Captured { capture } => Ok(capture),
        _ => Err("Connector did not return capture pixels".into()),
    }
}

fn receive_pixels(
    peer: &Exchange,
    address: &PipeAddress,
    pid: u32,
    result: CaptureResult,
) -> Result<(CaptureMetadata, image::DynamicImage), String> {
    let bytes = receive_artifact(
        address,
        &result.artifact,
        pid,
        MAX_ARTIFACT_BYTES as usize,
        DEADLINE,
        &peer.cancel,
    )
    .map_err(|e| e.to_string())?;
    decode_pixels(bytes, result.metadata)
}

fn decode_pixels(
    bytes: Vec<u8>,
    metadata: CaptureMetadata,
) -> Result<(CaptureMetadata, image::DynamicImage), String> {
    let image = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    if image.width() != metadata.width_px || image.height() != metadata.height_px {
        return Err("Capture dimensions disagree with its metadata".into());
    }
    Ok((metadata, image))
}

pub(super) fn preview(
    peer: &mut Exchange,
    address: &PipeAddress,
    pid: u32,
) -> Result<(u32, u32, Vec<u8>), String> {
    let result = request_pixels(peer, &slate_doc::scene::CmdAuthor::Human)?;
    let (_, image) = receive_pixels(peer, address, pid, result)?;
    Ok((image.width(), image.height(), image.to_rgba8().into_raw()))
}

pub(super) fn write_result(
    peer: &Exchange,
    address: &PipeAddress,
    pid: u32,
    path: &Path,
    content: &SourceContent,
    result: CaptureResult,
) -> Result<[u32; 2], String> {
    let (mut metadata, image) = receive_pixels(peer, address, pid, result)?;
    // Pixels remain useful when the linked file is unavailable; only a verified
    // content fingerprint permits restoration into a later native session.
    if let Ok(Some((digest, length))) = content.verified_view(&metadata.view) {
        annotate_capture_content(&mut metadata, &digest, length).map_err(|e| e.to_string())?;
    }
    let xmp = capture_xmp(&metadata).map_err(|e| e.to_string())?;
    let draft =
        super::super::super::board_portal::capture::output::Draft::reserve(path.to_path_buf())?;
    let (w, h) = (image.width(), image.height());
    let rgba = image.to_rgba8();
    use model_preview::view_meta as meta;
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => meta::write_png_with_xmp(draft.path(), &rgba, w, h, &xmp),
        "jpg" | "jpeg" => meta::write_jpeg_with_xmp(draft.path(), &image.to_rgb8(), w, h, 92, &xmp),
        "webp" => meta::write_webp_with_xmp(draft.path(), &rgba, w, h, &xmp),
        _ => return Err("Choose PNG, JPEG or WebP for a native capture".into()),
    }
    .map_err(|e| e.to_string())?;
    draft.commit()?;
    Ok([w, h])
}
fn requires_verified_content(
    metadata: &CaptureMetadata,
    current: &SourceScope,
) -> Result<bool, String> {
    if &metadata.source == current {
        return Ok(false);
    }
    if metadata.view.state["linked_source_continuity"] != true {
        return Err(
            "This capture has no verified linked-source continuity for cross-session recall".into(),
        );
    }
    Ok(true)
}
pub(super) fn restore(
    peer: &mut Exchange,
    path: &Path,
    content: &SourceContent,
) -> Result<(), String> {
    // A copied screenshot is data. It never authorizes opening its named source.
    use image::ImageDecoder;
    if atlas_core::cloud::is_dehydrated(path) {
        return Err("Make this screenshot available locally before restoring its view".into());
    }
    let mut decoder = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let xmp = decoder
        .xmp_metadata()
        .map_err(|e| e.to_string())?
        .ok_or("This image has no native view metadata")?;
    let packet = std::str::from_utf8(&xmp).map_err(|e| e.to_string())?;
    let metadata = parse_capture_xmp(packet)
        .map_err(|e| e.to_string())?
        .ok_or("This image has no Rhino view metadata")?;
    let current = peer
        .channel
        .session()
        .binding()
        .ok_or("No model is bound")?
        .source
        .scope();
    let verified = if !requires_verified_content(&metadata, &current)? {
        None
    } else {
        let live = match peer.human(
            Operation::SnapshotView {
                schema: ViewSchema {
                    namespace: SCHEMA.into(),
                    version: 1,
                },
            },
            true,
        )? {
            ReplyBody::View { view } => view,
            _ => return Err("Rhino did not return its current native view state".into()),
        };
        content.verified_view(&live)?
    };
    let view = view_for_restore(
        &metadata,
        &current,
        verified
            .as_ref()
            .map(|(digest, length)| (digest.as_str(), *length)),
    )
    .map_err(|e| e.to_string())?;
    peer.human(Operation::RestoreView { view }, true)
        .map(|_| ())
}

/// Only the independently selected model path is read; metadata never supplies
/// a path. Hashing is bounded in memory and never hydrates an online-only file.
fn selected_content(path: &Path) -> Result<Option<(String, u64)>, String> {
    if atlas_core::cloud::is_dehydrated(path) {
        return Ok(None);
    }
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let before = file.metadata().map_err(|e| e.to_string())?;
    if before.len() == 0 {
        return Ok(None);
    }
    let digest = model_preview::view_meta::hash_reader(&mut file).map_err(|e| e.to_string())?;
    let after = file.metadata().map_err(|e| e.to_string())?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("The selected model changed while its content was being verified".into());
    }
    Ok(Some((digest, before.len())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_preview_pixels_are_nonblack_and_do_not_gain_export_fingerprints() {
        let source = SourceScope {
            source_id: "native-preview".into(),
            revision: "live-revision".into(),
        };
        let metadata = CaptureMetadata {
            source: source.clone(),
            view: ViewSnapshot {
                source,
                schema: SCHEMA.into(),
                schema_version: 1,
                fidelity: ViewFidelity::ExactNative,
                state: serde_json::json!({"dirty":false}),
            },
            captured_at: "2026-10-25T20:00:00Z".into(),
            width_px: 4,
            height_px: 2,
            consistency: CaptureConsistency::Atomic,
        };
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            4,
            2,
            image::Rgba([20, 80, 120, 255]),
        ));
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let bytes = encoded.into_inner();
        let (actual_metadata, decoded) = decode_pixels(bytes.clone(), metadata.clone()).unwrap();
        assert_eq!(actual_metadata, metadata);
        assert_eq!(decoded.to_rgba8().get_pixel(0, 0).0, [20, 80, 120, 255]);
        let mut wrong = metadata;
        wrong.width_px = 8;
        assert!(decode_pixels(bytes, wrong).is_err());
    }
    #[test]
    fn replaced_file_cannot_relabel_an_already_open_native_document() {
        let root = std::env::temp_dir().join(format!(
            "slate-native-content-{}",
            atlas_agent::request_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("selected.3dm");
        std::fs::write(&path, b"document-A").unwrap();
        let opened = SourceContent::open(path.clone()).unwrap();
        assert!(opened.verified().unwrap().is_some());
        let mut view = ViewSnapshot {
            source: SourceScope {
                source_id: "live-session".into(),
                revision: opened.revision(),
            },
            schema: SCHEMA.into(),
            schema_version: 1,
            fidelity: ViewFidelity::ExactNative,
            state: serde_json::json!({"dirty":false,"linked_source_continuity":true}),
        };
        assert!(opened.verified_view(&view).unwrap().is_some());
        view.state
            .as_object_mut()
            .unwrap()
            .remove("linked_source_continuity");
        assert!(opened.verified_view(&view).unwrap().is_none());
        view.state["linked_source_continuity"] = false.into();
        assert!(opened.verified_view(&view).unwrap().is_none());
        view.state["linked_source_continuity"] = true.into();
        view.state["dirty"] = true.into();
        assert!(opened.verified_view(&view).unwrap().is_none());
        view.state["dirty"] = false.into();
        view.source.revision = "native-draft-revision".into();
        assert!(opened.verified_view(&view).unwrap().is_none());
        std::fs::write(&path, b"document-B").unwrap();
        assert!(opened.verified().unwrap().is_none());
        let reopened = SourceContent::open(path.clone()).unwrap();
        assert!(reopened.verified().unwrap().is_some());
        assert_ne!(opened.revision(), reopened.revision());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn cross_scope_recall_requires_captured_proof_but_exact_scope_keeps_legacy_behavior() {
        let source = SourceScope {
            source_id: "captured-session".into(),
            revision: "same-content".into(),
        };
        let mut metadata = CaptureMetadata {
            source: source.clone(),
            view: ViewSnapshot {
                source: source.clone(),
                schema: SCHEMA.into(),
                schema_version: 1,
                fidelity: ViewFidelity::ExactNative,
                state: serde_json::json!({"dirty":false}),
            },
            captured_at: "2026-10-26T18:00:00Z".into(),
            width_px: 4,
            height_px: 2,
            consistency: CaptureConsistency::Atomic,
        };
        assert!(!requires_verified_content(&metadata, &source).unwrap());
        let other = SourceScope {
            source_id: "new-session".into(),
            revision: source.revision.clone(),
        };
        assert!(requires_verified_content(&metadata, &other).is_err());
        metadata.view.state["linked_source_continuity"] = false.into();
        assert!(requires_verified_content(&metadata, &other).is_err());
        assert!(!requires_verified_content(&metadata, &source).unwrap());
        metadata.view.state["linked_source_continuity"] = true.into();
        assert!(requires_verified_content(&metadata, &other).unwrap());
    }
    #[test]
    fn new_draft_cannot_gain_a_fingerprint_from_an_unobserved_file_write() {
        let path = std::env::temp_dir().join(format!(
            "slate-native-draft-{}.3dm",
            atlas_agent::request_id()
        ));
        let opened = SourceContent::open(path.clone()).unwrap();
        std::fs::write(&path, b"new document").unwrap();
        assert!(opened.verified().unwrap().is_none());
        std::fs::remove_file(path).unwrap();
    }
}
