use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use slate_doc::media::{media_kind, web_safe_video, MediaKind};
use slate_doc::scene::{NodeKind, Scene};
use slate_doc::{ItemId, NodeId, SlateDoc};

use crate::ExportOptions;

/// Max characters excerpted from a text file into its snippet card.
const SNIPPET_MAX_CHARS: usize = 1200;
/// Max lines excerpted from a text file into its snippet card.
const SNIPPET_MAX_LINES: usize = 30;

/// Resolved asset URLs keyed by absolute source path string (deterministic
/// order). Besides the primary URL per item path, carries thumbnail URLs
/// (poster images for document/PDF/video cards) and text snippets, so
/// `render_html` stays a pure function of `(doc, assets)`.
#[derive(Debug, Default, Clone)]
pub struct AssetMap {
    urls: BTreeMap<String, String>,
    thumbs: BTreeMap<String, String>,
    snippets: BTreeMap<String, String>,
    /// Frozen-camera poster URLs for 3D model nodes, keyed by node id (one
    /// placed model = one saved perspective = one poster).
    model_posters: BTreeMap<u64, String>,
}

impl AssetMap {
    pub fn get(&self, path: &Path) -> Option<&str> {
        self.urls.get(&path_to_key(path)).map(String::as_str)
    }

    pub fn insert(&mut self, path: PathBuf, url: String) {
        self.urls.insert(path_to_key(&path), url);
    }

    pub fn thumb(&self, path: &Path) -> Option<&str> {
        self.thumbs.get(&path_to_key(path)).map(String::as_str)
    }

    pub fn insert_thumb(&mut self, path: PathBuf, url: String) {
        self.thumbs.insert(path_to_key(&path), url);
    }

    pub fn snippet(&self, path: &Path) -> Option<&str> {
        self.snippets.get(&path_to_key(path)).map(String::as_str)
    }

    pub fn insert_snippet(&mut self, path: PathBuf, text: String) {
        self.snippets.insert(path_to_key(&path), text);
    }

    pub fn model_poster(&self, node: NodeId) -> Option<&str> {
        self.model_posters.get(&node.0).map(String::as_str)
    }

    pub fn insert_model_poster(&mut self, node: NodeId, url: String) {
        self.model_posters.insert(node.0, url);
    }
}

pub struct AssetBuildReport {
    pub map: AssetMap,
    pub copied: usize,
    pub missing: usize,
}

/// Collects placed item paths from the scene and prepares each once,
/// according to its media kind:
///
/// - **Images** are copied (or inlined as data URIs).
/// - **Web-safe videos** are always copied — inlining video as base64 is
///   never reasonable.
/// - **Everything else** (PDF / docs / text / non-web-safe video) has the
///   original copied so the card can link to it, plus a poster thumbnail
///   (when the caller supplied one via [`ExportOptions::thumbs`]) and, for
///   text files, an excerpt for the snippet card.
pub fn build_assets(
    doc: &SlateDoc,
    out_dir: &Path,
    opts: &ExportOptions,
) -> io::Result<AssetBuildReport> {
    let items = placed_items(&doc.scene, doc);
    let assets_dir = out_dir.join("assets");
    let mut assets_dir_ready = false;

    let mut map = AssetMap::default();
    let mut copied = 0usize;
    let mut missing = 0usize;

    let copy_file =
        |path: &Path, assets_dir_ready: &mut bool, copied: &mut usize| -> io::Result<String> {
            if !*assets_dir_ready {
                fs::create_dir_all(&assets_dir)?;
                *assets_dir_ready = true;
            }
            let file_name = asset_file_name(path);
            fs::copy(path, assets_dir.join(&file_name))?;
            *copied += 1;
            Ok(format!("assets/{file_name}"))
        };

    for (item_id, path) in items {
        if !path.exists() {
            missing += 1;
            continue;
        }
        let key = path_to_key(&path);
        if map.urls.contains_key(&key) {
            continue;
        }

        let kind = media_kind(&path);
        let url = match kind {
            MediaKind::Image if opts.inline_assets => data_uri(&path)?,
            MediaKind::Image => copy_file(&path, &mut assets_dir_ready, &mut copied)?,
            // Videos and originals-behind-cards are always real files.
            _ => copy_file(&path, &mut assets_dir_ready, &mut copied)?,
        };
        map.urls.insert(key.clone(), url);

        // Everything that isn't an inline <img> or a playing <video> renders
        // as a card; cards and videos both benefit from a poster thumbnail.
        let renders_inline =
            kind == MediaKind::Image || (kind == MediaKind::Video && web_safe_video(&path));
        let wants_poster = !renders_inline || kind == MediaKind::Video;
        if wants_poster {
            if let Some(thumb_path) = opts.thumbs.get(&item_id) {
                if thumb_path.exists() {
                    let url = if opts.inline_assets {
                        data_uri(thumb_path)?
                    } else {
                        copy_file(thumb_path, &mut assets_dir_ready, &mut copied)?
                    };
                    map.thumbs.insert(key.clone(), url);
                }
            }
        }

        if kind == MediaKind::Text {
            if let Some(snippet) = read_snippet(&path) {
                map.snippets.insert(key, snippet);
            }
        }
    }

    // Frozen-camera posters for 3D model nodes (per node, not per item —
    // duplicated viewports carry distinct saved perspectives).
    for node in &doc.scene.nodes {
        let NodeKind::Image(img) = &node.kind else {
            continue;
        };
        let Some(item) = doc.item(img.item) else {
            continue;
        };
        if media_kind(&item.path) != MediaKind::Model {
            continue;
        }
        let Some(poster) = opts.model_posters.get(&node.id) else {
            continue;
        };
        if !poster.exists() {
            continue;
        }
        let url = if opts.inline_assets {
            data_uri(poster)?
        } else {
            copy_file(poster, &mut assets_dir_ready, &mut copied)?
        };
        map.model_posters.insert(node.id.0, url);
    }

    Ok(AssetBuildReport {
        map,
        copied,
        missing,
    })
}

