//! Shared canvas-space layout and surface for image selection controls.
//!
//! Applications decide when an image editor is visible and supply its values.
//! This module owns where its technical datum and main controls sit.

use super::{canvas_scale, FILTER_HEIGHT};
use eframe::egui::{Pos2, Rect, Vec2};

/// Height of a single-row color datum directly below an image.
pub const IMAGE_DATUM_HEIGHT: f32 = 28.0;
const IMAGE_DATUM_NARROW_HEIGHT: f32 = 46.0;
const IMAGE_DATUM_GROUP_WIDTH: f32 = 330.0;
/// Gap between the datum and the main control field.
pub const IMAGE_PANEL_GAP: f32 = 6.0;
/// Horizontal and vertical content inset, in board units.
pub const IMAGE_PANEL_INSET: f32 = 12.0;
/// Minimum board-space width required for the wide image color layout.
pub const IMAGE_WIDE_THRESHOLD: f32 = 560.0;
const COLOR_FIELD_WIDE_ASPECT: f32 = 5.0;
const COLOR_FIELD_STACKED_ASPECT: f32 = 6.0;
const COLOR_RAIL_HEIGHT: f32 = 7.0;
const COLOR_RAIL_GAP: f32 = 9.0;
const COLOR_NARROW_GAP: f32 = 8.0;
const COLOR_COLUMN_GAP: f32 = 12.0;

/// Which shared image control group occupies the main field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImagePanelKind {
    /// Color field and three rails; Stroke adds its width rail.
    Color { stroke: bool },
    /// Six filter circles and intensity control.
    Filters,
    /// Layer circles and opacity control.
    Layers,
    /// Shared corner editor height for Off/Crop and corner controls.
    Corners,
    /// Crop controls that use the shared image-panel silhouette.
    Crop,
    /// Brush parameter strip, color datum, and three-rail color body.
    Brush,
    /// A future or caller-defined panel using a board-unit main-field height.
    Custom { main_height: f32 },
}

impl ImagePanelKind {
    fn main_height(self, width_units: f32, wide: bool) -> f32 {
        match self {
            Self::Color { stroke } => {
                color_body_height(width_units, wide, if stroke { 4 } else { 3 })
            }
            Self::Filters => FILTER_HEIGHT,
            Self::Layers => super::image_layer_layout::image_layer_palette_height(
                (width_units - 2.0 * IMAGE_PANEL_INSET).max(0.0),
            ),
            Self::Corners => super::CORNER_HEIGHT + 2.0 * IMAGE_PANEL_INSET,
            Self::Crop => FILTER_HEIGHT,
            Self::Brush => color_body_height(width_units, wide, 3),
            Self::Custom { main_height } => main_height.max(0.0),
        }
    }
}

fn color_body_height(width_units: f32, wide: bool, rail_count: usize) -> f32 {
    let inner_width = (width_units - 2.0 * IMAGE_PANEL_INSET).max(0.0);
    let field_height = if wide {
        ((inner_width - COLOR_COLUMN_GAP).max(0.0) * 0.5) / COLOR_FIELD_WIDE_ASPECT
    } else {
        inner_width / COLOR_FIELD_STACKED_ASPECT
    };
    let rails_height = rail_count as f32 * COLOR_RAIL_HEIGHT
        + rail_count.saturating_sub(1) as f32 * COLOR_RAIL_GAP;
    if wide {
        2.0 * IMAGE_PANEL_INSET + field_height
    } else {
        2.0 * IMAGE_PANEL_INSET + field_height + COLOR_NARROW_GAP + rails_height
    }
}

/// Geometry for the color field and its companion rail column or row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageColorLayout {
    pub field: Rect,
    /// Opacity, saturation, hue, then optional stroke width.
    pub rails: [Rect; 4],
    pub rail_count: usize,
    pub wide: bool,
}

