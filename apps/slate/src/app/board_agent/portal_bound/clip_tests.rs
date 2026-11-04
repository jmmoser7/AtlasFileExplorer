use super::*;
use slate_doc::agent_chat::ChatView;

#[test]
fn fitted_transcript_clip_reserves_the_complete_raster_pixel_without_layout_changes() {
    let fitted = ChatView {
        fit_height: true,
        size: Some([680.0, 500.0]),
        ..Default::default()
    };
    for ppp in [1.0, 1.5, 2.0] {
        for zoom in [0.5, 0.72, 1.0, 1.5, 2.0] {
            for phase in [0.0, 0.1, 0.4, 0.9] {
                let view = Rect::from_min_max(
                    Pos2::new(100.0, 100.0),
                    Pos2::new(600.0, 100.0 + 510.4 * zoom + phase),
                );
                // This is the existing bottom pad/gap, not new layout space.
                let limit = Rect::from_min_max(
                    view.min,
                    Pos2::new(view.right(), view.bottom() + 14.0 * zoom),
                );
                let clip =
                    transcript_paint_clip(view, limit, ppp, &fitted, train_ux::CardFold::Open);
                assert_eq!(clip.left(), view.left());
                assert_eq!(clip.top(), view.top());
                assert_eq!(clip.right(), view.right());
                assert!(limit.contains_rect(clip));
                assert!(clip.bottom() >= view.bottom());
                assert_eq!(clip.bottom(), (view.bottom() * ppp).ceil() / ppp);
                // Backend rounds scissor edges; ink remains unrounded.
                assert!(
                    (clip.bottom() * ppp).round() >= view.bottom() * ppp,
                    "ppp={ppp}, zoom={zoom}, phase={phase}, view={view:?}, clip={clip:?}"
                );
                assert_eq!(
                    transcript_paint_clip(view, limit, ppp, &fitted, train_ux::CardFold::Open),
                    clip
                );
                // Body/canvas/control boundaries still win over raster fitting.
                let clipped = cap_bottom(limit, view.bottom() - 2.0);
                assert_eq!(
                    transcript_paint_clip(view, clipped, ppp, &fitted, train_ux::CardFold::Open),
                    view.intersect(clipped)
                );
            }
        }
    }
}

#[test]
fn manual_partial_and_window_transcript_viewports_keep_genuine_clipping() {
    let view = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(600.0, 610.4));
    let limit = view.expand(14.0);
    let manual = ChatView {
        train: true,
        size: Some([680.0, 500.0]),
        ..Default::default()
    };
    let window = ChatView::default();
    let fitted = ChatView {
        fit_height: true,
        ..manual.clone()
    };
    for ppp in [1.0, 1.5, 2.0] {
        // Keep the genuine manual crop at physical610.4 for each display.
        let view = Rect::from_min_max(view.min, Pos2::new(view.right(), 610.4 / ppp));
        let limit = view.expand(14.0);
        for (chat, fold) in [
            (&manual, train_ux::CardFold::Open),
            (&window, train_ux::CardFold::Open),
            (&fitted, train_ux::CardFold::Partial),
            (&fitted, train_ux::CardFold::Collapsed),
        ] {
            let clip = transcript_paint_clip(view, limit, ppp, chat, fold);
            assert_eq!(
                clip, view,
                "manual/capped/partial viewports must not gain paint room"
            );
            assert!(
                (clip.bottom() * ppp).round() < view.bottom() * ppp,
                "a manual viewport intentionally clips ink on its fractional end"
            );
        }
    }
    let automatic_train = ChatView {
        train: true,
        ..Default::default()
    };
    assert_eq!(
        transcript_paint_clip(view, limit, 1.0, &automatic_train, train_ux::CardFold::Open)
            .bottom(),
        611.0
    );
}