fn data_uri(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    let mime = mime_for_path(path);
    Ok(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// First ~[`SNIPPET_MAX_CHARS`] chars / [`SNIPPET_MAX_LINES`] lines of a text
/// file, lossy-decoded. `None` if unreadable or empty. Public so the live
/// board renders the *same* excerpt the artifact will.
pub fn read_snippet(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    // Read a bounded prefix; 4x chars is enough for any UTF-8 encoding.
    let prefix_len = bytes.len().min(SNIPPET_MAX_CHARS * 4);
    let text = String::from_utf8_lossy(&bytes[..prefix_len]);
    let mut out = String::new();
    let mut chars = 0usize;
    let mut lines = 1usize;
    for ch in text.chars() {
        if ch == '\n' {
            lines += 1;
            if lines > SNIPPET_MAX_LINES {
                break;
            }
        }
        out.push(ch);
        chars += 1;
        if chars >= SNIPPET_MAX_CHARS {
            break;
        }
    }
    let out = out.trim_end().to_string();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Unique `(item, path)` pairs placed on the board, in deterministic order.
fn placed_items(scene: &Scene, doc: &SlateDoc) -> Vec<(ItemId, PathBuf)> {
    let mut ids: BTreeSet<ItemId> = BTreeSet::new();
    for node in &scene.nodes {
        if let NodeKind::Image(img) = &node.kind {
            ids.insert(img.item);
        }
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<(ItemId, PathBuf)> = Vec::new();
    for id in ids {
        if let Some(item) = doc.item(id) {
            if seen.insert(path_to_key(&item.path)) {
                out.push((id, item.path.clone()));
            }
        }
    }
    out
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
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("ogg") | Some("ogv") => "video/ogg",
        Some("pdf") => "application/pdf",
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

    #[test]
    fn snippet_clamps_chars_and_lines() {
        let dir = std::env::temp_dir().join(format!(
            "slate-snippet-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let long = dir.join("long.txt");
        fs::write(&long, "x".repeat(SNIPPET_MAX_CHARS * 3)).unwrap();
        let s = read_snippet(&long).unwrap();
        assert_eq!(s.chars().count(), SNIPPET_MAX_CHARS);

        let many_lines = dir.join("lines.txt");
        fs::write(&many_lines, "line\n".repeat(200)).unwrap();
        let s = read_snippet(&many_lines).unwrap();
        assert!(s.lines().count() <= SNIPPET_MAX_LINES);

        let empty = dir.join("empty.txt");
        fs::write(&empty, "  \n ").unwrap();
        assert!(read_snippet(&empty).is_none());

        let _ = fs::remove_dir_all(dir);
    }
}
