//! Shared canvas-space layout and surface for image selection controls.
//!
//! Applications decide when an image editor is visible and supply its values.
//! This module owns where its technical datum and main controls sit.

use super::{FILTER_HEIGHT, canvas_scale};
use crate::theme::Palette;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

/// Height of the compact technical datum directly below an image.
pub const IMAGE_DATUM_HEIGHT: f32 = 28.0;
/// Gap between the datum and the main control field.
pub const IMAGE_PANEL_GAP: f32 = 6.0;
/// Horizontal and vertical content inset, in board units.
pub const IMAGE_PANEL_INSET: f32 = 12.0;
/// Responsive threshold in board units. The image's screen width is divided
/// by zoom before this comparison, so the layout does not jump while zooming.
pub const IMAGE_WIDE_THRESHOLD: f32 = 560.0;
const COLOR_FIELD_SIDE: f32 = 112.0;
const COLOR_RAIL_HEIGHT: f32 = 7.0;
const COLOR_RAIL_GAP: f32 = 9.0;
const COLOR_NARROW_GAP: f32 = 8.0;

/// Which shared image control group occupies the main field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImagePanelKind {
    /// Color field and three rails; Stroke adds its width rail.
    Color { stroke: bool },
    /// Six filter circles and intensity control.
    Filters,
    /// Layer circles, recent colors and opacity control.
    Layers,
    /// Shared corner editor height for Off/Crop and corner controls.
    Corners,
    /// Crop controls that use the shared image-panel silhouette.
    Crop,
    /// Brush controls that use the shared image-panel silhouette.
    Brush,
    /// A future or caller-defined panel using a board-unit main-field height.
    Custom { main_height: f32 },
}

impl ImagePanelKind {
    fn main_height(self) -> f32 {
        match self {
            Self::Color { stroke } => {
                let count = if stroke { 4.0 } else { 3.0 };
                2.0 * IMAGE_PANEL_INSET
                    + COLOR_FIELD_SIDE
                    + COLOR_NARROW_GAP
                    + count * COLOR_RAIL_HEIGHT
                    + (count - 1.0) * COLOR_RAIL_GAP
            }
            Self::Filters | Self::Layers => FILTER_HEIGHT,
            Self::Corners => super::CORNER_HEIGHT,
            Self::Crop | Self::Brush => FILTER_HEIGHT,
            Self::Custom { main_height } => main_height.max(0.0),
        }
    }
}

/// Geometry for the color field and its equal-width rails.
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
    let wide = match choice {
        ImageLayoutChoice::Responsive => image.width() / scale >= IMAGE_WIDE_THRESHOLD,
        ImageLayoutChoice::Narrow => false,
        ImageLayoutChoice::Wide => true,
    };
    let datum = IMAGE_DATUM_HEIGHT * scale;
    let gap = IMAGE_PANEL_GAP * scale;
    let main_height = kind.main_height() * scale;
    let technical = Rect::from_min_size(
        Pos2::new(image.left(), image.bottom()),
        Vec2::new(image.width(), datum),
    );
    let main = Rect::from_min_size(
        Pos2::new(image.left(), technical.bottom() + gap),
        Vec2::new(image.width(), main_height),
    );
    let tab_width = (72.0 * scale).min(image.width().max(0.0));
    let tab_height = 5.0 * scale;
    let tab = Rect::from_min_size(
        Pos2::new(
            image.center().x - tab_width * 0.5,
            image.bottom() - tab_height,
        ),
        Vec2::new(tab_width, tab_height * 2.0),
    );
    let technical_controls = technical_layout(technical);
    let color = match kind {
        ImagePanelKind::Color { stroke } => Some(color_layout(main, scale, wide, stroke)),
        _ => None,
    };
    ImagePresentationLayout {
        technical,
        main,
        overall: Rect::from_min_max(technical.min, main.max),
        tab,
        technical_controls,
        color,
        kind,
    }
}

fn technical_layout(technical: Rect) -> ImageTechnicalLayout {
    // The datum is arranged by fractions of the image width so every hit
    // target grows with the canvas object and no screen-space floor is used.
    let w = technical.width();
    let h = technical.height();
    let centered = |x: f32, width: f32| {
        Rect::from_center_size(
            Pos2::new(technical.left() + w * x, technical.center().y),
            Vec2::new(w * width, h * 0.78),
        )
    };
    let panel_buttons = std::array::from_fn(|i| centered(0.04 + i as f32 * 0.044, 0.032));
    let eyedropper = centered(0.37, 0.05);
    let recent_colors = std::array::from_fn(|i| centered(0.45 + i as f32 * 0.036, 0.022));
    let divider = Rect::from_center_size(
        Pos2::new(technical.left() + w * 0.69, technical.center().y),
        Vec2::new(canvas_scale::px(0.7, h / IMAGE_DATUM_HEIGHT), h * 0.55),
    );
    let rgb_values = std::array::from_fn(|i| centered(0.75 + i as f32 * 0.105, 0.075));
    ImageTechnicalLayout {
        panel_buttons,
        eyedropper,
        recent_colors,
        divider,
        rgb_values,
    }
}

