//! Shared capsule stack and row presentation.
use super::*;

pub(super) fn paint_capsule(ui: &egui::Ui, rect: Rect, zoom: f32, theme: Palette) {
    paint_capsule_shape(ui.painter(), rect, zoom, theme.panel, theme.border);
}

fn paint_capsule_shape(
    painter: &egui::Painter,
    rect: Rect,
    zoom: f32,
    fill: Color32,
    border: Color32,
) {
    painter.rect(
        rect,
        rect.height() * 0.5,
        fill,
        Stroke::new(0.8 * zoom, border),
        egui::StrokeKind::Inside,
    );
}

/// Width of one row of a capsule stack.
pub const STACK_WIDTH: f32 = 196.0;
/// Space between rows of a capsule stack.
pub const STACK_GAP: f32 = 5.0;
/// Space between a stack's anchor (a port circle) and its near edge.
pub const STACK_OFFSET: f32 = 18.0;
pub(super) const CAPSULE_LABEL: f32 = 9.0;
pub(super) const CAPSULE_TAG: f32 = 7.5;

/// Which way a capsule stack extends from its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackSide {
    Left,
    Right,
}

/// Rows of a capsule stack beside `anchor`, extending away from the host on
/// `side` and vertically centered on the anchor. Every length scales with
/// `zoom`, so the stack pans and zooms with its host.
pub fn capsule_stack_rects(
    anchor: Pos2,
    side: StackSide,
    count: usize,
    zoom: f32,
) -> impl ExactSizeIterator<Item = Rect> {
    let h = CORNER_HEIGHT * zoom;
    let gap = STACK_GAP * zoom;
    let w = STACK_WIDTH * zoom;
    let total = count as f32 * h + count.saturating_sub(1) as f32 * gap;
    let x = match side {
        StackSide::Right => anchor.x + STACK_OFFSET * zoom,
        StackSide::Left => anchor.x - STACK_OFFSET * zoom - w,
    };
    let top = anchor.y - total * 0.5;
    (0..count).map(move |i| {
        Rect::from_min_size(Pos2::new(x, top + i as f32 * (h + gap)), Vec2::new(w, h))
    })
}

/// One capsule row: a name, an optional trailing type chip and "vN" badge.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capsule<'a> {
    pub label: &'a str,
    pub chip: Option<&'a str>,
    pub badge: Option<&'a str>,
    /// Collected for a group action.
    pub selected: bool,
    /// Its object is on the canvas.
    pub spawned: bool,
    pub disabled: bool,
    /// Secondary row: quieter ink.
    pub dim: bool,
}

pub struct CapsuleResponse {
    pub response: egui::Response,
    /// The "vN" badge, when the capsule has one and is enabled.
    pub badge: Option<egui::Response>,
}

fn tag_rect(painter: &egui::Painter, right: f32, rect: Rect, text: &str, zoom: f32) -> Rect {
    let w = canvas_text::layout_no_wrap(
        painter,
        text.to_owned(),
        canvas_scale::font(CAPSULE_TAG, zoom),
        Color32::WHITE,
    )
    .size()
    .x;
    let h = rect.height() - 7.0 * zoom;
    Rect::from_min_max(
        Pos2::new(right - w - 8.0 * zoom, rect.center().y - h * 0.5),
        Pos2::new(right, rect.center().y + h * 0.5),
    )
}

/// Chip and badge rects inside a capsule, right to left.
pub(super) fn capsule_tags(
    painter: &egui::Painter,
    rect: Rect,
    capsule: &Capsule,
    zoom: f32,
) -> (Option<Rect>, Option<Rect>) {
    let mut right = rect.right() - 4.0 * zoom;
    let chip = capsule.chip.map(|c| {
        let r = tag_rect(painter, right, rect, c, zoom);
        right = r.left() - 3.0 * zoom;
        r
    });
    let badge = capsule
        .badge
        .map(|b| tag_rect(painter, right, rect, b, zoom));
    (chip, badge)
}

