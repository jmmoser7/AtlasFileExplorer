//! Scoped production-board pixels. The live camera and document never move.
use super::*;
use crate::app::board::{paint_query, paints_in_view, BoardXf};
use atlas_shell::frame_raster::FrameRaster;
use egui::{ClippedPrimitive, Id, LayerId, Order, Ui, UiBuilder};
use serde_json::{json, Value};
use slate_doc::scene::{Node, NodeKind, PortalKind, WorldRect};
use std::{collections::HashMap, sync::mpsc, time::Instant};
mod preflight;
#[cfg(test)]
mod tests;
mod textures;

const MAX_NODES: usize = 512;
const MAX_EDGE: usize = 1200;
const MAX_TRIANGLES: usize = 100_000;
const NODES_PER_FRAME: usize = 4;
const STAMP_WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) struct PreviewJob {
    nodes: Vec<Node>,
    next: usize,
    bounds: WorldRect,
    size: [usize; 2],
    primitives: Vec<ClippedPrimitive>,
    textures: HashMap<egui::TextureId, textures::Source>,
    rx: Option<mpsc::Receiver<Result<Value, String>>>,
    path: PathBuf,
    pub(super) result: Option<Value>,
    needs_sources: bool,
    unavailable: usize,
    stamp_wait_spent: std::time::Duration,
    stamp_wait_since: Option<Instant>,
}

