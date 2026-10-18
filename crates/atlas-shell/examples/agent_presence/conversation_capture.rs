//! Screenshot export for the native conversation component fixture.
use super::Preview;
use eframe::egui;
impl Preview {
    pub(super) fn capture(&mut self, ctx: &egui::Context) {
        let Some(prefix) = &self.capture else {
            return;
        };
        self.frames += 1;
        if self.frames == 12 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let pixels = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
                .unwrap()
                .save(format!("{prefix}-conversation.png"))
                .unwrap();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
