//! Transient intent curves: derived time never enters the scene journal.
use super::*;

pub(crate) struct FadingCurve {
    tab: u64,
    born: f64,
    opacity: f32,
    node: Node,
}

fn opacity_at(age: f64) -> f32 {
    if age <= 0.5 {
        1.0
    } else {
        ((1.0 - age) * 2.0).clamp(0.0, 1.0) as f32
    }
}

impl SlateApp {
    pub(crate) fn set_curve_fade_armed(&mut self, enabled: Option<bool>) -> bool {
        self.direct.fade_armed = enabled.unwrap_or(!self.direct.fade_armed);
        true
    }

    pub(crate) fn begin_curve_fade_creation(&mut self) {
        if self.direct.fade_armed {
            self.direct.fade_dirty_before = Some(self.tab().dirty);
        }
    }

    pub(crate) fn creates_fading_curves(&self, nodes: &[Node]) -> bool {
        self.direct.fade_armed && self.image_paint.is_none() && !nodes.is_empty() && nodes.iter().all(|n| {
            matches!(&n.kind, NodeKind::Shape(s) if matches!(s.shape, ShapeKind::Line | ShapeKind::Path) && !s.stroke.paints_as_stamp() && s.path.as_ref().is_none_or(|p| !p.closed))
        })
    }

    pub(crate) fn queue_fading_curves(&mut self, nodes: Vec<Node>) {
        let tab = self.tab().id;
        let born = self.frame_time;
        self.direct
            .fading
            .extend(nodes.into_iter().map(|node| FadingCurve {
                tab,
                born,
                opacity: node.opacity,
                node,
            }));
        if let Some(dirty) = self.direct.fade_dirty_before.take() {
            self.tab_mut().dirty = dirty;
        }
    }

    pub(crate) fn paint_fading_curves(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        let now = self.frame_time;
        let tab = self.tab().id;
        let mut fading = std::mem::take(&mut self.direct.fading);
        fading
            .retain(|curve| now - curve.born < 1.0 && self.tabs.iter().any(|t| t.id == curve.tab));
        for curve in &mut fading {
            if curve.tab != tab {
                continue;
            }
            curve.node.opacity = curve.opacity * opacity_at(now - curve.born);
            self.paint_board_node(ui, painter, xf, &curve.node, false);
        }
        if !fading.is_empty() {
            painter.ctx().request_repaint();
        }
        self.direct.fading = fading;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transient_curve_holds_for_half_second_then_fades_for_half_second() {
        assert_eq!(opacity_at(0.0), 1.0);
        assert_eq!(opacity_at(0.5), 1.0);
        assert_eq!(opacity_at(0.75), 0.5);
        assert_eq!(opacity_at(1.0), 0.0);
        assert_eq!(opacity_at(2.0), 0.0);
    }
}