/// Paint one capsule row. Painter-only; [`capsule`] adds the pointer.
pub fn paint_capsule_row(
    painter: &egui::Painter,
    rect: Rect,
    capsule: &Capsule,
    hovered: bool,
    badge_hovered: bool,
    zoom: f32,
    theme: Palette,
) {
    let mut painter = painter.clone();
    if capsule.disabled {
        painter.multiply_opacity(0.45);
    }
    let (fill, border, ink) = if capsule.selected {
        (theme.select_fill, theme.select, theme.select_stroke)
    } else if hovered && !capsule.disabled {
        (theme.card_hover, theme.border_strong, theme.ink)
    } else {
        (
            theme.panel,
            theme.border,
            if capsule.dim { theme.sub } else { theme.ink },
        )
    };
    paint_capsule_shape(&painter, rect, zoom, fill, border);
    let dot = Pos2::new(rect.left() + 9.0 * zoom, rect.center().y);
    if capsule.spawned {
        painter.circle_filled(dot, 2.6 * zoom, theme.accent);
    }
    if !canvas_text::legible(canvas_text::authored_px(CAPSULE_TAG, zoom)) {
        return;
    }
    let (chip, badge) = capsule_tags(&painter, rect, capsule, zoom);
    for (tag, text, accent) in [(chip, capsule.chip, false), (badge, capsule.badge, true)] {
        let (Some(tag), Some(text)) = (tag, text) else {
            continue;
        };
        let lit = accent && badge_hovered && !capsule.disabled;
        painter.rect(
            tag,
            tag.height() * 0.5,
            if lit {
                theme.card_hover
            } else {
                Color32::TRANSPARENT
            },
            Stroke::new(
                0.8 * zoom,
                if accent {
                    theme.accent
                } else {
                    theme.border_strong
                },
            ),
            egui::StrokeKind::Inside,
        );
        canvas_text::text(
            &painter,
            tag.center(),
            Align2::CENTER_CENTER,
            text,
            canvas_scale::font(CAPSULE_TAG, zoom),
            if accent { theme.accent } else { theme.sub },
        );
    }
    let end = badge
        .or(chip)
        .map_or(rect.right() - 10.0 * zoom, |r| r.left() - 4.0 * zoom);
    let label = Rect::from_min_max(
        Pos2::new(rect.left() + 16.0 * zoom, rect.top()),
        Pos2::new(end, rect.bottom()),
    );
    canvas_text::text(
        &painter.with_clip_rect(label.intersect(painter.clip_rect())),
        label.left_center(),
        Align2::LEFT_CENTER,
        capsule.label,
        canvas_scale::font(CAPSULE_LABEL, zoom),
        ink,
    );
}

/// The shared canvas-scaled capsule row. Clicks and drags on the body report
/// through `response`; the "vN" badge has its own response.
pub fn capsule(
    ui: &egui::Ui,
    id: Id,
    rect: Rect,
    capsule: &Capsule,
    zoom: f32,
    theme: Palette,
) -> CapsuleResponse {
    let sense = if capsule.disabled {
        Sense::hover()
    } else {
        Sense::click_and_drag()
    };
    let response = ui.interact(rect, id, sense);
    let badge = if capsule.disabled {
        None
    } else {
        capsule_tags(ui.painter(), rect, capsule, zoom)
            .1
            .map(|r| ui.interact(r, id.with("badge"), Sense::click()))
    };
    let badge_hovered = badge.as_ref().is_some_and(|b| b.hovered());
    paint_capsule_row(
        &object_painter(ui),
        rect,
        capsule,
        response.hovered() && !badge_hovered,
        badge_hovered,
        zoom,
        theme,
    );
    CapsuleResponse { response, badge }
}

/// A section break inside a capsule stack: a hairline and a small label, not
/// a header block. Paints on the same layer as [`capsule`].
pub fn capsule_divider(ui: &egui::Ui, rect: Rect, label: &str, zoom: f32, theme: Palette) {
    paint_capsule_divider(&object_painter(ui), rect, label, zoom, theme);
}

/// [`capsule_divider`] on a caller's painter, beside [`paint_capsule_row`].
pub fn paint_capsule_divider(
    painter: &egui::Painter,
    rect: Rect,
    label: &str,
    zoom: f32,
    theme: Palette,
) {
    let y = rect.center().y;
    let mut x = rect.left() + 6.0 * zoom;
    if canvas_text::legible(canvas_text::authored_px(CAPSULE_TAG, zoom)) {
        x = canvas_text::text(
            painter,
            Pos2::new(x, y),
            Align2::LEFT_CENTER,
            label,
            canvas_scale::font(CAPSULE_TAG, zoom),
            theme.sub,
        )
        .right()
            + 6.0 * zoom;
    }
    painter.line_segment(
        [Pos2::new(x, y), Pos2::new(rect.right() - 6.0 * zoom, y)],
        Stroke::new(0.6 * zoom, theme.border),
    );
}
