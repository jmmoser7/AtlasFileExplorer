//! Capture uses the source's transform, including a fitted maximized assembly.
use super::*;
use atlas_shell::selection_tools::capture;

impl SlateApp {
    /// Source and controls share one transform. A maximized fit is derived
    /// view state; it never moves the board camera.
    pub(crate) fn portal_capture_maximized_host(
        &self,
        id: NodeId,
        screen: Rect,
    ) -> Option<(Rect, f32)> {
        let node = self.doc().scene.node(id)?;
        let NodeKind::Portal(portal) = &node.kind else {
            return None;
        };
        let height = if self.portal_capture_status(id).is_some() {
            // Keep source scale stable through Recording, Pause and Saving.
            capture::active_height(capture::Phase::Paused, node.rect.w)
        } else if self.portal_capture_launch_source() == Some(id) {
            capture::launch_height(self.portal_capture_can_video(id), node.rect.w)
        } else {
            let host = maximized_host_rect(portal.kind, node.rect, screen);
            return Some((host, fit_xf(node.rect, host).z));
        };
        let assembly = WorldRect::new(node.rect.x, node.rect.y, node.rect.w, node.rect.h + height);
        let xf = fit_xf(assembly, screen);
        Some((xf.rect_w2s(node.rect), xf.z))
    }

    pub(crate) fn portal_capture_source_geometry(&self, id: NodeId) -> Option<(Rect, f32)> {
        let node = self.doc().scene.node(id)?;
        if self
            .portal_chrome
            .maximized
            .is_some_and(|maximized| maximized != id)
        {
            // This source is not painted in another portal's maximized view.
            return None;
        }
        if self.portal_chrome.maximized == Some(id) {
            self.portal_capture_maximized_host(id, self.canvas_rect)
        } else {
            let xf = self.board_xf();
            Some((
                xf.rect_w2s(node.rect.rotated_bounds(node.rotation_deg)),
                xf.z,
            ))
        }
    }

    #[cfg(test)]
    pub(crate) fn portal_capture_source_rect(&self, id: NodeId) -> Option<Rect> {
        self.portal_capture_source_geometry(id)
            .map(|(rect, _)| rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximized_assembly_fit_scales_source_and_apron_together() {
        let world = WorldRect::new(20.0, -40.0, 320.0, 200.0);
        let height = capture::active_height(capture::Phase::Paused, world.w);
        let assembly = WorldRect::new(world.x, world.y, world.w, world.h + height);
        for width in [320.0, 800.0] {
            let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0));
            let xf = fit_xf(assembly, screen);
            let source = xf.rect_w2s(world);
            let apron = atlas_shell::selection_tools::image_presentation_layout(
                source,
                xf.z,
                atlas_shell::selection_tools::ImagePanelKind::Custom {
                    main_height: height,
                },
            );
            assert!((apron.overall.left() - source.left()).abs() < 0.001);
            assert!((apron.overall.right() - source.right()).abs() < 0.001);
            assert!((apron.overall.top() - source.bottom()).abs() < 0.001);
            assert!(screen
                .expand(0.001)
                .contains_rect(source.union(apron.overall)));
            assert!((apron.overall.height() / xf.z - height).abs() < 0.001);
        }
    }
}
