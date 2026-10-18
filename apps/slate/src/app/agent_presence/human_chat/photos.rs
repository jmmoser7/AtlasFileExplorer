//! The single remote participant-portrait decode worker. Stored pictures are
//! preferences; decoding them never restores live room membership.
use super::StoredHuman;
use eframe::egui;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    io::Cursor,
};

const MAX_CONTACTS: usize = 64;
const JOBS: usize = 8;
const UPLOADS_PER_FRAME: usize = 2;

struct Job {
    id: String,
    generation: u64,
    hash: u64,
    bytes: Vec<u8>,
}
struct Decoded {
    id: String,
    generation: u64,
    hash: u64,
    image: Option<egui::ColorImage>,
}
struct Worker {
    tx: crossbeam_channel::Sender<Job>,
    rx: crossbeam_channel::Receiver<Decoded>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    New,
    Queued,
    Complete,
}
struct Entry {
    generation: u64,
    hash: u64,
    bytes: Vec<u8>,
    state: State,
    texture: Option<egui::TextureHandle>,
}
#[derive(Default)]
pub(super) struct PhotoCache {
    entries: HashMap<String, Entry>,
    generation: u64,
    worker: Option<Worker>,
    unavailable: bool,
}

impl PhotoCache {
    pub(super) fn photo(&self, id: &str) -> Option<egui::load::SizedTexture> {
        self.entries
            .get(id)?
            .texture
            .as_ref()
            .map(|t| (t.id(), t.size_vec2()).into())
    }

    pub(super) fn pump(&mut self, ctx: &egui::Context, people: &[StoredHuman]) {
        self.entries
            .retain(|id, _| people.iter().any(|p| &p.id == id));
        for p in people.iter().take(MAX_CONTACTS) {
            self.observe(&p.id, p.avatar.as_deref());
        }
        if self.entries.is_empty() {
            return;
        }
        if self.unavailable {
            return;
        }
        if self.worker.is_none() {
            match worker(ctx.clone()) {
                Ok(worker) => self.worker = Some(worker),
                Err(_) => {
                    self.unavailable = true;
                    return;
                }
            }
        }
        let worker = self.worker.as_ref().unwrap();
        for (id, entry) in &mut self.entries {
            if entry.state != State::New {
                continue;
            }
            let job = Job {
                id: id.clone(),
                generation: entry.generation,
                hash: entry.hash,
                bytes: entry.bytes.clone(),
            };
            if worker.tx.try_send(job).is_ok() {
                entry.state = State::Queued;
            } else {
                break;
            }
        }
        for result in worker.rx.try_iter().take(UPLOADS_PER_FRAME) {
            let Some(entry) = self.entries.get_mut(&result.id) else {
                continue;
            };
            if entry.generation != result.generation || entry.hash != result.hash {
                continue;
            }
            entry.state = State::Complete;
            entry.texture = result.image.map(|image| {
                ctx.load_texture(
                    format!("participant-portrait-{}", result.generation),
                    image,
                    egui::TextureOptions::LINEAR,
                )
            });
        }
        if self.entries.values().any(|e| e.state != State::Complete) || !worker.rx.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn observe(&mut self, id: &str, bytes: Option<&[u8]>) {
        let Some(bytes) =
            bytes.filter(|b| !b.is_empty() && b.len() <= atlas_relay::MAX_AVATAR_BYTES)
        else {
            self.entries.remove(id);
            return;
        };
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let hash = hasher.finish();
        if self
            .entries
            .get(id)
            .is_some_and(|e| e.hash == hash && e.bytes.as_slice() == bytes)
        {
            return;
        }
        if !self.entries.contains_key(id) && self.entries.len() >= MAX_CONTACTS {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.entries.insert(
            id.into(),
            Entry {
                generation: self.generation,
                hash,
                bytes: bytes.to_vec(),
                state: State::New,
                texture: None,
            },
        );
    }
}

fn worker(ctx: egui::Context) -> std::io::Result<Worker> {
    let (tx, jobs) = crossbeam_channel::bounded::<Job>(JOBS);
    let (results, rx) = crossbeam_channel::bounded(JOBS);
    std::thread::Builder::new()
        .name("participant-portraits".into())
        .spawn(move || {
            while let Ok(job) = jobs.recv() {
                let image = decode(&job.bytes).ok();
                let result = Decoded {
                    id: job.id,
                    generation: job.generation,
                    hash: job.hash,
                    image,
                };
                if results.send(result).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        })?;
    Ok(Worker { tx, rx })
}

fn decode(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    if bytes.is_empty() || bytes.len() > atlas_relay::MAX_AVATAR_BYTES {
        return Err("Invalid participant portrait size".into());
    }
    // Only the app's tiny JPEG format is accepted. Header limits are strict;
    // the image crate's allocation limit also applies during decoding.
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(128);
    limits.max_image_height = Some(128);
    limits.max_alloc = Some(64 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| e.to_string())?
        .resize_exact(32, 32, image::imageops::FilterType::Triangle)
        .to_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [32, 32],
        image.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(width, height, image::Rgb([72, 112, 160]));
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 60)
            .encode(
                image.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn portrait_decode_rejects_oversized_dimensions_bytes_and_invalid_images() {
        assert_eq!(decode(&jpeg(32, 32)).unwrap().size, [32, 32]);
        assert!(decode(&jpeg(129, 1)).is_err());
        assert!(decode(&vec![0; atlas_relay::MAX_AVATAR_BYTES + 1]).is_err());
        assert!(decode(b"not a JPEG").is_err());
    }

    #[test]
    fn changed_and_removed_portraits_invalidate_pending_generations() {
        let mut cache = PhotoCache::default();
        cache.observe("person", Some(&[1, 2]));
        let before = cache.entries["person"].generation;
        cache.observe("person", Some(&[1, 2]));
        assert_eq!(cache.entries["person"].generation, before);
        cache.observe("person", Some(&[2, 3]));
        assert_ne!(cache.entries["person"].generation, before);
        cache.observe("person", None);
        assert!(!cache.entries.contains_key("person"));
        cache.observe("person", Some(&[1, 2]));
        assert_ne!(cache.entries["person"].generation, before);
    }
}