impl PreviewJob {
    pub(super) fn new(
        app: &SlateApp,
        grant: &CollaboratorGrant,
        call: &BoardCall,
    ) -> Result<Self, String> {
        let BoardOperation::Invoke {
            targets, arguments, ..
        } = &call.operation
        else {
            return Err("Preview requires an explicit board command".into());
        };
        if !arguments.as_object().is_some_and(|a| a.is_empty()) {
            return Err("Preview takes no arguments; choose one frame ID or empty targets for the granted board".into());
        }
        if targets.len() > 1 {
            return Err("Preview one frame at a time".into());
        }
        let scene = &app.doc().scene;
        let frame = targets
            .first()
            .map(|id| {
                id.parse::<u64>()
                    .map(slate_doc::NodeId)
                    .map_err(|_| "Use one stable numeric frame ID".to_owned())
            })
            .transpose()?;
        let mut ids = if let Some(id) = frame {
            if !scene.node(id).is_some_and(Node::is_frame) {
                return Err("Preview target must be a frame".into());
            }
            let mut ids = scene.members_of(id);
            ids.push(id);
            ids
        } else {
            scene
                .nodes
                .iter()
                .filter(|n| !n.hidden && grant.allows_node(&n.id.0.to_string()))
                .map(|n| n.id)
                .collect()
        };
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() || ids.len() > MAX_NODES {
            return Err("Preview needs 1–512 granted objects; choose a smaller frame".into());
        }
        if ids.iter().any(|id| !grant.allows_node(&id.0.to_string())) {
            return Err("A frame member is outside this read grant".into());
        }
        let mut nodes: Vec<_> = scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && ids.contains(&n.id))
            .cloned()
            .collect();
        nodes.sort_by_key(|node| !node.is_frame());
        // Validate every source before any node reaches the painter (which can
        // enqueue texture work) and bound per-node inputs before tessellation.
        for node in &nodes {
            preflight::validate_source(app, node)?;
            preflight::validate_node(node)?;
        }
        let bounds = frame
            .and_then(|id| scene.node(id).map(|n| n.rect))
            .unwrap_or_else(|| {
                let a =
                    nodes
                        .iter()
                        .map(slate_doc::node_aabb)
                        .fold(None::<WorldRect>, |bounds, r| {
                            Some(match bounds {
                                None => r,
                                Some(b) => WorldRect::new(
                                    b.x.min(r.x),
                                    b.y.min(r.y),
                                    (b.x + b.w).max(r.x + r.w) - b.x.min(r.x),
                                    (b.y + b.h).max(r.y + r.h) - b.y.min(r.y),
                                ),
                            })
                        });
                a.unwrap_or(WorldRect::new(0.0, 0.0, 1.0, 1.0))
            })
            .normalized();
        if ![bounds.x, bounds.y, bounds.w, bounds.h]
            .iter()
            .all(|v| v.is_finite())
            || bounds.w <= 0.0
            || bounds.h <= 0.0
        {
            return Err("Preview bounds must be finite and nonempty".into());
        }
        let scale = MAX_EDGE as f32 / bounds.w.max(bounds.h);
        let size = [
            (bounds.w * scale).round().max(1.0) as usize,
            (bounds.h * scale).round().max(1.0) as usize,
        ];
        let needs_sources = nodes.iter().any(|n| {
            matches!(n.kind, NodeKind::Image(_))
                || matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::FileAtlas)
        });
        if needs_sources
            && !grant
                .readable_sources
                .iter()
                .any(|s| s == "board-file-inputs")
        {
            return Err(
                "Allow board images and wired inputs before rendering a frame containing images"
                    .into(),
            );
        }
        let workspace = app
            .ai
            .config
            .workspace_dir
            .as_ref()
            .ok_or("Set the AI workspace before requesting previews")?;
        let path = workspace
            .join("portal-previews")
            .join(format!("board-{}.png", atlas_agent::request_id()));
        Ok(Self {
            nodes,
            next: 0,
            bounds,
            size,
            primitives: vec![],
            textures: HashMap::new(),
            rx: None,
            path,
            result: None,
            needs_sources,
            unavailable: 0,
            stamp_wait_spent: std::time::Duration::ZERO,
            stamp_wait_since: None,
        })
    }

    pub(super) fn poll(
        &mut self,
        app: &mut SlateApp,
        ctx: &egui::Context,
        grant: &CollaboratorGrant,
    ) -> Result<bool, String> {
        if self.needs_sources
            && !grant
                .readable_sources
                .iter()
                .any(|s| s == "board-file-inputs")
        {
            return Err("Preview source permission was revoked".into());
        }
        if self
            .nodes
            .iter()
            .any(|n| !grant.allows_node(&n.id.0.to_string()))
        {
            return Err("Preview object permission was revoked".into());
        }
        // The read owner can keep polling after the worker finishes while the
        // collaborator's witnessed motion reaches its ready phase. Preserve
        // the consumed result instead of reading the closed channel again.
        if self.result.is_some() {
            return Ok(true);
        }
        if let Some(rx) = &self.rx {
            return match rx.try_recv() {
                Ok(result) => {
                    self.result = Some(result?);
                    Ok(true)
                }
                Err(mpsc::TryRecvError::Empty) => Ok(false),
                Err(_) => Err("Preview worker stopped; retry the preview".into()),
            };
        }
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(self.size[0] as f32, self.size[1] as f32),
        );
        let xf = BoardXf {
            center: screen.center(),
            offset: egui::vec2(
                self.bounds.x + self.bounds.w * 0.5,
                self.bounds.y + self.bounds.h * 0.5,
            ),
            z: self.size[0] as f32 / self.bounds.w,
        };
        let layer = LayerId::new(
            Order::Background,
            Id::new(("board-preview", self.path.clone())),
        );
        let painter = ctx.layer_painter(layer).with_clip_rect(screen);
        let ui = Ui::new(
            ctx.clone(),
            layer.id,
            UiBuilder::new().layer_id(layer).max_rect(screen),
        );
        let tiled = app.brush_tiles_enabled;
        app.brush_tiles_enabled = false;
        let mut waiting_for_stamp = false;
        let painted = (|| -> Result<(), String> {
            for _ in 0..NODES_PER_FRAME {
                let Some(node) = self.nodes.get(self.next) else {
                    break;
                };
                preflight::validate_source(app, node)?;
                preflight::validate_node(node)?;
                if matches!(&node.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web) {
                    // A board preview must not bypass the Web origin's own consent.
                    app.agent_web_check_scope(node.id)?;
                }
                if !paints_in_view(node, &paint_query(&self.bounds)) {
                    self.next += 1;
                    continue;
                }
                // Live hosts/docks paint interactive UI and update viewport geometry.
                // Omit them explicitly instead of disturbing the human's live host.
                let live_media = match &node.kind {
                    NodeKind::Image(image) if image.agent.is_none() || !image.item.is_none() => {
                        app.image_item_path(node.id).is_some_and(|path| {
                            slate_doc::media_kind(&path) != slate_doc::MediaKind::Image
                        })
                    }
                    _ => false,
                };
                if live_media || matches!(node.kind, NodeKind::Portal(_) | NodeKind::DockStrip(_)) {
                    self.unavailable += 1;
                    self.next += 1;
                    continue;
                }
                let keep_shapes =
                    ctx.graphics(|g| g.get(layer).map_or(0, |list| list.all_entries().count()));
                app.paint_board_node(&ui, &painter, &xf, node, false);
                if !textures::remember(app, ctx, &xf, node, &mut self.textures)? {
                    self.unavailable += 1;
                }
                if textures::stamp_pending(app, node) {
                    let since = *self.stamp_wait_since.get_or_insert_with(Instant::now);
                    if self.stamp_wait_spent + since.elapsed() >= STAMP_WAIT_LIMIT {
                        self.stamp_wait_spent += since.elapsed();
                        self.stamp_wait_since = None;
                        self.unavailable += 1;
                        self.next += 1;
                    } else {
                        waiting_for_stamp = true;
                    }
                    let shapes = ctx.graphics_mut(|graphics| {
                        graphics
                            .get_mut(layer)
                            .map(|list| {
                                std::mem::take(list)
                                    .all_entries()
                                    .take(keep_shapes)
                                    .cloned()
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    });
                    self.primitives.extend(ctx.tessellate(shapes, 1.0));
                    if waiting_for_stamp {
                        ctx.request_repaint_after(std::time::Duration::from_millis(16));
                        break;
                    }
                    continue;
                }
                if let Some(since) = self.stamp_wait_since.take() {
                    self.stamp_wait_spent += since.elapsed();
                }
                self.next += 1;
            }
            Ok(())
        })();
        app.brush_tiles_enabled = tiled;
        // Never let this isolated layer reach the actual native viewport.
        let shapes = ctx.graphics_mut(|graphics| {
            graphics
                .get_mut(layer)
                .map(|list| {
                    let owned = std::mem::take(list);
                    owned.all_entries().cloned().collect::<Vec<_>>()
                })
                .unwrap_or_default()
        });
        painted?;
        self.primitives.extend(ctx.tessellate(shapes, 1.0));
        let triangles: usize = self
            .primitives
            .iter()
            .map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) => m.indices.len() / 3,
                _ => 0,
            })
            .sum();
        if triangles > MAX_TRIANGLES {
            return Err("Preview geometry limit reached; choose a smaller frame".into());
        }
        if waiting_for_stamp || self.next < self.nodes.len() {
            ctx.request_repaint();
            return Ok(false);
        }
        let fonts = ctx.fonts(|fonts| fonts.image());
        let primitives = std::mem::take(&mut self.primitives);
        let sources = std::mem::take(&mut self.textures);
        let path = self.path.clone();
        let size = self.size;
        let bounds = self.bounds;
        let background = app.palette().bg;
        let unavailable = self.unavailable;
        let (tx, rx) = mpsc::sync_channel(1);
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let result = (|| -> Result<Value, String> {
                let mut raster = FrameRaster::new(size[0], size[1]);
                raster
                    .px
                    .fill(background.to_array().map(|c| c as f32 / 255.0));
                raster.textures.insert(
                    egui::TextureId::default(),
                    (
                        fonts.size[0],
                        fonts.size[1],
                        fonts.srgba_pixels(None).collect(),
                    ),
                );
                let mut texture_pixels = 0usize;
                for (id, source) in sources {
                    let pixels = source.pixels()?;
                    texture_pixels = texture_pixels.saturating_add(pixels.pixels.len());
                    if texture_pixels > 16 * 1024 * 1024 {
                        return Err("Combined preview textures exceed the pixel budget; choose a smaller frame".into());
                    }
                    raster
                        .textures
                        .insert(id, (pixels.size[0], pixels.size[1], pixels.pixels));
                }
                let missing: std::collections::HashSet<_> = primitives
                    .iter()
                    .filter_map(|p| match &p.primitive {
                        egui::epaint::Primitive::Mesh(mesh)
                            if !raster.textures.contains_key(&mesh.texture_id) =>
                        {
                            Some(mesh.texture_id)
                        }
                        _ => None,
                    })
                    .collect();
                let callbacks = primitives
                    .iter()
                    .filter(|p| matches!(p.primitive, egui::epaint::Primitive::Callback(_)))
                    .count();
                // A separate total-work bound covers heavily overlapping geometry.
                let work: f64 = primitives
                    .iter()
                    .map(|p| match &p.primitive {
                        egui::epaint::Primitive::Mesh(m) => m
                            .indices
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .map(|t| {
                                let a = m.vertices[t[0] as usize].pos;
                                let b = m.vertices[t[1] as usize].pos;
                                let c = m.vertices[t[2] as usize].pos;
                                let r = egui::Rect::from_min_max(a.min(b).min(c), a.max(b).max(c))
                                    .intersect(p.clip_rect)
                                    .intersect(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(size[0] as f32, size[1] as f32),
                                    ));
                                (r.width().max(0.0).ceil() as f64 + 1.0)
                                    * (r.height().max(0.0).ceil() as f64 + 1.0)
                            })
                            .sum(),
                        _ => 0.0,
                    })
                    .sum();
                if work > 64_000_000.0 {
                    return Err("Preview raster-work budget reached; choose a smaller frame".into());
                }
                raster.draw(&primitives);
                let rgba: Vec<u8> = raster
                    .px
                    .iter()
                    .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
                    .collect();
                crate::app::board_portal::capture::output::write_png(
                    &path,
                    &rgba,
                    size[0] as u32,
                    size[1] as u32,
                )?;
                let complete = missing.is_empty() && unavailable == 0 && callbacks == 0;
                Ok(
                    json!({"image_source":path,"width":size[0],"height":size[1],"bounds":bounds,"renderer":"slate-production-board-painter","complete":complete,"missing_texture_count":missing.len(),"unavailable_objects":unavailable,"unsupported_callbacks":callbacks,"note":if complete{"Read-only authored scene preview; no camera, selection or document changes"}else{"Some host/texture content is unavailable; inspect those portals directly before claiming full visual verification"}}),
                )
            })();
            let _ = tx.send(result);
        });
        Ok(false)
    }
}
