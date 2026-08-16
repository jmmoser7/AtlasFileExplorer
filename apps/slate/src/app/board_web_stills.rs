//! Card pictures that survive closing the workbook.
//!
//! A board of web portals opens blank and fills itself by *running the pages* —
//! a browser per card, a few seconds each, one after another. Driving the
//! reference 58-portal board proved what that costs: a full minute of reading
//! and 22 cards had still never shown anything, while the browsers doing the
//! filling were themselves the reason the board felt heavy.
//!
//! Almost none of that work needed doing twice. A card's picture is the same
//! picture it was in the last session, so this keeps it on disk, keyed by the
//! page it came from, and hands it back in milliseconds the next time the
//! workbook opens. The board is then whole on arrival, and the live pool spends
//! its browsers on the page the reader is actually looking at.
//!
//! Everything here is **derived state** (Art. VI): the pixels are never
//! journaled, never enter the `.slate`, and losing the whole cache costs
//! nothing but the warming it saves. And it stays honest (Art. IV, D30) — an
//! entry's age is read from the file and the card says it, so a picture from
//! Tuesday reads `poster · 3d` rather than pretending to be the live page.

use eframe::egui;
use slate_doc::scene::NodeId;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Bump when the stored form changes, exactly as `atlas_core::thumbs` does with
/// its cache key: entries are keyed by content, so an entry written by an older
/// and worse pipeline would otherwise be served for ever.
const VERSION: &str = "1";

/// JPEG quality. These are 640-px-wide pictures of web pages painted at card
/// size; the difference between 80 and 95 is invisible and doubles the file.
const QUALITY: u8 = 80;

/// What the cache may occupy before the oldest entries are dropped. At roughly
/// 40 KB an entry this is thousands of cards — far more than any board — and it
/// exists so that a cache can never grow without bound.
const BUDGET_BYTES: u64 = 128 * 1024 * 1024;

/// A picture that came back from disk.
pub struct Loaded {
    pub id: NodeId,
    /// The page it is a picture of. Carried back so a portal rebound while the
    /// read was in flight cannot be handed the previous page's picture.
    pub key: String,
    /// How old the pixels are, so the card can say so.
    pub age: Duration,
    pub img: Arc<egui::ColorImage>,
}

enum Job {
    Load {
        id: NodeId,
        key: String,
    },
    Save {
        key: String,
        img: Arc<egui::ColorImage>,
    },
}

/// The disk cache, and the one worker thread that touches it.
///
/// One thread on purpose. Decoding a card is about two milliseconds, so even a
/// wall of them is a fraction of a second, and the alternative — a pool racing
/// to fill a board — competes with the browsers for exactly the cores that were
/// the problem in the first place.
pub struct StillCache {
    dir: PathBuf,
    jobs: crossbeam_channel::Sender<Job>,
    done: crossbeam_channel::Receiver<Loaded>,
    /// Keys asked for this session, so a card is fetched once however many
    /// frames it takes to arrive.
    asked: HashSet<String>,
    /// Keys written this session. A page that recaptures five times a second
    /// must not write five files a second.
    stored: HashSet<String>,
    hits: usize,
    /// A cache that keeps nothing. Every board in a test process would
    /// otherwise share one folder on disk, and a picture one test captured
    /// would arrive in another's board as if that board had earned it — which
    /// makes the second test measure something that never happened.
    off: bool,
}

impl Default for StillCache {
    fn default() -> Self {
        match default_dir() {
            Some(dir) => Self::new(dir),
            None => Self::disabled(),
        }
    }
}

/// Where cards are kept, or `None` for a cache that keeps nothing.
///
/// `SLATE_STILL_CACHE` names the folder, which is how the live board probe
/// measures a *second* session. Under `cargo test` there is no folder unless
/// that variable says so: a test that wants persistence asks for it by handing
/// its board a cache of its own.
fn default_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SLATE_STILL_CACHE") {
        return Some(PathBuf::from(dir));
    }
    if cfg!(test) {
        return None;
    }
    Some(atlas_core::index::data_dir().join("web-stills"))
}

