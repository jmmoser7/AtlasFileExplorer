//! Sticky-note fit and the image-draft painter.

use super::*;

impl SlateApp {
    /// The pool item behind an image node, if any.
    pub(crate) fn image_item(&self, id: NodeId) -> Option<ItemId> {
        match self.doc().scene.node(id).map(|n| &n.kind) {
            Some(NodeKind::Image(img)) => Some(img.item),
            _ => None,
        }
    }

    pub(super) fn paint_hosted_text(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        text: Option<&slate_doc::scene::ShapeText>,
        srect: Rect,
        fade: &impl Fn(Color32) -> Color32,
    ) {
        if self
            .text_edit
            .as_ref()
            .is_some_and(|(edit_id, _)| *edit_id == node.id)
        {
            return;
        }
        let Some(text) = text.filter(|text| !text.body.is_empty()) else {
            return;
        };
        let z = xf.z;
        if !canvas_text::legible(canvas_text::authored_px(text.size, z)) {
            return;
        }
        // 8 world units: the artifact's `padding:8px` on shape text.
        let inset = 8.0;
        let galley = zoom_text_galley(
            painter.ctx(),
            &text.body,
            typeface_font(text.family, text.size),
            (node.rect.w - inset * 2.0).max(0.0),
            text.align,
            fade(rgba32(text.color)),
            z,
        );
        let pos = Pos2::new(
            srect.left() + canvas_scale::px(inset, z),
            srect.center().y - galley.size().y * 0.5,
        );
        let pos = rotate_points(&[pos], srect.center(), node.rotation_deg)[0];
        let mut shape = egui::epaint::TextShape::new(pos, galley, Color32::WHITE);
        shape.angle = node.rotation_deg.to_radians();
        painter.add(shape);
    }

    /// Authored size is the ceiling. A long note shrinks until the block fits.
    /// The chosen size is in world units and does not depend on the camera.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sticky_font_size(
        &mut self,
        ctx: &egui::Context,
        id: NodeId,
        text: &str,
        family: Typeface,
        max_size: f32,
        box_w: f32,
        box_h: f32,
        align: TextAlign,
    ) -> f32 {
        if let Some(hit) = self.sticky_fit_hit(id, text, box_w, box_h, max_size, family, align) {
            return hit;
        }
        let fitted = measure_sticky_font(ctx, text, family, max_size, box_w, box_h, align);
        self.store_sticky_fit(id, text, box_w, box_h, max_size, family, align, fitted);
        fitted
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn sticky_fit_hit(
        &self,
        id: NodeId,
        text: &str,
        box_w: f32,
        box_h: f32,
        max_size: f32,
        family: Typeface,
        align: TextAlign,
    ) -> Option<f32> {
        let hit = self.sticky_fit.get(&id)?;
        (hit.text == text
            && (hit.w - box_w).abs() < 0.5
            && (hit.h - box_h).abs() < 0.5
            && (hit.max - max_size).abs() < 0.05
            && hit.family == family
            && hit.align == align)
            .then_some(hit.fitted)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn store_sticky_fit(
        &mut self,
        id: NodeId,
        text: &str,
        box_w: f32,
        box_h: f32,
        max_size: f32,
        family: Typeface,
        align: TextAlign,
        fitted: f32,
    ) {
        self.sticky_fit.insert(
            id,
            StickyFit {
                text: text.to_string(),
                w: box_w,
                h: box_h,
                max: max_size,
                family,
                align,
                fitted,
            },
        );
    }

    /// Soft drop under a sticky. Same offsets as the artifact's `box-shadow`.
    pub(super) fn paint_sticky_shadow(
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        srect: Rect,
        z: f32,
        fade: &impl Fn(Color32) -> Color32,
    ) {
        let dy = canvas_scale::px(slate_doc::scene::STICKY_SHADOW_OFFSET_Y, z);
        let blur = canvas_scale::px(slate_doc::scene::STICKY_SHADOW_BLUR, z);
        let alpha = (slate_doc::scene::STICKY_SHADOW_ALPHA * 255.0).round() as u8;
        let color = fade(Color32::from_black_alpha(alpha));
        if node.rotation_deg.abs() <= 0.01 {
            let shadow = egui::epaint::Shadow {
                offset: [0, dy.round().clamp(-128.0, 127.0) as i8],
                blur: blur.round().clamp(0.0, 255.0) as u8,
                spread: 0,
                color,
            };
            painter.add(shadow.as_shape(srect, 0.0));
            return;
        }
        let shifted = node
            .rect
            .translated(0.0, slate_doc::scene::STICKY_SHADOW_OFFSET_Y);
        let pts: Vec<Pos2> = shifted
            .corners_rotated(node.rotation_deg)
            .into_iter()
            .map(|(x, y)| xf.w2s(Pos2::new(x, y)))
            .collect();
        painter.add(egui::Shape::convex_polygon(pts, color, EStroke::NONE));
    }

    /// Clip in-progress ink to the hosted image outline (D09).
    pub(crate) fn image_paint_draft_painter(
        &self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) -> egui::Painter {
        let Some(session) = self.image_paint.as_ref() else {
            return painter.clone();
        };
        let Some(host) = self.doc().scene.node(session.image) else {
            return painter.clone();
        };
        let outline = self.node_screen_outline(ctx, xf, host);
        if outline.len() < 3 {
            return painter.clone();
        }
        let min_x = outline.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let min_y = outline.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_x = outline
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = outline
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        painter.with_clip_rect(
            egui::Rect::from_min_max(Pos2::new(min_x, min_y), Pos2::new(max_x, max_y))
                .intersect(painter.clip_rect()),
        )
    }
}
