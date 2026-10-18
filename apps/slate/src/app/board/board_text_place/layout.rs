//! World text growth shared by human composition and prepared commands.
use super::*;

/// Text boxes retain their wrap width and grow down to include every row.
pub(crate) fn grow_text_rect(
    ctx: &egui::Context,
    mut rect: WorldRect,
    text: &str,
    font: FontId,
    wrap: f32,
    align: TextAlign,
    padding: f32,
) -> WorldRect {
    let laid = canvas_text::world_layout(ctx, text, font, wrap, egui_align(align));
    let height = laid.height + padding;
    if height.is_finite() {
        rect.h = rect.h.max(height);
    }
    rect
}

/// The active composition policy, retained across both growth passes in a frame.
pub(crate) struct TextGrowth {
    pub(crate) font: FontId,
    pub(crate) wrap: f32,
    pub(crate) align: TextAlign,
    padding: f32,
}
impl TextGrowth {
    pub(crate) fn grow(&self, ctx: &egui::Context, rect: WorldRect, text: &str) -> WorldRect {
        grow_text_rect(
            ctx,
            rect,
            text,
            self.font.clone(),
            self.wrap,
            self.align,
            self.padding,
        )
    }
}

/// Only free text grows; live property previews may supply its drawing style.
pub(crate) fn inline_text_layout(
    ctx: &egui::Context,
    node: &Node,
    live: Option<&TextNode>,
) -> Option<TextGrowth> {
    let NodeKind::Text(body) = &node.kind else {
        return None;
    };
    if body.fill.is_some() {
        return None;
    }
    let body = live.unwrap_or(body);
    Some(TextGrowth {
        font: styled_typeface_font(ctx, body.family, body.size, body.style),
        wrap: node.rect.w,
        align: body.align,
        padding: 4.0,
    })
}

pub(crate) fn document_text_font() -> FontId {
    FontId::monospace(TEXT_CARD_BODY_PX)
}

pub(crate) struct DocumentTextLayout {
    pub(crate) growth: TextGrowth,
    corner: slate_doc::scene::Corner,
}
impl DocumentTextLayout {
    pub(crate) fn grow(&self, ctx: &egui::Context, rect: WorldRect, text: &str) -> WorldRect {
        self.growth.grow(ctx, rect, text)
    }
    pub(crate) fn inner(&self, xf: &BoardXf, rect: WorldRect) -> Rect {
        text_card_inner(xf.rect_w2s(rect), self.corner, xf.z)
    }
}

/// Draft geometry/path are explicit: human source-backed cards need no override.
pub(crate) fn document_text_layout(
    rect: WorldRect,
    corner: slate_doc::scene::Corner,
    xf: &BoardXf,
    path: Option<&std::path::Path>,
) -> DocumentTextLayout {
    let corner = path.map_or(corner, |path| {
        slate_doc::media::text_card_corner(path, corner)
    });
    let inner = text_card_inner(xf.rect_w2s(rect), corner, xf.z);
    let wrap = inner.width() / xf.z.max(1.0e-4);
    DocumentTextLayout {
        growth: TextGrowth {
            font: document_text_font(),
            wrap,
            align: TextAlign::Left,
            padding: (rect.w - wrap).max(0.0),
        },
        corner,
    }
}