impl StillCache {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let (jobs, job_rx) = crossbeam_channel::unbounded::<Job>();
        let (done_tx, done) = crossbeam_channel::unbounded();
        let worker_dir = dir.clone();
        std::thread::Builder::new()
            .name("slate-web-stills".into())
            .spawn(move || {
                prune(&worker_dir);
                while let Ok(job) = job_rx.recv() {
                    match job {
                        Job::Load { id, key } => {
                            if let Some((img, age)) = read(&worker_dir, &key) {
                                let _ = done_tx.send(Loaded { id, key, age, img });
                            }
                        }
                        Job::Save { key, img } => write(&worker_dir, &key, &img),
                    }
                }
            })
            .ok();
        StillCache {
            dir,
            jobs,
            done,
            asked: HashSet::new(),
            stored: HashSet::new(),
            hits: 0,
            off: false,
        }
    }

    /// A cache that answers nothing and keeps nothing. No folder, no worker.
    pub fn disabled() -> Self {
        let (jobs, _) = crossbeam_channel::unbounded::<Job>();
        let (_, done) = crossbeam_channel::unbounded();
        StillCache {
            dir: PathBuf::new(),
            jobs,
            done,
            asked: HashSet::new(),
            stored: HashSet::new(),
            hits: 0,
            off: true,
        }
    }

    /// The cache key for a page. Public because the caller keeps it on the view:
    /// the same string answers both "is there a picture for this?" and "where
    /// does this page's picture go?".
    pub fn key(target: &str) -> String {
        // Two independent FNV-1a passes → a 128-bit key, as `atlas_core::thumbs`
        // does. Collisions here would show one page's picture on another card.
        let s = format!("{target}|{VERSION}");
        format!(
            "{:016x}{:016x}",
            fnv64(s.as_bytes(), 0xcbf2_9ce4_8422_2325),
            fnv64(s.as_bytes(), 0x9e37_79b9_7f4a_7c15)
        )
    }

    /// Ask for a card's stored picture, at most once per key per session.
    pub fn request(&mut self, id: NodeId, key: &str) {
        if self.off || !self.asked.insert(key.to_owned()) {
            return;
        }
        let _ = self.jobs.send(Job::Load {
            id,
            key: key.to_owned(),
        });
    }

    /// Pictures that have arrived since the last call. Never blocks.
    pub fn drain(&mut self) -> Vec<Loaded> {
        let got: Vec<Loaded> = self.done.try_iter().collect();
        self.hits += got.len();
        got
    }

    /// Cards handed back from disk this session — the number that says whether
    /// a board arrived whole or had to run its pages to find out.
    pub fn hits(&self) -> usize {
        self.hits
    }

    /// Keep this picture for next time.
    ///
    /// Once per key per session: a live page hands back a new frame five times a
    /// second, and the value of a stored still is that it exists at all, not
    /// that it is seconds fresh. A session that opens the board again writes the
    /// entry again, so a page that changed does not stay wrong for ever.
    pub fn store(&mut self, key: &str, img: Arc<egui::ColorImage>) {
        if self.off || !self.stored.insert(key.to_owned()) {
            return;
        }
        let _ = self.jobs.send(Job::Save {
            key: key.to_owned(),
            img,
        });
    }

    /// Forget an entry, for a card whose page turned out not to load. Cheap
    /// enough to do inline: one `remove_file`.
    pub fn forget(&mut self, key: &str) {
        self.asked.remove(key);
        self.stored.remove(key);
        let _ = std::fs::remove_file(self.path(key));
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.jpg"))
    }
}

fn entry_path(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{key}.jpg"))
}

