//! Bind the marker to actual paint inputs, rather than post-input state alone.
use super::*;

impl NativeBench {
    pub(super) fn arm_painted_response(&mut self) {
        let painted = crate::app::board::paint_probe::take();
        let Some((issued, phase, _, changed)) = self.waiting.as_ref() else {
            return;
        };
        if self.painted_sequence == self.sequence {
            return;
        }
        let ready = !changed
            || match *phase {
                "pan" | "zoom" if self.model.is_some() => self
                    .model
                    .and_then(|id| self.h.app.model3d.live.get(&id))
                    .is_some_and(|vp| {
                        vp.rendered
                            .zip(self.before_model)
                            .is_some_and(|(stamp, before)| {
                                stamp.0 == vp.cam.cache_hash() && stamp.0 != before.cache_hash()
                            })
                    }),
                "pan" => painted.is_some_and(|p| p.xf.offset != self.before.0),
                "zoom" => painted.is_some_and(|p| p.xf.z != self.before.1),
                "selection" => painted.is_some_and(|p| p.selected != self.before.2),
                _ => false,
            };
        if ready {
            *self.capture.as_ref().unwrap().pending.lock().unwrap() =
                Some((self.sequence, *issued, self.request_clock_100ns));
            self.painted_sequence = self.sequence;
        }
        // Do not force an extra paint to satisfy the witness. Normal egui /
        // app wake scheduling must produce the visible response itself.
    }
}
