use eframe::egui::{Pos2, Rect, Vec2};

/// Low-profile tool pallet along the bottom of a selected paged image.
pub(crate) fn pages_album_layout(host: Rect, zoom: f32) -> PagesAlbumLayout {
    let btn = atlas_shell::selection_tools::CORNER_HEIGHT * zoom;
    let inner = atlas_shell::selection_tools::IMAGE_PANEL_INSET * zoom;
    let outer_width = host.width().max(3.0 * inner + btn + 8.0 * zoom);
    let height = btn + inner * 2.0;
    let pallet = Rect::from_min_max(
        Pos2::new(host.center().x - outer_width * 0.5, host.bottom()),
        Pos2::new(host.center().x + outer_width * 0.5, host.bottom() + height),
    );
    let unbundle = Rect::from_center_size(
        Pos2::new(pallet.right() - inner - btn * 0.5, pallet.center().y),
        Vec2::splat(btn),
    );
    let album = Rect::from_min_max(
        Pos2::new(pallet.left() + inner, pallet.top() + inner),
        Pos2::new(
            (unbundle.left() - inner).max(pallet.left() + inner + 8.0 * zoom),
            pallet.bottom() - inner,
        ),
    );
    PagesAlbumLayout {
        pallet,
        album,
        unbundle,
    }
}

pub(crate) struct PagesAlbumLayout {
    pub pallet: Rect,
    pub album: Rect,
    pub unbundle: Rect,
}

#[derive(Default)]
pub(crate) struct PagesAlbumOutcome {
    pub focus: u16,
    pub commit_page: bool,
    pub unbundle: bool,
    pub hover: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_album_is_a_thin_pallet_below_the_host() {
        let host = Rect::from_min_size(Pos2::new(40.0, 20.0), Vec2::new(320.0, 240.0));
        let layout = pages_album_layout(host, 1.0);
        assert!(layout.pallet.height() <= 48.0);
        assert_eq!(layout.pallet.top(), host.bottom());
        assert!(layout.pallet.bottom() > host.bottom());
        assert!(layout.album.width() > layout.unbundle.width());
        assert!(layout.pallet.contains(layout.album.center()));
        assert!(layout.pallet.contains(layout.unbundle.center()));

        let narrow = pages_album_layout(
            Rect::from_min_size(Pos2::new(40.0, 20.0), Vec2::new(12.0, 24.0)),
            1.0,
        );
        assert!(narrow.pallet.width() > 12.0);
        assert!(!narrow.album.intersects(narrow.unbundle));

        for zoom in [0.25, 4.0] {
            let scaled = pages_album_layout(host, zoom);
            assert!(scaled.pallet.height() / zoom <= 48.0);
            assert!(scaled.pallet.contains_rect(scaled.album));
            assert!(scaled.pallet.contains_rect(scaled.unbundle));
            assert!(!scaled.album.intersects(scaled.unbundle));
        }
    }
}
