//! Source texture reads and decoding belong to the model parse worker.
use super::*;
use model_preview::TextureSource;

const TEXTURE_BYTES: u64 = 32 * 1024 * 1024;
const DECODE_BYTES: u64 = 64 * 1024 * 1024;
const TEXTURE_EDGE: u32 = 2048;

pub(super) fn read_dependency(path: &Path) -> std::io::Result<Vec<u8>> {
    if atlas_core::cloud::is_dehydrated(path) {
        return Err(std::io::Error::other(
            "Linked model asset is cloud-only; make it available locally.",
        ));
    }
    std::fs::read(path)
}

pub(super) fn decode_textures(scene: &mut PreviewScene) {
    let mut budget = DECODE_BYTES;
    let mut cache = HashMap::<String, Option<TextureSource>>::new();
    for mesh in &mut scene.meshes {
        let Some(material) = mesh.material.as_mut() else {
            continue;
        };
        let Some(source) = material.base_color_texture.take() else {
            continue;
        };
        let key = match &source {
            TextureSource::LocalPath(path) => path.to_string_lossy().into_owned(),
            TextureSource::Embedded { bytes, .. } => format!("embedded-{:p}", bytes.as_ptr()),
            TextureSource::Decoded { .. } => {
                material.base_color_texture = Some(source);
                continue;
            }
        };
        let decoded = cache
            .entry(key)
            .or_insert_with(|| decode(&source, &mut budget));
        material.base_color_texture = decoded.clone();
        if decoded.is_none() && !scene.notes.iter().any(|n|n=="Some base-color textures were missing, cloud-only, unsupported, or exceeded the decode budget.") {
            scene.notes.push("Some base-color textures were missing, cloud-only, unsupported, or exceeded the decode budget.".into());
        }
    }
}

