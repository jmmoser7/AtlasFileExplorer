//! A placed image supplies its resolved file path to a Grasshopper text input.
use super::*;
use slate_doc::media::{media_kind, MediaKind};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub(in crate::app::grasshopper) struct Stamp {
    status: Option<(u64, std::time::SystemTime)>,
    pub(super) digest: Option<u64>,
}
pub(in crate::app::grasshopper) type Revisions = HashMap<PathBuf, Stamp>;

pub(in crate::app::grasshopper) fn changed(previous: &Revisions, fresh: &Revisions) -> bool {
    fresh.iter().any(|(path, stamp)| {
        previous
            .get(path)
            .is_none_or(|old| old.digest != stamp.digest)
    })
}

pub(in crate::app::grasshopper) fn inspect_revisions(
    paths: &HashSet<PathBuf>,
    previous: &Revisions,
    cancelled: &impl Fn() -> bool,
) -> Revisions {
    paths
        .iter()
        .take_while(|_| !cancelled())
        .map(|path| {
            let status = if atlas_core::cloud::is_dehydrated(path) {
                None
            } else {
                std::fs::metadata(path)
                    .ok()
                    .and_then(|m| Some((m.len(), m.modified().ok()?)))
            };
            let prior = previous
                .get(path)
                .filter(|old| old.status == status && (old.digest.is_some() || status.is_none()));
            let stamp = prior.cloned().unwrap_or_else(|| Stamp {
                status,
                digest: status.and_then(|_| content_digest(path, cancelled).ok()),
            });
            (path.clone(), stamp)
        })
        .collect()
}

fn content_digest(path: &std::path::Path, cancelled: &impl Fn() -> bool) -> std::io::Result<u64> {
    use std::hash::Hasher;
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let mut buf = [0_u8; 65536];
    loop {
        if cancelled() {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.write(&buf[..n]);
    }
    Ok(hash.finish())
}

pub(in crate::app::grasshopper) fn accept_revisions(
    current: &mut Revisions,
    fresh: Revisions,
    live: &HashSet<PathBuf>,
) -> bool {
    current.retain(|path, _| live.contains(path));
    let mut changed = false;
    for (path, stamp) in fresh {
        if live.contains(&path) {
            changed |= current.get(&path).is_none_or(|p| p.digest != stamp.digest);
            current.insert(path, stamp);
        }
    }
    changed
}

/// The card and its connected definition must use the same image revision.
/// Only refresh derived cache metadata when the background watcher reports a change.
pub(in crate::app::grasshopper) fn refresh_card_previews(
    tab: &mut SlateTab,
    revisions: &Revisions,
) {
    if revisions.is_empty() {
        return;
    }
    for item in &mut tab.doc.items {
        if media_kind(&item.path) != MediaKind::Image {
            continue;
        }
        let path =
            slate_doc::scene::resolve_source(tab.path.as_deref(), &item.path.to_string_lossy());
        let Some(stamp) = revisions.get(&path) else {
            continue;
        };
        let (Some(digest), Some((size, modified))) = (stamp.digest, stamp.status) else {
            continue;
        };
        item.size = size;
        item.mtime = modified
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |age| age.as_secs() as i64);
        // Files of the same size can be replaced several times in one second.
        let revision = format!("{}|content-{digest:016x}", path.display());
        item.cache_key = atlas_core::thumbs::cache_key(&revision, size, item.mtime);
    }
}

pub(super) fn file_port() -> &'static GrasshopperPort {
    static IMAGE: OnceLock<GrasshopperPort> = OnceLock::new();
    IMAGE.get_or_init(|| GrasshopperPort {
        id: "f2ca3d77-27e4-42b2-8771-814396130003"
            .parse()
            .expect("image port UUID"),
        name: "Image file".into(),
        direction: GrasshopperPortDirection::Output,
        kind: GrasshopperPortType::Text,
        access: GrasshopperPortAccess::Item,
    })
}

