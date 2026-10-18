use slate_doc::scene::{WebPortalRef, WebZoom, WorldRect};

/// CSS viewport dimensions shared by paint, host requests, and input mapping.
pub(super) fn css_size(web: &WebPortalRef, rect: WorldRect) -> (u32, u32) {
    let aspect = if rect.w > 0.0 {
        rect.h / rect.w
    } else {
        0.5625
    };
    match web.viewport.zoom {
        WebZoom::Auto => (
            rect.w.max(1.0).round() as u32,
            rect.h.max(1.0).round() as u32,
        ),
        _ => {
            let w = web.viewport.width_css.max(1);
            (w, ((w as f32) * aspect).round().max(1.0) as u32)
        }
    }
}
