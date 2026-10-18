use super::*;
use eframe::egui::Pos2;

pub(super) struct SmoothPreview {
    pub(super) before: Vec<Node>,
    pub(super) final_nodes: Vec<Node>,
    gesture: crate::app::board_smooth::SmoothGesture,
    zoom: f32,
}

impl SmoothPreview {
    pub(super) fn prepare(
        prepared: &PreparedBoardCommand,
        steps: &[(Option<Node>, Option<Node>)],
    ) -> Result<(Self, Vec<WorldPoint>), String> {
        let gesture = prepared
            .smooth_gesture()
            .ok_or("Smooth command is missing its captured gesture")?
            .clone();
        if steps.is_empty() || steps.len() > 64 {
            return Err("Smooth command has no bounded before/final node pairs".into());
        }
        let mut before = Vec::with_capacity(steps.len());
        let mut final_nodes = Vec::with_capacity(steps.len());
        for (source, result) in steps {
            let (Some(source), Some(result)) = (source, result) else {
                return Err("Smooth command requires complete before/final node pairs".into());
            };
            if source.id != result.id || before.iter().any(|node: &Node| node.id == source.id) {
                return Err("Smooth command contains mismatched or duplicate node pairs".into());
            }
            before.push(source.clone());
            final_nodes.push(result.clone());
        }
        let route = gesture
            .points
            .iter()
            .map(|point| {
                WorldPoint::new(f64::from(point.x), f64::from(point.y))
                    .map_err(|_| "Smooth route contains an invalid world point".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        WorkPath::new(vec![route.clone()], PathMode::Continuous)
            .map_err(|error| error.to_string())?;
        Ok((
            Self {
                before,
                final_nodes,
                gesture,
                zoom: prepared.camera_zoom(),
            },
            route,
        ))
    }

    pub(super) fn prefix(&self, path: &WorkPath, progress: f32) -> Option<Vec<Node>> {
        let at = path.position(progress);
        let route = path.strokes().first()?;
        let mut prefix: Vec<Pos2> = route
            .iter()
            .take(at.vertex_index + 1)
            .map(|point| Pos2::new(point.x as f32, point.y as f32))
            .collect();
        let tip = Pos2::new(at.position.x as f32, at.position.y as f32);
        if prefix.last().is_none_or(|point| *point != tip) {
            prefix.push(tip);
        }
        crate::app::board_smooth::prepare_smooth(
            &self.before,
            &prefix,
            self.gesture.width,
            self.gesture.strength,
            self.zoom,
        )
        .ok()
    }
}