fn read(dir: &Path, key: &str) -> Option<(Arc<egui::ColorImage>, Duration)> {
    let path = entry_path(dir, key);
    let meta = std::fs::metadata(&path).ok()?;
    let age = meta
        .modified()
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .unwrap_or_default();
    let bytes = std::fs::read(&path).ok()?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some((
        Arc::new(egui::ColorImage::from_rgba_unmultiplied(
            size,
            rgba.as_raw(),
        )),
        age,
    ))
}

fn write(dir: &Path, key: &str, img: &egui::ColorImage) {
    let (w, h) = (img.size[0] as u32, img.size[1] as u32);
    if w == 0 || h == 0 {
        return;
    }
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in &img.pixels {
        rgb.extend_from_slice(&[px.r(), px.g(), px.b()]);
    }
    let Some(buf) = image::RgbImage::from_raw(w, h, rgb) else {
        return;
    };
    // Written beside and renamed, so a half-written file is never read back as a
    // corrupt card — the same discipline the index uses.
    let tmp = dir.join(format!("{key}.tmp"));
    let out = std::fs::File::create(&tmp);
    let Ok(file) = out else { return };
    let mut w = std::io::BufWriter::new(file);
    let ok = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, QUALITY)
        .encode_image(&buf)
        .is_ok();
    drop(w);
    if ok {
        let _ = std::fs::rename(&tmp, entry_path(dir, key));
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Drop the oldest entries until the cache is inside its budget. Runs once, on
/// the worker, before any load — a directory listing, never on the frame loop.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect();
    let total: u64 = files.iter().map(|(_, len, _)| len).sum();
    if total <= BUDGET_BYTES {
        return;
    }
    files.sort_by_key(|(t, _, _)| *t);
    let mut over = total - BUDGET_BYTES;
    for (_, len, path) in files {
        if over == 0 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            over = over.saturating_sub(len);
        }
    }
}

fn fnv64(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = seed;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: usize, h: usize, shade: u8) -> Arc<egui::ColorImage> {
        Arc::new(egui::ColorImage::new(
            [w, h],
            egui::Color32::from_gray(shade),
        ))
    }

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("slate-stills-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The whole point: what one session captured, the next session paints
    /// without running a browser at all.
    #[test]
    fn a_card_kept_from_one_session_is_there_for_the_next() {
        let dir = temp("roundtrip");
        let key = StillCache::key("https://example.com/dash");
        {
            let mut cache = StillCache::new(dir.clone());
            cache.store(&key, img(64, 36, 200));
            // The worker owns the write; wait for the file rather than sleeping
            // on a guess.
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !entry_path(&dir, &key).exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let mut next = StillCache::new(dir.clone());
        next.request(NodeId(1), &key);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut got = Vec::new();
        while got.is_empty() && std::time::Instant::now() < deadline {
            got = next.drain();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(got.len(), 1, "the stored card came back");
        assert_eq!(got[0].id, NodeId(1));
        assert_eq!(got[0].img.size, [64, 36]);
        let px = got[0].img.pixels[0];
        assert!(
            px.r().abs_diff(200) < 4,
            "the picture survived the round trip, got {}",
            px.r()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Two pages must never share a card, and one page must always find its own.
    #[test]
    fn a_key_follows_the_page_and_nothing_else() {
        let a = StillCache::key("https://example.com/a");
        let b = StillCache::key("https://example.com/b");
        assert_ne!(a, b);
        assert_eq!(a, StillCache::key("https://example.com/a"));
        assert_eq!(a.len(), 32);
    }

    /// A live page recaptures several times a second; the disk must not.
    #[test]
    fn a_page_that_recaptures_all_day_is_written_once() {
        let dir = temp("once");
        let mut cache = StillCache::new(dir.clone());
        let key = StillCache::key("https://example.com/busy");
        for shade in 0..20u8 {
            cache.store(&key, img(8, 8, shade));
        }
        assert_eq!(cache.stored.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
