use crate::{
    icons::{self, Icon},
    theme::Palette,
};
use eframe::egui::{Color32, Painter, Pos2, Rect, Vec2};

/// Fixed height of the raised tool notch in board units.
pub const BLISTER_TAB_HEIGHT: f32 = 40.0;
pub const BLISTER_SHOULDER_RUN: f32 = 84.0;

/// Which media blister presentation is being laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlisterPresentation {
    /// Bare icons centered on the image's lower edge, with a shallow cutout.
    Collapsed,
    /// The existing raised menu, fully inside the host.
    Overlay,
}

/// Canonical shell geometry for a media blister and its optional host cutout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlisterLayout {
    pub band: Rect,
    pub cutout: Option<Rect>,
    count: usize,
    presentation: BlisterPresentation,
}

impl BlisterLayout {
    /// Bare media glyphs retain their state color and get a contrast edge over content.
    pub fn paint_icon(
        self,
        painter: &Painter,
        index: usize,
        icon: Icon,
        color: Color32,
        theme: Palette,
    ) {
        let rect = self.icon_rect(index);
        match self.presentation {
            BlisterPresentation::Collapsed => icons::paint(painter, rect, icon, color),
            BlisterPresentation::Overlay => {
                icons::paint_on_media(painter, rect, icon, color, theme)
            }
        }
    }

    pub fn icon_rect(self, index: usize) -> Rect {
        let count = self.count.max(1);
        let inset = match self.presentation {
            BlisterPresentation::Collapsed => {
                BLISTER_SHOULDER_RUN * self.band.height() / BLISTER_TAB_HEIGHT
            }
            BlisterPresentation::Overlay => 42.0 * self.band.height() / BLISTER_TAB_HEIGHT,
        }
        .min(self.band.width() * 0.24);
        let pitch = (self.band.width() - 2.0 * inset) / count as f32;
        let size = (self.band.height() * 0.78).min(pitch * 0.82);
        Rect::from_center_size(
            Pos2::new(
                self.band.left() + inset + pitch * (index as f32 + 0.5),
                self.band.center().y,
            ),
            Vec2::splat(size),
        )
    }
}

/// Compute collapsed or overlay blister geometry in the host's coordinate space.
pub fn blister_layout(
    host: Rect,
    zoom: f32,
    count: usize,
    presentation: BlisterPresentation,
) -> BlisterLayout {
    let zoom = zoom.max(f32::EPSILON);
    let (natural_width, y, cutout) = match presentation {
        BlisterPresentation::Collapsed => {
            let natural_width = (count as f32 * 48.0 + 2.0 * BLISTER_SHOULDER_RUN) * zoom;
            let band = Rect::from_center_size(
                Pos2::new(host.center().x, host.bottom()),
                Vec2::new(natural_width, BLISTER_TAB_HEIGHT * zoom),
            );
            let cutout = Rect::from_min_max(
                Pos2::new(band.left(), host.bottom() - 20.0 * zoom),
                Pos2::new(band.right(), host.bottom()),
            );
            return BlisterLayout {
                band,
                cutout: Some(cutout),
                count,
                presentation,
            };
        }
        BlisterPresentation::Overlay => {
            let natural_width = (count as f32 * 48.0 + 96.0) * zoom;
            (
                natural_width,
                host.bottom() - BLISTER_TAB_HEIGHT * zoom,
                None,
            )
        }
    };
    let size = Vec2::new(
        natural_width.min(host.width().max(0.0)),
        BLISTER_TAB_HEIGHT * zoom,
    );
    let band = Rect::from_min_size(Pos2::new(host.center().x - size.x * 0.5, y), size);
    BlisterLayout {
        band,
        cutout,
        count,
        presentation,
    }
}

/// Legacy overlay band geometry for PDF/capture popups and older shell callers.
pub fn blister_tab_rect(host: Rect, zoom: f32, count: usize) -> Rect {
    blister_layout(host, zoom, count, BlisterPresentation::Overlay).band
}

/// Image-overlapping chrome gives way to the external squircle strip when
/// its footprint exceeds a fifth of the host or its flat edge cannot fit it.
/// Inputs share one coordinate space; the decision is invariant under zoom.
pub fn blister_fits(host: Rect, radius: f32, zoom: f32, count: usize) -> bool {
    let full = blister_layout(host, zoom, count, BlisterPresentation::Collapsed);
    let natural_width = (count as f32 * 48.0 + 2.0 * BLISTER_SHOULDER_RUN) * zoom;
    let flat_width = (host.width() - 2.0 * radius.max(0.0)).max(0.0);
    host.is_positive() && natural_width <= flat_width && full.band.area() <= host.area() * 0.20
}

/// The raised tab needs an actual horizontal lower edge, including after
/// rotation or trimming. A diagonal bottom point falls back to squircles.
pub fn blister_fits_outline(outline: &[Pos2], zoom: f32, count: usize) -> bool {
    let bounds = Rect::from_points(outline);
    if outline.len() < 3 || !blister_fits(bounds, 0.0, zoom, count) {
        return false;
    }
    let band = blister_layout(bounds, zoom, count, BlisterPresentation::Collapsed).band;
    let epsilon = 0.05 * zoom;
    outline
        .iter()
        .zip(outline.iter().cycle().skip(1))
        .take(outline.len())
        .any(|(a, b)| {
            (a.y - bounds.bottom()).abs() <= epsilon
                && (b.y - bounds.bottom()).abs() <= epsilon
                && a.x.min(b.x) <= band.left() + epsilon
                && a.x.max(b.x) >= band.right() - epsilon
        })
}

pub fn blister_icon_rect(tab: Rect, index: usize, count: usize) -> Rect {
    BlisterLayout {
        band: tab,
        cutout: None,
        count,
        presentation: BlisterPresentation::Overlay,
    }
    .icon_rect(index)
}