/// Regions in the technical datum for color sampling, history, and channels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageTechnicalLayout {
    /// Bare glyph hit regions for Fill, Stroke, Corners, Filters, Brush,
    /// Layers, and Agent, in that order.
    pub panel_buttons: [Rect; 7],
    pub eyedropper: Rect,
    pub recent_colors: [Rect; 6],
    pub divider: Rect,
    pub rgb_values: [Rect; 3],
}

/// Geometry shared by the image technical datum, main field, and blister.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImagePresentationLayout {
    /// The image edge is exactly this rect's top edge.
    pub technical: Rect,
    /// Brush's parameter strip immediately below the image, when present.
    pub brush_header: Option<Rect>,
    /// Layer previews occupy their own top datum, above the opacity rail.
    pub layer_header: Option<Rect>,
    /// Main editor below the technical datum.
    pub main: Rect,
    /// Combined surface, spanning the image's full width.
    pub overall: Rect,
    /// Small raised center tab that overlaps the image edge.
    pub tab: Rect,
    /// Technical-datum controls shared by color and stroke variants.
    pub technical_controls: ImageTechnicalLayout,
    /// Present for color panels, allowing the caller to reuse shared color UI.
    pub color: Option<ImageColorLayout>,
    pub kind: ImagePanelKind,
}

impl ImagePresentationLayout {
    /// Preserve the shared corner editor inside the image apron's content inset.
    pub fn editor_rect(&self, zoom: f32) -> Rect {
        if self.kind == ImagePanelKind::Corners {
            self.main.shrink(IMAGE_PANEL_INSET * zoom.max(f32::EPSILON))
        } else {
            self.main
        }
    }
}

/// Layout choice for responsive policy and deterministic geometry checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageLayoutChoice {
    #[default]
    Responsive,
    Narrow,
    Wide,
}

/// Compute image-control geometry using responsive width derived from the
/// image's board-space width.
pub fn image_presentation_layout(
    image: Rect,
    zoom: f32,
    kind: ImagePanelKind,
) -> ImagePresentationLayout {
    image_presentation_layout_with_choice(image, zoom, kind, ImageLayoutChoice::Responsive)
}

/// Explicit layout choice is useful to keep narrow and wide arrangements
/// covered by headless geometry checks and by narrow hosting contexts.
pub fn image_presentation_layout_with_choice(
    image: Rect,
    zoom: f32,
    kind: ImagePanelKind,
    choice: ImageLayoutChoice,
) -> ImagePresentationLayout {
    let scale = zoom.max(f32::EPSILON);
    let width_units = image.width() / scale;
    let aspect = image.width() / image.height().max(f32::EPSILON);
    let wide = match choice {
        ImageLayoutChoice::Responsive => width_units >= IMAGE_WIDE_THRESHOLD && aspect >= 1.4,
        ImageLayoutChoice::Narrow => false,
        ImageLayoutChoice::Wide => true,
    };
    let narrow_datum = width_units < IMAGE_DATUM_GROUP_WIDTH + 24.0;
    let has_color_datum = matches!(kind, ImagePanelKind::Color { .. } | ImagePanelKind::Brush);
    let datum = if kind == ImagePanelKind::Layers {
        64.0 * scale
    } else if kind == ImagePanelKind::Brush {
        IMAGE_DATUM_HEIGHT * scale
    } else if !has_color_datum {
        0.0
    } else if narrow_datum {
        IMAGE_DATUM_NARROW_HEIGHT * scale
    } else {
        IMAGE_DATUM_HEIGHT * scale
    };
    let gap = if has_color_datum {
        IMAGE_PANEL_GAP * scale
    } else {
        0.0
    };
    let main_height = kind.main_height(width_units, wide) * scale;
    let technical = Rect::from_min_size(image.left_bottom(), Vec2::new(image.width(), datum));
    let brush_header = (kind == ImagePanelKind::Brush).then(|| {
        Rect::from_min_size(
            technical.min + Vec2::new(IMAGE_PANEL_INSET * scale, 0.0),
            Vec2::new(
                super::image_brush::image_brush_header_width(width_units) * scale,
                datum,
            ),
        )
    });
    let layer_header = (kind == ImagePanelKind::Layers).then_some(technical);
    let main = Rect::from_min_size(
        Pos2::new(image.left(), technical.bottom() + gap),
        Vec2::new(image.width(), main_height),
    );
    let tab = blister_tab_rect(image, scale, 7);
    let color_datum = brush_header.map_or(technical, |header| {
        Rect::from_min_max(
            Pos2::new(header.right() + 6.0 * scale, technical.top()),
            technical.max,
        )
    });
    let technical_controls = technical_layout(
        color_datum,
        tab,
        narrow_datum && brush_header.is_none(),
        brush_header.is_some(),
        scale,
    );
    let color = match kind {
        ImagePanelKind::Color { stroke } => Some(color_layout(main, scale, wide, stroke)),
        ImagePanelKind::Brush => Some(color_layout(main, scale, wide, false)),
        _ => None,
    };
    ImagePresentationLayout {
        technical,
        brush_header,
        layer_header,
        main,
        overall: Rect::from_min_max(technical.min, main.max),
        tab,
        technical_controls,
        color,
        kind,
    }
}

