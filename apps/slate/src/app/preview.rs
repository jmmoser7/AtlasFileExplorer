//! Lazy full-resolution canvas previews.
//!
//! Every canvas (grid, Venn, board, presentation) paints instantly from the
//! shared 192 px thumbnail cache, then *sharpens in place*: when an item's
//! on-screen size outgrows its thumbnail, a capped full-resolution decode is
//! queued (`atlas_core::preview`) and the texture swaps when it lands.
//!
//! Performance contract (a couple thousand items must stay smooth):
//! - Painting never blocks: [`SlateApp::item_texture`] returns the best
//!   texture already on the GPU and only *queues* upgrades.
//! - At most [`REQUESTS_PER_FRAME`] new decodes start per frame, and the
//!   pool serves LIFO, so a fast zoom or scroll can't build a decode storm —
//!   whatever is on screen right now always wins.
//! - Decoded previews live in an LRU cache bounded by the user's memory
//!   budget (Advanced → Canvas previews); zoomed-out items age out first and
//!   silently fall back to their thumbnails.
//! - Target sizes are quantized to a power-of-two ladder capped by the
//!   user's max-resolution setting, so continuous zoom re-decodes a handful
//!   of times, not every frame.

use super::{SlateApp, ThumbState};
use atlas_core::preview::{tier_for, PreviewRequest};
use atlas_core::thumbs::THUMB_PX;
use eframe::egui::{self, TextureHandle};
use slate_doc::ItemId;

/// Don't start a full-res decode until the thumbnail is visibly upscaled.
const UPGRADE_FACTOR: f32 = 1.15;

/// New full-resolution decodes allowed to start per frame. Combined with the
/// pool's LIFO order this bounds latency for what's actually on screen while
/// a big scene streams in gradually.
const REQUESTS_PER_FRAME: u32 = 3;

/// Tier sentinel: the source decoded completely — no higher tier exists, so
/// any future desired size is already satisfied.
pub(crate) const PX_EXACT: u32 = u32::MAX;

/// One GPU-resident full-resolution preview.
pub struct PreviewEntry {
    pub tex: TextureHandle,
    /// Ladder tier this texture satisfies ([`PX_EXACT`] = native size).
    pub px: u32,
    /// Decoded RGBA footprint, the unit of the memory budget.
    pub bytes: usize,
    /// Frame counter of the last paint that needed this preview (LRU key).
    pub last_used: u64,
}

impl SlateApp {
    /// The best texture for an item at a given on-screen size (physical px,
    /// longest edge): the full-res preview when one is resident, else the
    /// thumbnail, else `None` (caller paints its placeholder). Queues the
    /// thumbnail and any preview upgrade as side effects — never blocks.
    pub fn item_texture(&mut self, item_id: ItemId, desired_px: f32) -> Option<TextureHandle> {
        let (key, path) = self
            .doc()
            .item(item_id)
            .map(|it| (it.cache_key.clone(), it.path.clone()))?;
        if key.is_empty() {
            return None;
        }
        // The thumbnail tier is always ensured — it is the instant fallback
        // every other tier degrades to.
        if !self.textures.contains_key(&key) {
            self.request_thumb(item_id);
        }

        let settings = &self.settings.preview;
        let wants_preview = settings.enabled
            && desired_px > THUMB_PX as f32 * UPGRADE_FACTOR
            && slate_doc::media_kind(&path) != slate_doc::MediaKind::Text;
        if wants_preview {
            let tier = tier_for(desired_px, settings.max_px);
            let satisfied = matches!(self.preview_cache.get(&key), Some(e) if e.px >= tier);
            if !satisfied
                && !self.preview_failed.contains(&key)
                && self.preview_inflight.get(&key).copied().unwrap_or(0) < tier
                && self.preview_reqs_this_frame < REQUESTS_PER_FRAME
            {
                self.preview_reqs_this_frame += 1;
                let slot = self.next_preview_slot;
                self.next_preview_slot = self.next_preview_slot.wrapping_add(1);
                self.preview_slots.insert(slot, (key.clone(), tier));
                self.preview_inflight.insert(key.clone(), tier);
                self.previews.request(PreviewRequest {
                    id: slot,
                    path,
                    key: key.clone(),
                    target_px: tier,
                });
            }
            // Touch the LRU only while the preview is actually needed; a
            // zoomed-out item keeps painting its resident preview below but
            // stops defending it from eviction.
            if let Some(e) = self.preview_cache.get_mut(&key) {
                e.last_used = self.frame_no;
            }
        }

        if let Some(e) = self.preview_cache.get(&key) {
            return Some(e.tex.clone());
        }
        match self.textures.get(&key) {
            Some(ThumbState::Ready(t)) => Some(t.clone()),
            _ => None,
        }
    }

    /// Upload finished decodes. Runs once per frame before painting.
    pub(super) fn drain_previews(&mut self, ctx: &egui::Context) {
        while let Ok(res) = self.previews.rx.try_recv() {
            let Some((key, tier)) = self.preview_slots.remove(&res.id) else {
                continue;
            };
            if self.preview_inflight.get(&key) == Some(&tier) {
                self.preview_inflight.remove(&key);
            }
            match res.image {
                Some((w, h, rgba)) => {
                    // Smaller than requested = the source itself ran out of
                    // pixels; no higher tier will ever exist.
                    let px = if w.max(h) < tier { PX_EXACT } else { tier };
                    // LIFO can deliver a stale lower tier after a newer
                    // higher one; never downgrade.
                    if matches!(self.preview_cache.get(&key), Some(e) if e.px >= px) {
                        continue;
                    }
                    let img =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    let tex = ctx.load_texture(
                        format!("slate-preview-{key}-{px}"),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.preview_cache.insert(
                        key,
                        PreviewEntry {
                            tex,
                            px,
                            bytes: (w as usize) * (h as usize) * 4,
                            last_used: self.frame_no,
                        },
                    );
                    ctx.request_repaint();
                }
                // Undecodable, or no sharper than the thumbnail: remember so
                // the paint loop stops asking every frame.
                None => {
                    self.preview_failed.insert(key);
                }
            }
        }
    }

    /// Enforce the preview memory budget: evict least-recently-needed
    /// entries first, never anything painted this frame or the last (their
    /// thumbnails would pop back in mid-view). Runs once per frame, after
    /// painting, so `last_used` is fresh.
    pub(super) fn evict_previews(&mut self) {
        let budget = (self.settings.preview.budget_mb as usize) << 20;
        let mut total: usize = self.preview_cache.values().map(|e| e.bytes).sum();
        if total <= budget {
            return;
        }
        let mut by_age: Vec<(u64, String, usize)> = self
            .preview_cache
            .iter()
            .map(|(k, e)| (e.last_used, k.clone(), e.bytes))
            .collect();
        by_age.sort_unstable_by_key(|a| a.0);
        for (last_used, key, bytes) in by_age {
            if total <= budget || last_used + 1 >= self.frame_no {
                break;
            }
            self.preview_cache.remove(&key);
            total -= bytes;
        }
    }

    /// (entries, resident bytes) — the Advanced window's usage readout.
    pub fn preview_cache_stats(&self) -> (usize, usize) {
        (
            self.preview_cache.len(),
            self.preview_cache.values().map(|e| e.bytes).sum(),
        )
    }

    /// Drop every resident preview (textures free with their handles) and
    /// forget past failures so changed environments (e.g. pdfium installed
    /// mid-session) get retried.
    pub fn clear_preview_cache(&mut self) {
        self.preview_cache.clear();
        self.preview_failed.clear();
    }
}
