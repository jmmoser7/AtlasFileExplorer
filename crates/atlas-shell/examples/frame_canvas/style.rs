//! Frame fixture style controls.
use super::*;

impl Specimen {
    pub(super) fn style_panel(
        &mut self,
        ui: &mut Ui,
        id: NodeId,
        kind: frame::Action,
        rect: Rect,
        z: f32,
        theme: Palette,
    ) {
        use atlas_shell::selection_tools as chrome;
        let before = self.scene.node(id).unwrap().clone();
        let mut after = before.clone();
        let NodeKind::Frame(data) = &mut after.kind else {
            return;
        };
        if kind == frame::Action::Corners {
            let (chamfer, percent, amount) = match data.corner {
                Corner::Square => (false, false, 0.0),
                Corner::Rounded { radius } => (false, false, radius),
                Corner::Chamfer { cut } => (true, false, cut),
                Corner::RoundedPercent { percent } => (false, true, percent),
                Corner::ChamferPercent { percent } => (true, true, percent),
            };
            let change = chrome::corner_editor(
                ui,
                rect,
                chamfer,
                percent,
                amount,
                if percent {
                    100.0
                } else {
                    before.rect.w.min(before.rect.h) / 2.0
                },
                None,
                z,
                theme,
            );
            if change.amount.is_some() || change.chamfer != chamfer || change.percent != percent {
                let v = change.amount.unwrap_or(amount);
                data.corner = match (change.chamfer, change.percent) {
                    (false, false) => Corner::Rounded { radius: v },
                    (true, false) => Corner::Chamfer { cut: v },
                    (false, true) => Corner::RoundedPercent { percent: v },
                    (true, true) => Corner::ChamferPercent { percent: v },
                };
            }
        } else {
            let (color, width) = if kind == frame::Action::Fill {
                (&mut data.fill, None)
            } else {
                (&mut data.stroke.color, Some(data.stroke.width))
            };
            let change = chrome::color_editor(
                ui,
                rect,
                color.0,
                width,
                false,
                &[],
                &mut self.colors,
                z,
                theme,
            );
            if let Some(rgb) = change.rgb {
                color.0[..3].copy_from_slice(&rgb);
            }
            if let Some(alpha) = change.alpha {
                color.0[3] = alpha;
            }
            if let Some(width) = change.width {
                data.stroke.width = width;
            }
            if kind == frame::Action::Fill && (change.rgb.is_some() || change.alpha.is_some()) {
                data.fill_authored = true;
            }
        }
        if before != after {
            self.journal.commit(
                &mut self.scene,
                vec![SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                }],
            );
        }
    }
}
