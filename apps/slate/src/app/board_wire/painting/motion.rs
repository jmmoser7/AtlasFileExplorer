//! One-pixel coverage proxy for small plain wires while the camera moves.
use super::*;

impl WireStrokeBatch {
    pub(super) fn append_subpixel_chord<const TRACE: bool>(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        path: &ConnectorPath,
        coverage: f32,
        color: Color32,
    ) -> bool {
        let [ax, ay] = path.start();
        let [bx, by] = path.end();
        let a = xf.w2s(Pos2::new(ax, ay));
        let b = xf.w2s(Pos2::new(bx, by));
        let delta = b - a;
        if delta.length_sq() <= f32::EPSILON {
            return false;
        }
        let normal =
            Vec2::new(-delta.y, delta.x).normalized() * (0.5 / painter.ctx().pixels_per_point());
        if self.mesh.vertices.len() + 4 > WIRE_BATCH_VERTICES {
            self.flush::<TRACE>(painter);
        }
        let first = self.mesh.vertices.len() as u32;
        let color = color.gamma_multiply(coverage);
        for vertex in [a - normal, a + normal, b + normal, b - normal] {
            self.mesh.colored_vertex(vertex, color);
        }
        self.mesh.add_triangle(first, first + 1, first + 2);
        self.mesh.add_triangle(first, first + 2, first + 3);
        true
    }
}
