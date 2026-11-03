//! Shared screen or canvas scale for portal chrome.
use atlas_core::display::SLATE_BOARD;

/// Maximized chrome uses screen units; canvas chrome follows the board camera.
pub(crate) fn portal_chrome_scale(maximized: bool, canvas_zoom: f32) -> f32 {
    (if maximized { 1.0 } else { canvas_zoom }).max(SLATE_BOARD.min)
}

#[cfg(test)]
mod tests {
    use super::super::{fit_xf, layout_portal_chrome};
    use super::*;
    use eframe::egui::{pos2, Rect};
    use slate_doc::scene::{Corner, WorldRect};

    #[test]
    fn on_canvas_the_tab_bar_tracks_zoom() {
        let frame = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 300.0));
        let corner = Corner::Rounded {
            radius: slate_doc::media::PORTAL_FRAME_DEFAULT_FILLET,
        };
        let full = layout_portal_chrome(frame, false, false, corner, 1.0);
        let half = layout_portal_chrome(frame, false, false, corner, 0.5);
        let bar_full = full.bar.expect("tab");
        let bar_half = half.bar.expect("tab");
        assert!((bar_half.height() - bar_full.height() * 0.5).abs() < 0.05);
        assert!((half.radius - full.radius * 0.5).abs() < 0.05);
    }

    #[test]
    fn maximized_chrome_ignores_the_authored_model_fit_scale() {
        let model = WorldRect {
            x: 10.0,
            y: 20.0,
            w: 240.0,
            h: 180.0,
        };
        let host = Rect::from_min_max(pos2(0.0, 0.0), pos2(1440.0, 1080.0));
        let xf = fit_xf(model, host);
        assert_eq!(xf.z, 6.0);
        assert_eq!(xf.rect_w2s(model), host);
        assert_eq!(portal_chrome_scale(true, xf.z), 1.0);
        assert_eq!(portal_chrome_scale(true, 0.02), 1.0);
        assert_eq!(portal_chrome_scale(false, 0.5), 0.5);
        assert_eq!(portal_chrome_scale(false, 0.0), SLATE_BOARD.min);
    }
}