fn technical_layout(
    technical: Rect,
    tab: Rect,
    narrow: bool,
    brush: bool,
    unit: f32,
) -> ImageTechnicalLayout {
    if brush {
        return super::image_brush::image_brush_color_datum(technical, tab, unit);
    }
    let w = technical.width();
    let h = technical.height();
    let group_width = (IMAGE_DATUM_GROUP_WIDTH * unit).min((w - 24.0 * unit).max(0.0));
    let group_left = technical.center().x - group_width * 0.5;
    let group_center_y = if narrow {
        technical.top() + 14.0 * unit
    } else {
        technical.center().y
    };
    let second_row_y = technical.top() + 35.0 * unit;
    let panel_buttons = std::array::from_fn(|i| blister_icon_rect(tab, i, 7));
    let eye_width = 24.0 * unit;
    let eyedropper = Rect::from_center_size(
        Pos2::new(group_left + eye_width * 0.5, group_center_y),
        Vec2::new(eye_width, h * if narrow { 0.36 } else { 0.78 }),
    );
    let recent_colors = if narrow {
        let available = group_width.max(0.0);
        let diameter = (18.0 * unit).min(((available - 43.0 * unit) / 6.0).max(0.0));
        let pitch = diameter + 3.0 * unit;
        std::array::from_fn(|i| {
            Rect::from_center_size(
                Pos2::new(
                    group_left + 28.0 * unit + diameter * 0.5 + i as f32 * pitch,
                    group_center_y,
                ),
                Vec2::splat(diameter),
            )
        })
    } else {
        std::array::from_fn(|i| {
            Rect::from_center_size(
                Pos2::new(group_left + (41.0 + i as f32 * 26.0) * unit, group_center_y),
                Vec2::splat(22.0 * unit),
            )
        })
    };
    let divider_x = group_left + 196.0 * unit;
    let divider = if narrow {
        Rect::from_min_size(Pos2::ZERO, Vec2::ZERO)
    } else {
        Rect::from_center_size(
            Pos2::new(divider_x, group_center_y),
            Vec2::new(canvas_scale::px(0.7, unit), h * 0.55),
        )
    };
    let rgb_width = if narrow {
        (40.0 * unit).min(((group_width - 12.0 * unit) / 3.0).max(0.0))
    } else {
        40.0 * unit
    };
    let rgb_gap = if narrow { 4.0 * unit } else { 2.0 * unit };
    let rgb_total = 3.0 * rgb_width + 2.0 * rgb_gap;
    let rgb_left = if narrow {
        group_left + (group_width - rgb_total) * 0.5
    } else {
        group_left + 202.0 * unit
    };
    let rgb_y = if narrow { second_row_y } else { group_center_y };
    let rgb_values = std::array::from_fn(|i| {
        Rect::from_center_size(
            Pos2::new(
                rgb_left + rgb_width * 0.5 + i as f32 * (rgb_width + rgb_gap),
                rgb_y,
            ),
            Vec2::new(rgb_width, h * if narrow { 0.36 } else { 0.78 }),
        )
    });
    ImageTechnicalLayout {
        panel_buttons,
        eyedropper,
        recent_colors,
        divider,
        rgb_values,
    }
}

