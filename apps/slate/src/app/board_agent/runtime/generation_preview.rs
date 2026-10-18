//! Ephemeral generator images share Slate's existing background preview pool.
use super::*;
use atlas_core::preview::{PreviewBytesRequest, PreviewOutcome, PreviewResult, MAX_LIVE_TARGET_PX};

const LIVE_INFLIGHT: usize = 4;
const LIVE_ADMISSIONS_PER_FRAME: usize = 2;
/// Eight RGBA previews at 1024² contain at most 32 MiB of texture pixels.
const LIVE_RESIDENTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    tab: u64,
    node: NodeId,
    session: String,
    request: String,
}

struct Ticket {
    identity: Identity,
    frame: u64,
    step: u32,
    steps: u32,
}

/// Process-owned: moved with Codex links when document runtimes are parked.
#[derive(Default)]
pub(in crate::app::board_agent) struct GenerationPreviews {
    pending: HashMap<u32, Ticket>,
    seen: HashMap<(u64, NodeId), (String, String, u64)>,
    last_admitted: u64,
}

impl GenerationPreviews {
    fn available(&self, identity: &Identity) -> bool {
        self.pending.len() < LIVE_INFLIGHT
            && !self.pending.values().any(|ticket| {
                ticket.identity.tab == identity.tab && ticket.identity.node == identity.node
            })
    }

    fn seen(&self, identity: &Identity, frame: u64) -> bool {
        self.seen
            .get(&(identity.tab, identity.node))
            .is_some_and(|(session, request, seen)| {
                session == &identity.session && request == &identity.request && *seen >= frame
            })
    }

    fn remember(&mut self, ticket: &Ticket) {
        self.seen.insert(
            (ticket.identity.tab, ticket.identity.node),
            (
                ticket.identity.session.clone(),
                ticket.identity.request.clone(),
                ticket.frame,
            ),
        );
    }

    fn forget(&mut self, ticket: &Ticket) {
        let key = (ticket.identity.tab, ticket.identity.node);
        if self
            .seen
            .get(&key)
            .is_some_and(|(session, request, frame)| {
                session == &ticket.identity.session
                    && request == &ticket.identity.request
                    && *frame == ticket.frame
            })
        {
            self.seen.remove(&key);
        }
    }
}

impl SlateApp {
    fn generation_preview_resident_available(&self, node: NodeId) -> bool {
        self.agents.previews.contains_key(&node) || self.agents.previews.len() < LIVE_RESIDENTS
    }

