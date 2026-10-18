//! Shared native-fixture screenshot lifecycle; callers supply their settle gate.
use eframe::egui;

#[derive(Default)]
pub(super) struct Capture {
    prefix: Option<String>,
    frames: usize,
    requested: bool,
}

impl Capture {
    pub(super) fn from_args(args: &[String]) -> Self {
        Self {
            prefix: args
                .iter()
                .position(|arg| arg == "--capture")
                .and_then(|index| args.get(index + 1))
                .cloned(),
            ..Default::default()
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.prefix.is_some()
    }

    /// Preview passes true: it still requests exactly frame eight of each theme.
    /// A light-starting fixture captures once; dark starts advance through light.
    pub(super) fn update(&mut self, ctx: &egui::Context, dark: &mut bool, ready: bool) {
        let Some(prefix) = &self.prefix else {
            return;
        };
        self.frames += 1;
        if self.frames >= 8 && ready && !self.requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.requested = true;
        }
        let shot = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let pixels = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
                .unwrap()
                .save(format!(
                    "{prefix}-{}.png",
                    if *dark { "dark" } else { "light" }
                ))
                .unwrap();
            if *dark {
                *dark = false;
                self.frames = 0;
                self.requested = false;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        ctx.request_repaint();
    }
}
