use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use slate_doc::scene::{NodeKind, Scene};
use slate_doc::{ItemId, SlateDoc};

/// Resolved asset URLs keyed by absolute source path string (deterministic order).
#[derive(Debug, Default, Clone)]
pub struct AssetMap {
    urls: std::collections::BTreeMap<String, String>,
}

impl AssetMap {
    pub fn get(&self, path: &Path) -> Option<&str> {
        let key = path_to_key(path);
        self.urls.get(&key).map(String::as_str)
    }

    pub fn insert(&mut self, path: PathBuf, url: String) {
        self.urls.insert(path_to_key(&path), url);
    }
}

pub struct AssetBuildReport {
    pub map: AssetMap,
    pub copied: usize,
    pub missing: usize,
}

/// Collects image item paths from the scene and copies or inlines each once.
pub fn build_assets(
    doc: &SlateDoc,
    out_dir: &Path,
    inline_assets: bool,
) -> io::Result<AssetBuildReport> {
    let paths = image_paths_in_scene(&doc.scene, doc);
    let assets_dir = out_dir.join("assets");
    if !inline_assets {
        fs::create_dir_all(&assets_dir)?;
    }

    let mut map = AssetMap::default();
    let mut copied = 0usize;
    let mut missing = 0usize;

    for path in paths {
        if !path.exists() {
            missing += 1;
            continue;
        }

        let key = path_to_key(&path);
        if map.urls.contains_key(&key) {
            continue;
        }

        let url = if inline_assets {
            let bytes = fs::read(&path)?;
            let mime = mime_for_path(&path);
            format!("data:{mime};base64,{}", base64_encode(&bytes))
        } else {
            let file_name = asset_file_name(&path);
            let dest = assets_dir.join(&file_name);
            fs::copy(&path, &dest)?;
            copied += 1;
            format!("assets/{file_name}")
        };

        map.urls.insert(key, url);
    }

    Ok(AssetBuildReport {
        map,
        copied,
        missing,
    })
}

fn image_paths_in_scene(scene: &Scene, doc: &SlateDoc) -> Vec<PathBuf> {
    let mut ids: BTreeSet<ItemId> = BTreeSet::new();
    for node in &scene.nodes {
        if let NodeKind::Image(img) = &node.kind {
            ids.insert(img.item);
        }
    }

    let mut paths: Vec<PathBuf> = ids
        .into_iter()
        .filter_map(|id| doc.item(id).map(|item| item.path.clone()))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn path_to_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `{stem}-{hash8}.{ext}` where hash8 is the first 8 hex chars of FNV-1a 64 of the path.
pub fn asset_file_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "asset".into());
    let ext = path
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let hash8 = format!("{:08x}", fnv1a64(&path_to_key(path)))[..8].to_string();
    if ext.is_empty() {
        format!("{stem}-{hash8}")
    } else {
        format!("{stem}-{hash8}.{ext}")
    }
}

fn fnv1a64(s: &str) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in s.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

pub fn mime_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("ogg") => "video/ogg",
        _ => "application/octet-stream",
    }
}

pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_is_deterministic() {
        let a = fnv1a64("/tmp/photo.png");
        let b = fnv1a64("/tmp/photo.png");
        assert_eq!(a, b);
        assert_ne!(a, fnv1a64("/tmp/other.png"));
    }

    #[test]
    fn asset_file_name_format() {
        let name = asset_file_name(Path::new("/photos/vacation.png"));
        assert!(name.starts_with("vacation-"));
        assert!(name.ends_with(".png"));
        assert_eq!(name.len(), "vacation-".len() + 8 + ".png".len());
    }

    #[test]
    fn base64_round_trip_length() {
        let encoded = base64_encode(b"Man");
        assert_eq!(encoded, "TWFu");
    }
}