    fn generation_preview_identity_current(&self, identity: &Identity) -> bool {
        self.tab().id == identity.tab
            && self.agents.bindings.get(&identity.node) == Some(&identity.session)
            && self.agents.requests.get(&identity.node) == Some(&identity.request)
            && self
                .doc()
                .scene
                .node(identity.node)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|agent| agent.session == identity.session)
            && self.agent_is_running(identity.node)
    }

    /// Samples only the latest engine frame, with one outstanding decode per
    /// generator and four across all documents. No codec work runs here.
    pub(in crate::app::board_agent) fn pump_generation_previews(&mut self, ctx: &egui::Context) {
        let tab = self.tab().id;
        let mut running: Vec<(NodeId, String, String)> = self
            .agents
            .requests
            .iter()
            .filter(|(id, _)| self.agent_is_running(**id))
            .filter_map(|(id, request)| {
                Some((*id, request.clone(), self.agents.bindings.get(id)?.clone()))
            })
            .collect();
        running.sort_unstable_by_key(|(node, _, _)| node.0);
        let start = running
            .partition_point(|(node, _, _)| node.0 <= self.agents.live_decodes.last_admitted);
        running.rotate_left(start);
        self.agents.previews.retain(|id, preview| {
            preview.tab == tab
                && running.iter().any(|(node, request, session)| {
                    node == id && request == &preview.request && session == &preview.session
                })
        });
        self.agents
            .live_decodes
            .seen
            .retain(|(doc, id), _| *doc == tab && running.iter().any(|(node, _, _)| node == id));
        let target =
            ctx.input(|input| input.max_texture_side.min(MAX_LIVE_TARGET_PX as usize)) as u32;
        let mut admitted = 0;
        for (node, request, session) in running {
            if admitted >= LIVE_ADMISSIONS_PER_FRAME {
                break;
            }
            let identity = Identity {
                tab,
                node,
                request,
                session,
            };
            if !self.agents.live_decodes.available(&identity)
                || !self.generation_preview_resident_available(node)
            {
                continue;
            }
            let Some(frame) = self
                .agents
                .codex
                .get(&identity.session)
                .and_then(|link| link.preview())
                .filter(|frame| frame.request == identity.request)
            else {
                continue;
            };
            if self.agents.live_decodes.seen(&identity, frame.frame)
                || self.agents.previews.get(&node).is_some_and(|preview| {
                    preview.tab == identity.tab
                        && preview.session == identity.session
                        && preview.request == identity.request
                        && preview.frame >= frame.frame
                })
            {
                continue;
            }
            let ticket = Ticket {
                identity,
                frame: frame.frame,
                step: frame.step,
                steps: frame.steps,
            };
            self.agents.live_decodes.remember(&ticket);
            let slot = self.next_preview_slot;
            self.next_preview_slot = self.next_preview_slot.wrapping_add(1);
            // Namespace prevents a live frame superseding a placed-file request.
            let key = format!(
                "generation:{}:{}:{}:{}",
                tab, node.0, ticket.identity.session, ticket.identity.request
            );
            self.previews.request_bytes(PreviewBytesRequest {
                id: slot,
                key,
                image: frame.image,
                target_px: target,
            });
            self.agents.live_decodes.pending.insert(slot, ticket);
            self.agents.live_decodes.last_admitted = node.0;
            admitted += 1;
        }
        if !self.agents.live_decodes.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// Called by the single preview inbox drain. A consumed terminal result
    /// always retires its slot, including requests for a tab since left.
    pub(crate) fn consume_generation_preview(
        &mut self,
        result: PreviewResult,
        ctx: &egui::Context,
    ) -> Option<PreviewResult> {
        let Some(ticket) = self.agents.live_decodes.pending.remove(&result.id) else {
            return Some(result);
        };
        if !self.generation_preview_identity_current(&ticket.identity) {
            self.agents.live_decodes.forget(&ticket);
            return None;
        }
        if matches!(result.outcome, PreviewOutcome::Cancelled) {
            self.agents.live_decodes.forget(&ticket);
            return None;
        }
        let PreviewOutcome::Ready(w, h, rgba) = result.outcome else {
            return None;
        };
        let node = ticket.identity.node;
        // Pending jobs can race for the last resident slot. Leave a declined
        // frame retryable when a generator stops and releases its texture.
        if !self.generation_preview_resident_available(node) {
            self.agents.live_decodes.forget(&ticket);
            return None;
        }
        if self.agents.previews.get(&node).is_some_and(|old| {
            old.tab == ticket.identity.tab
                && old.session == ticket.identity.session
                && old.request == ticket.identity.request
                && old.frame >= ticket.frame
        }) {
            return None;
        }
        let hardware_limit = ctx.input(|input| input.max_texture_side);
        if w == 0
            || h == 0
            || w > MAX_LIVE_TARGET_PX
            || h > MAX_LIVE_TARGET_PX
            || w as usize > hardware_limit
            || h as usize > hardware_limit
            || rgba.len() != w as usize * h as usize * 4
        {
            return None;
        }
        let pixels = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        let texture = match self.agents.previews.remove(&node) {
            Some(mut previous) => {
                previous.texture.set(pixels, egui::TextureOptions::LINEAR);
                previous.texture
            }
            None => ctx.load_texture(
                format!("generation-preview-{}-{}", ticket.identity.tab, node.0),
                pixels,
                egui::TextureOptions::LINEAR,
            ),
        };
        self.agents.previews.insert(
            node,
            LivePreview {
                tab: ticket.identity.tab,
                session: ticket.identity.session,
                request: ticket.identity.request,
                frame: ticket.frame,
                step: ticket.step,
                steps: ticket.steps,
                texture,
            },
        );
        ctx.request_repaint();
        None
    }

    /// The image coming into focus while it samples, with a thin step bar.
    pub(in crate::app::board_agent) fn paint_generation_preview(
        &self,
        painter: &egui::Painter,
        id: NodeId,
        body: Rect,
        z: f32,
    ) {
        let Some(preview) = self.agents.previews.get(&id) else {
            return;
        };
        if preview.tab != self.tab().id
            || self.agents.requests.get(&id) != Some(&preview.request)
            || self.agents.bindings.get(&id) != Some(&preview.session)
        {
            return;
        }
        let size = preview.texture.size_vec2();
        let scale = (body.width() / size.x).min(body.height() / size.y);
        let rect = Rect::from_center_size(body.center(), size * scale);
        painter.image(
            preview.texture.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if preview.steps > 0 {
            let fraction = (preview.step as f32 / preview.steps as f32).clamp(0.0, 1.0);
            let height = canvas_scale::px(3.0, z);
            let bar = Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - height),
                egui::vec2(rect.width() * fraction, height),
            );
            painter.rect_filled(bar, 0.0, self.palette().accent);
        }
    }
}

impl AgentRuntime {
    /// Sampling images are disposable display state. Parking a tab releases
    /// its GPU pixels and lets its latest engine frame be sampled on return.
    pub(in crate::app::board_agent) fn clear_generation_preview_residents(&mut self) {
        self.previews.clear();
        self.live_decodes.seen.clear();
    }
}

#[cfg(test)]
mod tests;
