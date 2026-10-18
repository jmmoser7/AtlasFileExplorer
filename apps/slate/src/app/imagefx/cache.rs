//! Adjusted GPU texture residency and admission, shared by board and model FX.
//!
//! Admission happens before filtering or uploading. Keep this pass and the
//! preceding pass resident: an over-budget visible set must not reload itself
//! cyclically. A small reserved tier lets those views retain filtered thumbs.

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;

type Key = (String, u64, u32);

#[derive(Clone, Copy)]
pub(crate) enum TextureClass {
    Full,
    Thumbnail,
}

#[derive(Clone, Copy)]
struct Limits {
    bytes: usize,
    entries: usize,
    thumbnail_bytes: usize,
    thumbnail_entries: usize,
    uploads: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 128 * 1024 * 1024,
            entries: 256,
            thumbnail_bytes: 8 * 1024 * 1024,
            thumbnail_entries: 32,
            uploads: 2,
        }
    }
}

struct Entry<T> {
    value: T,
    bytes: usize,
    recency: u64,
    seen: u64,
}

struct Residency<T> {
    entries: HashMap<Key, Entry<T>>,
    bytes: usize,
    clock: u64,
    pass: Option<u64>,
    uploads: usize,
    limits: Limits,
}

impl<T> Default for Residency<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            clock: 0,
            pass: None,
            uploads: 0,
            limits: Limits::default(),
        }
    }
}

#[derive(Debug, PartialEq)]
enum Denied {
    Dimensions,
    Capacity,
    UploadQuota,
}

impl<T: Clone> Residency<T> {
    fn begin_pass(&mut self, pass: u64) {
        if self.pass != Some(pass) {
            self.pass = Some(pass);
            self.uploads = 0;
        }
    }

    fn get(&mut self, key: &Key, pass: u64) -> Option<T> {
        self.begin_pass(pass);
        let entry = self.entries.get_mut(key)?;
        self.clock = self.clock.saturating_add(1);
        entry.recency = self.clock;
        entry.seen = pass;
        Some(entry.value.clone())
    }

    fn admit(
        &mut self,
        size: [usize; 2],
        class: TextureClass,
        pass: u64,
        max_side: usize,
    ) -> Result<usize, Denied> {
        self.begin_pass(pass);
        let bytes = crate::app::preview::upload::checked_texture_bytes(size, max_side)
            .ok_or(Denied::Dimensions)?;
        let (byte_limit, entry_limit) = match class {
            TextureClass::Full => (
                self.limits
                    .bytes
                    .saturating_sub(self.limits.thumbnail_bytes),
                self.limits
                    .entries
                    .saturating_sub(self.limits.thumbnail_entries),
            ),
            TextureClass::Thumbnail => (self.limits.bytes, self.limits.entries),
        };
        if bytes > byte_limit || entry_limit == 0 {
            return Err(Denied::Capacity);
        }
        if self.uploads >= self.limits.uploads {
            return Err(Denied::UploadQuota);
        }
        let protected_since = pass.saturating_sub(1);
        let (reclaimable_bytes, reclaimable_entries) = self
            .entries
            .values()
            .filter(|entry| entry.seen < protected_since)
            .fold((0usize, 0usize), |(bytes, count), entry| {
                (bytes.saturating_add(entry.bytes), count + 1)
            });
        if self.bytes.saturating_sub(reclaimable_bytes) > byte_limit - bytes
            || self.entries.len() - reclaimable_entries >= entry_limit
        {
            return Err(Denied::Capacity);
        }
        while self.bytes > byte_limit - bytes || self.entries.len() >= entry_limit {
            let victim = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.seen < protected_since)
                .min_by_key(|(_, entry)| entry.recency)
                .map(|(key, _)| key.clone())
                .expect("reclaimable capacity checked above");
            let removed = self.entries.remove(&victim).unwrap();
            self.bytes -= removed.bytes;
        }
        self.uploads += 1;
        Ok(bytes)
    }

    fn insert(&mut self, key: Key, value: T, bytes: usize, pass: u64) {
        self.clock = self.clock.saturating_add(1);
        if let Some(previous) = self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                recency: self.clock,
                seen: pass,
            },
        ) {
            self.bytes -= previous.bytes;
        }
        self.bytes += bytes;
    }

    fn get_or_create(
        &mut self,
        key: Key,
        size: [usize; 2],
        class: TextureClass,
        pass: u64,
        max_side: usize,
        create: impl FnOnce(&Key) -> T,
    ) -> Result<T, Denied> {
        if let Some(value) = self.get(&key, pass) {
            return Ok(value);
        }
        let bytes = self.admit(size, class, pass, max_side)?;
        let value = create(&key);
        self.insert(key, value.clone(), bytes, pass);
        Ok(value)
    }
}

#[derive(Default)]
pub(crate) struct TextureCache {
    resident: Residency<TextureHandle>,
}

impl TextureCache {
    pub(crate) fn get(&mut self, ctx: &Context, key: &Key) -> Option<TextureHandle> {
        self.resident.get(key, ctx.cumulative_pass_nr())
    }

    /// `make_pixels` is called only after the byte, entry and pass budgets
    /// admit the upload, including the renderer's maximum texture side.
    pub(crate) fn get_or_create(
        &mut self,
        ctx: &Context,
        key: Key,
        size: [usize; 2],
        class: TextureClass,
        make_pixels: impl FnOnce() -> ColorImage,
    ) -> Option<TextureHandle> {
        match self.resident.get_or_create(
            key,
            size,
            class,
            ctx.cumulative_pass_nr(),
            ctx.input(|input| input.max_texture_side),
            |key| {
                ctx.load_texture(
                    format!("slate-fx-{}-{}-{}", key.0, key.1, key.2),
                    make_pixels(),
                    TextureOptions::LINEAR,
                )
            },
        ) {
            Ok(texture) => Some(texture),
            Err(Denied::UploadQuota) => {
                ctx.request_repaint();
                None
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
