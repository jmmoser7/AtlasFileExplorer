//! Render limits and format-owner parity.
use super::*;

#[test]
fn quantize_px_steps_and_clamps() {
    assert_eq!(quantize_px(1.0), 64);
    assert_eq!(quantize_px(100.0), 128);
    assert_eq!(quantize_px(128.0), 128);
    assert_eq!(quantize_px(1e9), MAX_RENDER_PX);
}

#[test]
fn every_model_extension_has_one_preview_owner() {
    let mut preview: Vec<_> = model_preview::extensions().collect();
    preview.sort_unstable();
    let mut media = slate_doc::media::MediaGroup::Model.extensions();
    media.sort_unstable();
    assert_eq!(
        preview, media,
        "slate-doc model extensions and model-preview readers drifted"
    );
}