/// Shared shallow tab geometry; callers supply their eligible property count.
pub fn blister_tab_rect(host: Rect, zoom: f32, count: usize) -> Rect {
    let size = Vec2::new(
        ((count as f32 * 40.0 + 8.0) * zoom).min(host.width().max(0.0)),
        28.0 * zoom,
    );
    Rect::from_min_size(
        Pos2::new(host.center().x - size.x * 0.5, host.bottom() - size.y),
        size,
    )
}

pub fn blister_icon_rect(tab: Rect, index: usize, count: usize) -> Rect {
    let inset = (tab.height() * 0.55).min(tab.width() * 0.06);
    let pitch = (tab.width() - 2.0 * inset) / count.max(1) as f32;
    let size = (tab.height() * 0.78).min(pitch * 0.82);
    Rect::from_center_size(
        Pos2::new(
            tab.left() + inset + pitch * (index as f32 + 0.5),
            tab.center().y,
        ),
        Vec2::splat(size),
    )
}

fn color_layout(main: Rect, zoom: f32, wide: bool, stroke: bool) -> ImageColorLayout {
    let inset = canvas_scale::px(IMAGE_PANEL_INSET, zoom);
    let inner = main.shrink(inset);
    let count = if stroke { 4 } else { 3 };
    let empty = Rect::from_min_size(inner.min, Vec2::ZERO);
    let mut rails = [empty; 4];
    let field = if wide {
        let column_width =
            (inner.width() - canvas_scale::px(COLOR_COLUMN_GAP, zoom)).max(0.0) * 0.5;
        let field_height = (column_width / COLOR_FIELD_WIDE_ASPECT).min(inner.height());
        let field = Rect::from_min_max(
            Pos2::new(
                inner.right() - column_width,
                inner.center().y - field_height * 0.5,
            ),
            Pos2::new(inner.right(), inner.center().y + field_height * 0.5),
        );
        let available = inner.height();
        let pitch = available / count as f32;
        for (i, rail) in rails.iter_mut().enumerate().take(count) {
            *rail = Rect::from_min_size(
                Pos2::new(
                    inner.left(),
                    inner.top() + (i as f32 + 0.5) * pitch - COLOR_RAIL_HEIGHT * zoom * 0.5,
                ),
                Vec2::new(column_width, COLOR_RAIL_HEIGHT * zoom),
            );
        }
        field
    } else {
        let field_height = inner.width() / COLOR_FIELD_STACKED_ASPECT;
        let field = Rect::from_min_size(
            inner.min,
            Vec2::new(inner.width(), field_height.min(inner.height())),
        );
        let top = field.bottom() + COLOR_NARROW_GAP * zoom;
        let rail_width = inner.width();
        for (i, rail) in rails.iter_mut().enumerate().take(count) {
            *rail = Rect::from_min_size(
                Pos2::new(
                    inner.left(),
                    top + i as f32 * (COLOR_RAIL_HEIGHT + COLOR_RAIL_GAP) * zoom,
                ),
                Vec2::new(rail_width, COLOR_RAIL_HEIGHT * zoom),
            );
        }
        field
    };
    ImageColorLayout {
        field,
        rails,
        rail_count: count,
        wide,
    }
}

#[cfg(test)]
#[path = "image_presentation_tests.rs"]
mod tests;