fn decode(source: &TextureSource, budget: &mut u64) -> Option<TextureSource> {
    if *budget < 4 {
        return None;
    }
    let bytes = match source {
        TextureSource::Embedded { bytes, .. } => {
            if bytes.len() as u64 > TEXTURE_BYTES {
                return None;
            }
            bytes.to_vec()
        }
        TextureSource::LocalPath(path) => {
            if atlas_core::cloud::is_dehydrated(path) {
                return None;
            }
            if std::fs::metadata(path).ok()?.len() > TEXTURE_BYTES {
                return None;
            }
            std::fs::read(path).ok()?
        }
        TextureSource::Decoded { .. } => return Some(source.clone()),
    };
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(DECODE_BYTES);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let image = if image.width() > TEXTURE_EDGE || image.height() > TEXTURE_EDGE {
        image.thumbnail(TEXTURE_EDGE, TEXTURE_EDGE)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    let size = width as u64 * height as u64 * 4;
    if size > *budget {
        return None;
    }
    *budget -= size;
    Some(TextureSource::Decoded {
        rgba: rgba.into_raw().into(),
        width,
        height,
    })
}

impl SlateApp {
    pub fn model_materials_of(&self, id: NodeId) -> Option<bool> {
        if let Some(vp) = self.model3d.live.get(&id) {
            return Some(vp.cam.materials);
        }
        self.model_node_info(id).map(|info| info.cam.materials)
    }

    pub fn set_model_materials(&mut self, id: NodeId, enabled: bool) -> bool {
        if !self.model_has_viewport(id)
            || self.doc().scene.node(id).is_none_or(|n| n.locked)
            || self.model_materials_of(id) == Some(enabled)
            || self.refuse_read_only_edit()
        {
            return false;
        }
        self.last_board_edit = None;
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Image(img) = &mut n.kind {
                img.model.materials = enabled;
            }
        });
        self.last_board_edit = None;
        if let Some(vp) = self.model3d.live.get_mut(&id) {
            vp.cam.materials = enabled;
            vp.before.materials = enabled;
            vp.last_interact = Instant::now();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::live_model;
    use super::*;

    #[test]
    #[ignore = "Reads only ATLAS_MODEL_PROBE after the cloud-placeholder guard; writes local diagnostic metadata"]
    fn model_report_probe_uses_the_guarded_production_reader() {
        let path =
            PathBuf::from(std::env::var_os("ATLAS_MODEL_PROBE").expect("set ATLAS_MODEL_PROBE"));
        let bytes = read_dependency(&path).expect("source must be locally available");
        let scene =
            model_preview::load_preview_with_reader(&path, &bytes, &mut read_dependency).unwrap();
        let bounds: Vec<_> = scene
            .meshes
            .iter()
            .map(|mesh| {
                let mut min = [f32::INFINITY; 3];
                let mut max = [f32::NEG_INFINITY; 3];
                for vertex in &mesh.positions {
                    for axis in 0..3 {
                        min[axis] = min[axis].min(vertex[axis]);
                        max[axis] = max[axis].max(vertex[axis]);
                    }
                }
                serde_json::json!({"min":min,"max":max,"triangles":mesh.indices.len()/3})
            })
            .collect();
        let result = serde_json::json!({"cloud_guard":"local", "bytes":bytes.len(),
            "bounds_min":scene.bounds_min,"bounds_max":scene.bounds_max,
            "notes":scene.notes,"meshes":bounds});
        let target =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/model-report-probe.json");
        std::fs::write(&target, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        println!(
            "{} meshes, {} bytes; {}",
            bounds.len(),
            bytes.len(),
            target.display()
        );
    }

    #[test]
    fn live_materials_toggle_is_one_undo_step_and_tracks_undo_redo() {
        let (mut h, id) = live_model("live_materials_undo_redo");
        assert!(h.app.set_model_display(id, ModelDisplay::HiddenLine));
        let saved = h.app.model_node_info(id).unwrap().cam;
        let live = h.app.model3d.live[&id].cam;
        let depth = h.app.tab().journal.undo_depth();

        assert!(h.app.set_model_materials(id, false));
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam,
            ModelCamera {
                materials: false,
                ..saved
            }
        );
        assert_eq!(
            h.app.model3d.live[&id].cam,
            ModelCamera {
                materials: false,
                ..live
            }
        );
        assert_eq!(
            h.app.model3d.live[&id].before,
            ModelCamera {
                materials: false,
                ..saved
            }
        );
        assert!(!h.app.set_model_materials(id, false));
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            depth + 1,
            "same value is a no-op"
        );

        h.app.board_undo();
        h.app.model3d_frame(&h.ctx);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert_eq!(h.app.model_node_info(id).unwrap().cam, saved);
        assert_eq!(h.app.model3d.live[&id].cam, live);
        assert_eq!(h.app.model3d.live[&id].before, saved);

        h.app.board_redo();
        h.app.model3d_frame(&h.ctx);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam,
            ModelCamera {
                materials: false,
                ..saved
            }
        );
        assert_eq!(
            h.app.model3d.live[&id].cam,
            ModelCamera {
                materials: false,
                ..live
            }
        );
    }

    #[test]
    fn locking_orbited_model_keeps_materials_as_a_separate_undo_step() {
        let (mut h, id) = live_model("live_materials_lock_independence");
        let saved = h.app.model_node_info(id).unwrap().cam;
        let depth = h.app.tab().journal.undo_depth();
        assert!(h.app.set_model_materials(id, false));
        let live = h.app.model3d.live[&id].cam;
        h.app.model_drag(id, 38.0, -14.0, false, 240.0);
        let orbited = h.app.model3d.live[&id].cam;
        assert_ne!(orbited.yaw, live.yaw);
        assert!(!orbited.materials);
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            depth + 1,
            "orbit stays ephemeral until lock"
        );

        h.app.lock_model(id);
        assert!(!h.app.model3d.live.contains_key(&id));
        assert_eq!(h.app.model_node_info(id).unwrap().cam, orbited);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

        h.app.board_undo();
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam,
            ModelCamera {
                materials: false,
                ..saved
            },
            "undoing the camera leaves the independent material decision intact"
        );
        h.app.board_undo();
        assert_eq!(h.app.model_node_info(id).unwrap().cam, saved);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        h.app.board_redo();
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam,
            ModelCamera {
                materials: false,
                ..saved
            }
        );
        h.app.board_redo();
        assert_eq!(h.app.model_node_info(id).unwrap().cam, orbited);
    }

    #[test]
    fn materials_change_invalidates_display_swatches_without_changing_pose() {
        let (mut h, id) = live_model("materials_swatch_invalidation");
        let first = h.app.ensure_model_display_swatches(id);
        assert!(h.app.set_model_display(id, ModelDisplay::HiddenLine));
        assert_eq!(
            h.app.ensure_model_display_swatches(id),
            first,
            "each display chip already carries its own pass"
        );
        let saved = h.app.model_node_info(id).unwrap().cam;
        let live = h.app.model3d.live[&id].cam;

        assert!(h.app.set_model_materials(id, false));
        let changed = h.app.ensure_model_display_swatches(id);
        assert!(
            changed > first,
            "material appearance invalidates cached chips"
        );
        assert_eq!(
            h.app.ensure_model_display_swatches(id),
            changed,
            "unchanged appearance does not continuously queue new chips"
        );
        assert_eq!(
            h.app.model_node_info(id).unwrap().cam,
            ModelCamera {
                materials: false,
                ..saved
            }
        );
        assert_eq!(
            h.app.model3d.live[&id].cam,
            ModelCamera {
                materials: false,
                ..live
            }
        );

        h.app.board_undo();
        h.app.model3d_frame(&h.ctx);
        assert_eq!(h.app.model_node_info(id).unwrap().cam, saved);
        assert_eq!(h.app.model3d.live[&id].cam, live);
        assert!(
            h.app.ensure_model_display_swatches(id) > changed,
            "undo also rejects stale material swatches"
        );
    }

    #[test]
    fn texture_decode_is_bounded_and_shared() {
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let source = TextureSource::Embedded {
            bytes: encoded.into_inner().into(),
            mime_type: None,
        };
        let mut budget = 64;
        let decoded = decode(&source, &mut budget).unwrap();
        assert_eq!(budget, 0);
        assert!(decode(&source, &mut budget).is_none());
        assert!(matches!(
            decoded,
            TextureSource::Decoded {
                width: 4,
                height: 4,
                ..
            }
        ));
    }
}
