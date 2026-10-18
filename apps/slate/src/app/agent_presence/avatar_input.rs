//! Gesture/camera adapter and one asynchronously decoded local profile picture.
use super::*;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct ProfilePhoto {
    path: Option<PathBuf>,
    initialized: bool,
    identity_saved: bool,
    rx: Option<std::sync::mpsc::Receiver<Result<Option<Photo>, String>>>,
    texture: Option<egui::TextureHandle>,
    avatar: Option<Vec<u8>>,
}
struct Photo {
    path: PathBuf,
    image: egui::ColorImage,
    avatar: Option<Vec<u8>>,
}
pub(super) struct InputHold {
    who: String,
    request: String,
    origin: egui::Pos2,
    until: f64,
}
impl ProfilePhoto {
    pub(super) fn avatar(&self) -> Option<&[u8]> {
        self.avatar.as_deref()
    }
    pub(super) fn texture(&self) -> Option<egui::load::SizedTexture> {
        self.texture
            .as_ref()
            .map(|t| (t.id(), t.size_vec2()).into())
    }
}
fn decode_photo(path: PathBuf, import: bool) -> Result<Photo, String> {
    if atlas_core::cloud::is_dehydrated(&path) {
        return Err("Download this profile picture locally before choosing it".into());
    }
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if metadata.len() > 16 * 1024 * 1024 {
        return Err("Choose a profile picture smaller than 16 MB".into());
    }
    let mut reader = image::ImageReader::open(&path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|e| e.to_string())?;
    let side = decoded.width().min(decoded.height());
    let image = decoded
        .crop_imm(
            (decoded.width() - side) / 2,
            (decoded.height() - side) / 2,
            side,
            side,
        )
        .resize_exact(256, 256, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let path = if import {
        let dir = atlas_core::index::data_dir().join("participant-profile");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dest = dir.join(format!("{}.png", atlas_agent::request_id()));
        image.save(&dest).map_err(|e| e.to_string())?;
        dest
    } else {
        path
    };
    Ok(Photo {
        path,
        avatar: Some(encode_shared_portrait(&image)?),
        image: egui::ColorImage::from_rgba_unmultiplied([256, 256], image.as_raw()),
    })
}

/// Encode only the already-cropped portrait, on the existing photo worker.
/// Flatten transparency on white so JPEG does not create black silhouettes.
fn encode_shared_portrait(image: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let small = image::imageops::resize(image, 32, 32, image::imageops::FilterType::Triangle);
    let rgb = image::RgbImage::from_fn(32, 32, |x, y| {
        let p = small.get_pixel(x, y);
        let a = u32::from(p[3]);
        image::Rgb(std::array::from_fn(|i| {
            ((u32::from(p[i]) * a + 255 * (255 - a) + 127) / 255) as u8
        }))
    });
    for quality in [90, 75, 60, 45, 30, 15] {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
            .encode(rgb.as_raw(), 32, 32, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
        if bytes.len() <= atlas_relay::MAX_AVATAR_BYTES {
            return Ok(bytes);
        }
    }
    Err("Could not make a small shared profile picture".into())
}

#[cfg(test)]
mod photo_tests {
    use super::encode_shared_portrait;

    #[test]
    fn shared_portrait_is_small_jpeg_and_transparency_becomes_white() {
        let detail = image::RgbaImage::from_fn(256, 256, |x, y| {
            image::Rgba([
                (x.wrapping_mul(137) ^ y.wrapping_mul(71)) as u8,
                (x.wrapping_mul(73) ^ y.wrapping_mul(181)) as u8,
                (x ^ y) as u8,
                255,
            ])
        });
        let bytes = encode_shared_portrait(&detail).unwrap();
        assert!(bytes.len() <= atlas_relay::MAX_AVATAR_BYTES);
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (32, 32));
        let clear = image::RgbaImage::from_pixel(32, 32, image::Rgba([0, 0, 0, 0]));
        let white = image::load_from_memory(&encode_shared_portrait(&clear).unwrap())
            .unwrap()
            .to_rgb8();
        assert!(white.pixels().all(|p| p.0.iter().all(|c| *c >= 250)));
    }
}
impl SlateApp {
    pub(crate) fn presence_remove_photo(&mut self, who: &str) -> Result<(), String> {
        if who != "self" {
            return Err("Only your own profile picture can be changed".into());
        }
        // Dropping the old receiver fences an import already running; it can
        // never restore this photo or answer a later picker invocation.
        self.presence.photo.rx = None;
        self.presence.photo.path = None;
        self.presence.photo.texture = None;
        self.presence.photo.avatar = None;
        self.presence.photo.initialized = true;
        let name = self.settings.presence_identity.name.clone();
        self.presence_update_identity(&name, None)
    }
    pub(super) fn presence_release_avatar_hold(
        &mut self,
        ctx: &egui::Context,
        addressed: Option<&str>,
        keep_paused: bool,
    ) {
        let Some(hold) = self.presence.avatar_hold.as_mut() else {
            return;
        };
        if let Some(who) = addressed {
            if who != hold.who {
                return;
            }
        } else {
            let now = ctx.input(|i| i.time);
            let options = ctx.options(|o| o.input_options.clone());
            if ctx.input(|i| i.pointer.primary_pressed()) {
                hold.until = now + options.max_click_duration;
            } else if ctx.input(|i| i.pointer.primary_released()) {
                hold.until = now + options.max_double_click_delay;
            }
            let dragged = ctx
                .pointer_latest_pos()
                .is_some_and(|p| p.distance(hold.origin) > options.max_click_dist);
            if now < hold.until && !dragged {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                    (hold.until - now).min(0.1),
                ));
                return;
            }
        }
        let hold = self.presence.avatar_hold.take().unwrap();
        if !keep_paused
            && self.presence.agents.iter().any(|a| {
                a.profile.id == hold.who
                    && a.request == hold.request
                    && a.paused
                    && a.departure.is_none()
            })
        {
            let _ = self.apply_presence_action(
                ctx,
                chrome::PresenceAction::Pause {
                    id: hold.who,
                    paused: false,
                },
            );
        }
    }
    pub(crate) fn presence_choose_photo(
        &mut self,
        ctx: &egui::Context,
        who: &str,
    ) -> Result<(), String> {
        if who != "self" {
            return Err("Only your own profile picture can be changed".into());
        }
        if self.presence.photo.rx.is_some() {
            return Err("A profile picture is already loading".into());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.presence.photo.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = rfd::FileDialog::new()
                .set_title("Choose your profile picture")
                .add_filter("Picture", &["png", "jpg", "jpeg", "webp"])
                .pick_file()
                .map(|p| decode_photo(p, true))
                .transpose();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        Ok(())
    }
    pub(super) fn presence_poll_photo(&mut self, ctx: &egui::Context) {
        if !self.presence.photo.identity_saved {
            self.settings.save();
            self.presence.photo.identity_saved = true;
        }
        if let Some(result) = self
            .presence
            .photo
            .rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.presence.photo.rx = None;
            match result {
                Ok(Some(photo)) => {
                    self.presence.photo.path = Some(photo.path.clone());
                    self.presence.photo.avatar = photo.avatar;
                    self.presence.photo.texture = Some(ctx.load_texture(
                        "participant-profile",
                        photo.image,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.presence.photo.initialized = true;
                    let name = self.settings.presence_identity.name.clone();
                    if let Err(e) = self.presence_update_identity(&name, Some(photo.path)) {
                        self.toast(e);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.presence.photo.initialized = true;
                    self.toast(e);
                }
            }
        }
        let path = self.settings.presence_identity.photo.clone();
        if self.presence.photo.rx.is_none()
            && (!self.presence.photo.initialized || self.presence.photo.path != path)
        {
            self.presence.photo.initialized = true;
            self.presence.photo.path = path.clone();
            self.presence.photo.texture = None;
            self.presence.photo.avatar = None;
            if let Some(path) = path {
                let (tx, rx) = std::sync::mpsc::channel();
                self.presence.photo.rx = Some(rx);
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(decode_photo(path, false).map(Some));
                    ctx.request_repaint();
                });
            }
        }
    }
    /// Run before board hit testing; no human click passes through an avatar.
    pub(super) fn presence_avatar_input(&mut self, ctx: &egui::Context) -> bool {
        let xf = self.board_xf();
        if let Some(who) = self.presence.chrome.canvas_chat().map(str::to_owned) {
            if let Some(a) = self.presence.agents.iter().find(|a| a.profile.id == who) {
                if let Some(session) = self.presence.roster.session(&a.motion) {
                    let p = session.sample().position;
                    let center = xf.w2s(egui::pos2(p.x as f32, p.y as f32));
                    self.presence
                        .chrome
                        .update_chat_anchor(center + egui::vec2(-20.0, 36.0) * xf.z, xf.z);
                }
            }
        }
        let Some(pointer) = ctx.pointer_latest_pos() else {
            return false;
        };
        if !self.canvas_rect.contains(pointer) {
            return false;
        }
        if self.presence.chrome.occludes_point(ctx, pointer)
            || ctx.layer_id_at(pointer).is_some_and(|layer| {
                matches!(
                    layer.order,
                    egui::Order::Foreground | egui::Order::Tooltip | egui::Order::Debug
                ) || [
                    "presence-people-setup",
                    "presence-schedule-editor",
                    "collaborator-conversation",
                ]
                .iter()
                .any(|name| layer.id == egui::Id::new(name))
            })
        {
            return false;
        }
        let hit = self
            .presence
            .agents
            .iter()
            .filter(|a| a.tab == self.tab().id && a.departure.is_none())
            .filter_map(|a| {
                self.presence
                    .roster
                    .session(&a.motion)
                    .map(|s| (a.profile.id.clone(), s.sample().position))
            })
            .find_map(|(id, p)| {
                let center = xf.w2s(egui::pos2(p.x as f32, p.y as f32));
                (center.distance(pointer) <= (18.0 * xf.z).max(12.0)).then_some((id, center))
            });
        let Some((who, _center)) = hit else {
            return false;
        };
        // Hold briefly for the native double-click window. A single click or
        // drag resumes automatically; only addressing establishes a real pause.
        if ctx.input(|i| i.pointer.primary_pressed())
            && self
                .presence
                .agents
                .iter()
                .any(|a| a.profile.id == who && !a.paused)
        {
            let request = self
                .presence
                .agents
                .iter()
                .find(|a| a.profile.id == who)
                .map(|a| a.request.clone())
                .unwrap_or_default();
            if self
                .apply_presence_action(
                    ctx,
                    chrome::PresenceAction::Pause {
                        id: who.clone(),
                        paused: true,
                    },
                )
                .is_ok()
            {
                self.presence.avatar_hold = Some(InputHold {
                    who: who.clone(),
                    request,
                    origin: pointer,
                    until: ctx.input(|i| i.time)
                        + ctx.options(|o| o.input_options.max_click_duration),
                });
            }
        }
        if ctx.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        }) {
            self.dispatch(
                ctx,
                atlas_commands::CommandId("presence.address"),
                Some(serde_json::json!({"id":who}).to_string()),
            );
        }
        ctx.input(|i| i.pointer.any_down() || i.pointer.any_click())
    }
    pub(crate) fn presence_address(
        &mut self,
        ctx: &egui::Context,
        who: &str,
    ) -> Result<(), String> {
        self.presence_release_avatar_hold(ctx, Some(who), true);
        self.apply_presence_action(
            ctx,
            chrome::PresenceAction::Pause {
                id: who.into(),
                paused: true,
            },
        )?;
        let xf = self.board_xf();
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
            .ok_or("Unknown collaborator")?;
        a.activity = "listening".into();
        a.status = "Board task paused · listening".into();
        let p = self
            .presence
            .roster
            .session(&a.motion)
            .ok_or("Collaborator has left")?
            .sample()
            .position;
        let center = xf.w2s(egui::pos2(p.x as f32, p.y as f32));
        self.presence
            .chrome
            .open_chat_at(who, center + egui::vec2(-20.0, 36.0) * xf.z, xf.z);
        ctx.request_repaint();
        Ok(())
    }
}
