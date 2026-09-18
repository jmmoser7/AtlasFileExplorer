//! Visual regression fixture for the actual shared renderer (no source-file I/O).
//! `cargo run -p atlas-shell --example cover_flow -- --capture C:/temp/flow`
//! writes flow-dark.png and flow-light.png, then exits. Omit --capture to browse.
use atlas_shell::home::{
    cover_flow_home, HomeArtwork, HomeCover, HomeCta, HomeModel, COVER_TEXTURE_OPTIONS,
};
use atlas_shell::theme::Palette;
use eframe::egui::{self, Color32, FontFamily, Id, TextureHandle};
use std::path::PathBuf;

struct Preview {
    covers: Vec<HomeCover>,
    _textures: Vec<TextureHandle>,
    focus: usize,
    dark: bool,
    capture: Option<String>,
    frames: usize,
}

impl Preview {
    fn new(ctx: &egui::Context, capture: Option<String>) -> Self {
        let mut textures = Vec::new();
        let covers = [
            "Field notes",
            "Structure",
            "Colour study",
            "Workbooks",
            "Studio",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, title)| {
            let texture = if i == 4 {
                None
            } else {
                let mask = i == 1 || i == 3;
                let pixels = (0..512 * 512)
                    .map(|p| {
                        let (x, y) = (p % 512, p / 512);
                        if mask {
                            let card = x > 85 && x < 427 && y > 145 && y < 367;
                            let edge =
                                card && (!(90..=422).contains(&x) || !(150..=362).contains(&y));
                            Color32::from_white_alpha(if edge {
                                85
                            } else if card {
                                22
                            } else {
                                0
                            })
                        } else {
                            let circle = (x as f32 - 256.0).hypot(y as f32 - 240.0) < 165.0;
                            if circle {
                                Color32::from_rgb(230, (95 + y / 5) as u8, 65)
                            } else if (x + y / 3) % 56 < 4 {
                                Color32::from_rgb(204, 225, 217)
                            } else {
                                Color32::from_rgb(31, (75 + y / 10) as u8, (95 + x / 8) as u8)
                            }
                        }
                    })
                    .collect();
                let tex = ctx.load_texture(
                    title,
                    egui::ColorImage {
                        size: [512, 512],
                        pixels,
                    },
                    COVER_TEXTURE_OPTIONS,
                );
                let artwork = HomeArtwork {
                    texture: tex.id(),
                    theme_mask: mask,
                };
                textures.push(tex);
                Some(artwork)
            };
            HomeCover {
                path: PathBuf::from(title),
                title: title.into(),
                texture,
                placeholder: false,
            }
        })
        .collect();
        Self {
            covers,
            _textures: textures,
            focus: 2,
            dark: true,
            capture,
            frames: 0,
        }
    }
}

impl eframe::App for Preview {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let palette = Palette::for_mode(self.dark);
        ctx.set_visuals(palette.visuals());
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                let result = cover_flow_home(
                    ui,
                    &palette,
                    HomeModel {
                        id: Id::new("cover-flow-preview"),
                        new_label: "New",
                        covers: &self.covers,
                        focus: self.focus,
                        backdrop: true,
                        honor_cover_limits: true,
                        cta: HomeCta::Bottom,
                        host: None,
                        interactive: true,
                        title_font: FontFamily::Proportional,
                    },
                );
                self.focus = result.focus;
            });
        if ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.dark = !self.dark;
        }
        if let Some(prefix) = &self.capture {
            self.frames += 1;
            if self.frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                let output =
                    image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, bytes)
                        .unwrap();
                output
                    .save(format!(
                        "{prefix}-{}.png",
                        if self.dark { "dark" } else { "light" }
                    ))
                    .unwrap();
                if self.dark {
                    self.dark = false;
                    self.frames = 0;
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    let capture = args
        .iter()
        .position(|a| a == "--capture")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let height = if args.iter().any(|a| a == "--short") {
        520.0
    } else {
        780.0
    };
    eframe::run_native(
        "Cover Flow renderer check",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, height]),
            multisampling: atlas_shell::NATIVE_MSAA_SAMPLES,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(Preview::new(&cc.egui_ctx, capture)))),
    )
}