pub(super) fn source_path(tab: &SlateTab, id: NodeId) -> Option<PathBuf> {
    let NodeKind::Image(image) = &tab.doc.scene.node(id)?.kind else {
        return None;
    };
    let item = tab.doc.item(image.item)?;
    (media_kind(&item.path) == MediaKind::Image).then(|| {
        slate_doc::scene::resolve_source(tab.path.as_deref(), &item.path.to_string_lossy())
    })
}

impl SlateApp {
    pub(super) fn is_grasshopper_image_source(&self, id: NodeId) -> bool {
        self.image_item(id)
            .and_then(|i| self.doc().item(i))
            .is_some_and(|i| media_kind(&i.path) == MediaKind::Image)
    }

    pub(super) fn grasshopper_image_packet(
        &self,
        wire: &GrasshopperWire,
        target: Option<&GrasshopperPort>,
    ) -> Result<serde_json::Value, String> {
        if wire.source_port != file_port().id
            || target.is_some_and(|t| !t.kind.accepts(GrasshopperPortType::Text))
        {
            return Err("Connect the image file output to a Grasshopper text input.".into());
        }
        let path = self
            .image_item_path(wire.source)
            .ok_or("Connected image has no linked file.")?;
        let mut packet = literal_packet(
            &GrasshopperValue::Text(path.to_string_lossy().into_owned()),
            GrasshopperPortType::Text,
        )
        .ok_or("Cannot resolve the connected image file.")?;
        let stamp = self
            .grasshopper
            .image_revisions
            .get(&path)
            .ok_or("Reading the connected image file…")?;
        let digest = stamp.digest.ok_or(
            "Connected image is unavailable locally. Download it or relink it to resume the solve.",
        )?;
        packet["source_revision"] = format!("{digest:016x}").into();
        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use std::time::{Duration, Instant, UNIX_EPOCH};

    #[test]
    fn a_replaced_linked_image_updates_its_card_pixels_without_an_authored_edit() {
        use ::image::codecs::png::{CompressionType, FilterType, PngEncoder};
        use ::image::{ExtendedColorType, ImageEncoder};
        let mut h = Harness::new("linked-image-card-revision");
        let path = h.base.join("field.png");
        let draw = |value, millis: u64| {
            let file = std::fs::File::create(&path).unwrap();
            PngEncoder::new_with_quality(file, CompressionType::Uncompressed, FilterType::NoFilter)
                .write_image(&[value; 32 * 24], 32, 24, ExtendedColorType::L8)
                .unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(UNIX_EPOCH + Duration::from_millis(1_700_000_000_000 + millis))
                .unwrap();
        };
        draw(0, 100);
        let id = h.app.add_paths(std::slice::from_ref(&path))[0];
        h.app.tab_mut().path = Some(h.base.join("timber.slate"));
        h.app.tab_mut().doc.items[0].path = PathBuf::from("field.png");
        h.app.tab_mut().dirty = false;
        let paths = HashSet::from([path.clone()]);
        let mut revisions = Revisions::new();
        let mut keys = Vec::new();
        for (value, millis) in [(0, 100), (255, 800)] {
            draw(value, millis);
            revisions = inspect_revisions(&paths, &revisions, &|| false);
            refresh_card_previews(h.app.tab_mut(), &revisions);
            let item = h.app.doc().item(id).unwrap();
            assert_eq!(item.mtime, 1_700_000_000);
            assert!(!h.app.tab().dirty);
            let key = item.cache_key.clone();
            h.app.request_thumb(id);
            let deadline = Instant::now() + Duration::from_secs(30);
            while !h.app.thumb_pixels.contains_key(&key) {
                h.app.drain_thumbs(&h.ctx);
                assert!(Instant::now() < deadline, "missing thumbnail for {value}");
                std::thread::sleep(Duration::from_millis(10));
            }
            let pixels = &h.app.thumb_pixels[&key].pixels;
            assert_eq!(pixels[pixels.len() / 2].r(), value);
            keys.push(key);
        }
        assert_ne!(keys[0], keys[1]);
    }
}