fn color_layout(main: Rect, zoom: f32, wide: bool, stroke: bool) -> ImageColorLayout {
    let inset = canvas_scale::px(IMAGE_PANEL_INSET, zoom);
    let inner = main.shrink(inset);
    let count = if stroke { 4 } else { 3 };
    let empty = Rect::from_min_size(inner.min, Vec2::ZERO);
    let mut rails = [empty; 4];
    let field = if wide {
        let side = (inner.height()).min(inner.width() * 0.43);
        let field = Rect::from_min_size(inner.min, Vec2::splat(side));
        let rail_left = field.right() + canvas_scale::px(12.0, zoom);
        let rail_width = (inner.right() - rail_left).max(0.0);
        let available = inner.height();
        let pitch = available / count as f32;
        for (i, rail) in rails.iter_mut().enumerate().take(count) {
            *rail = Rect::from_min_size(
                Pos2::new(
                    rail_left,
                    inner.top() + (i as f32 + 0.5) * pitch - COLOR_RAIL_HEIGHT * zoom * 0.5,
                ),
                Vec2::new(rail_width, COLOR_RAIL_HEIGHT * zoom),
            );
        }
        field
    } else {
        let side = COLOR_FIELD_SIDE * zoom;
        let field = Rect::from_min_size(
            inner.min,
            Vec2::splat(side.min(inner.width()).min(inner.height())),
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

/// Paint the image-specific shared surface. Side edges have no outline, so
/// the panel visually continues the image's vertical edges; the low tab is
/// the only raised cue. Controls remain the caller's responsibility.
pub fn paint_image_blister(
    painter: &egui::Painter,
    layout: &ImagePresentationLayout,
    zoom: f32,
    theme: Palette,
) {
    let panel = layout.overall;
    painter.rect_filled(panel, 0.0, theme.panel);
    let edge = Stroke::new(canvas_scale::px(0.7, zoom), theme.border);
    painter.line_segment([panel.left_bottom(), panel.right_bottom()], edge);
    let radius = canvas_scale::px(2.0, zoom);
    painter.rect_filled(layout.tab, radius, theme.card_hover);
    painter.rect_stroke(
        layout.tab,
        radius,
        Stroke::new(canvas_scale::px(0.7, zoom), theme.border),
        egui::StrokeKind::Inside,
    );
    // A quiet inner highlight lifts the center without turning the tab into a
    // button or introducing a second icon container.
    painter.line_segment(
        [layout.tab.left_center(), layout.tab.right_center()],
        Stroke::new(canvas_scale::px(0.5, zoom), Color32::from_white_alpha(18)),
    );
}

/// Paint only the closed image palette's low tab. This keeps an image naked
/// until selection hover or a palette interaction asks for more affordance.
pub fn paint_image_tab(
    painter: &egui::Painter,
    layout: &ImagePresentationLayout,
    zoom: f32,
    theme: Palette,
    hovered: bool,
) {
    let radius = canvas_scale::px(2.0, zoom);
    painter.rect_filled(
        layout.tab,
        radius,
        if hovered {
            theme.card_hover
        } else {
            theme.panel
        },
    );
    painter.rect_stroke(
        layout.tab,
        radius,
        Stroke::new(
            canvas_scale::px(0.7, zoom),
            if hovered { theme.select } else { theme.border },
        ),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Rect {
        Rect::from_min_size(Pos2::new(80.0, 120.0), Vec2::new(640.0, 320.0))
    }

    #[test]
    fn image_datum_touches_image_and_main_field_starts_after_datum_gap() {
        let layout =
            image_presentation_layout(image(), 1.0, ImagePanelKind::Color { stroke: true });
        assert_eq!(layout.technical.top(), image().bottom());
        assert_eq!(
            layout.main.top() - layout.technical.bottom(),
            IMAGE_PANEL_GAP
        );
        assert_eq!(layout.overall.left(), image().left());
        assert_eq!(layout.overall.right(), image().right());
    }

    #[test]
    fn narrow_color_field_stacks_rails_below_the_gradient_without_overlap() {
        let layout = image_presentation_layout_with_choice(
            image(),
            1.0,
            ImagePanelKind::Color { stroke: true },
            ImageLayoutChoice::Narrow,
        );
        let color = layout.color.unwrap();
        assert!(!color.wide);
        assert_eq!(color.rail_count, 4);
        assert!(color.field.bottom() <= color.rails[0].top());
        assert!(color.rails[0].bottom() <= color.rails[1].top());
        assert!(color.rails[1].bottom() <= color.rails[2].top());
        assert!(color.rails[2].bottom() <= color.rails[3].top());
    }

    #[test]
    fn wide_color_field_and_equal_rails_share_the_same_vertical_span() {
        let layout = image_presentation_layout_with_choice(
            image(),
            1.0,
            ImagePanelKind::Color { stroke: true },
            ImageLayoutChoice::Wide,
        );
        let color = layout.color.unwrap();
        assert!(color.wide);
        assert_eq!(color.rails[0].height(), color.rails[1].height());
        assert_eq!(color.rails[1].height(), color.rails[2].height());
        assert_eq!(color.rails[2].height(), color.rails[3].height());
        assert!(color.field.right() <= color.rails[0].left());
        assert!(color.rails[0].top() >= layout.main.top());
        assert!(color.rails[3].bottom() <= layout.main.bottom());
    }

    #[test]
    fn zoom_scales_datum_and_panel_padding_in_canvas_space() {
        let a = image_presentation_layout(image(), 1.0, ImagePanelKind::Color { stroke: false });
        let b = image_presentation_layout(
            Rect::from_min_size(image().min * 2.0, image().size() * 2.0),
            2.0,
            ImagePanelKind::Color { stroke: false },
        );
        assert_eq!(b.technical.height(), a.technical.height() * 2.0);
        assert_eq!(
            b.color.unwrap().field.size(),
            a.color.unwrap().field.size() * 2.0
        );
    }
}
