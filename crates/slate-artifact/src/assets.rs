use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use slate_doc::media::{media_kind, web_safe_video, MediaKind};
use slate_doc::scene::{NodeKind, Scene};
use slate_doc::{ItemId, NodeId, SlateDoc};

use crate::ExportOptions;

mod snippet;
pub use snippet::read_snippet;

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
    agent_images: BTreeMap<u64, Vec<String>>,
    agent_replies: BTreeMap<u64, String>,
    urls: BTreeMap<String, String>,
    thumbs: BTreeMap<String, String>,
    item_thumbs: BTreeMap<ItemId, String>,
    snippets: BTreeMap<String, String>,
    /// CSV / Excel grids for text cards, including authored cell fills.
    sheets: BTreeMap<String, Vec<Vec<atlas_core::office::SheetCell>>>,
    /// Frozen-camera poster URLs for 3D model nodes, keyed by node id (one
    /// placed model = one saved perspective = one poster).
    model_posters: BTreeMap<u64, String>,
    /// Packaged local web-portal material, keyed by node id.
    web_pages: BTreeMap<u64, WebAsset>,
    /// Captured web-portal posters, keyed by node id.
    web_posters: BTreeMap<u64, String>,
}

impl AssetMap {
    pub fn agent_images(&self, node: NodeId) -> Option<&[String]> {
        self.agent_images.get(&node.0).map(Vec::as_slice)
    }
    pub fn insert_agent_images(&mut self, node: NodeId, images: Vec<String>) {
        self.agent_images.insert(node.0, images);
    }
    pub fn agent_reply(&self, node: NodeId) -> Option<&str> {
        self.agent_replies.get(&node.0).map(String::as_str)
    }
    pub fn insert_agent_reply(&mut self, node: NodeId, text: String) {
        self.agent_replies.insert(node.0, text);
    }
    pub fn get(&self, path: &Path) -> Option<&str> {
        self.urls.get(&path_to_key(path)).map(String::as_str)
    }

    pub fn insert(&mut self, path: PathBuf, url: String) {
        self.urls.insert(path_to_key(&path), url);
    }

    pub fn thumb(&self, path: &Path) -> Option<&str> {
        self.thumbs.get(&path_to_key(path)).map(String::as_str)
    }

    pub fn item_thumb(&self, item: ItemId, path: &Path) -> Option<&str> {
        self.item_thumbs
            .get(&item)
            .map(String::as_str)
            .or_else(|| self.thumb(path))
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

    pub fn sheet(&self, path: &Path) -> Option<&[Vec<atlas_core::office::SheetCell>]> {
        self.sheets.get(&path_to_key(path)).map(Vec::as_slice)
    }

    pub fn model_poster(&self, node: NodeId) -> Option<&str> {
        self.model_posters.get(&node.0).map(String::as_str)
    }

    pub fn insert_model_poster(&mut self, node: NodeId, url: String) {
        self.model_posters.insert(node.0, url);
    }

    /// The packaged copy of a local web portal's material, and where it came
    /// from.
    pub fn web_page(&self, node: NodeId) -> Option<&WebAsset> {
        self.web_pages.get(&node.0)
    }

    pub fn web_poster(&self, node: NodeId) -> Option<&str> {
        self.web_posters.get(&node.0).map(String::as_str)
    }
}

/// A local page copied beside the artifact. `origin` is the path it was forked
/// from, which the caption states rather than quietly dropping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebAsset {
    pub url: String,
    pub origin: String,
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
            atlas_core::workbook_assets::require_local_source(path)?;
            if !*assets_dir_ready {
                fs::create_dir_all(&assets_dir)?;
                *assets_dir_ready = true;
            }
            let file_name = asset_file_name(path);
            atlas_core::workbook_assets::copy_file(path, &assets_dir.join(&file_name))?;
            *copied += 1;
            Ok(format!("assets/{file_name}"))
        };

    for (item_id, path) in items {
        if !path.exists() {
            missing += 1;
            continue;
        }
        let key = path_to_key(&path);
        let already_copied = map.urls.contains_key(&key);

        let kind = media_kind(&path);
        if !already_copied {
            let url = match kind {
                MediaKind::Image if opts.inline_assets => data_uri(&path)?,
                MediaKind::Image => copy_file(&path, &mut assets_dir_ready, &mut copied)?,
                // Videos and originals-behind-cards are always real files.
                _ => copy_file(&path, &mut assets_dir_ready, &mut copied)?,
            };
            map.urls.insert(key.clone(), url);
        }

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
                    map.item_thumbs.insert(item_id, url.clone());
                    map.thumbs.entry(key.clone()).or_insert(url);
                }
            }
        }

        if kind == MediaKind::Text {
            if let Some(snippet) = read_snippet(&path) {
                map.snippets.insert(key.clone(), snippet);
            }
            if let Some(sheet) = atlas_core::table::read_sheet_card(&path) {
                map.sheets.insert(key, sheet);
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

    // Generated results are linked images, packaged through the same asset writer.
    for (id, images) in &opts.agent_images {
        let mut urls = Vec::new();
        for path in images {
            if !path.is_file() {
                missing += 1;
                continue;
            }
            let url = if let Some(url) = map.get(path) {
                url.to_string()
            } else {
                let url = if opts.inline_assets {
                    data_uri(path)?
                } else {
                    copy_file(path, &mut assets_dir_ready, &mut copied)?
                };
                map.insert(path.clone(), url.clone());
                url
            };
            urls.push(url);
        }
        map.insert_agent_images(*id, urls);
    }
    for (id, text) in &opts.agent_replies {
        map.insert_agent_reply(*id, text.clone());
    }

    // Web portals. Local material is copied whole — a dashboard is its entry
    // file plus the scripts and data beside it — and the copy records its
    // origin (Art. IX.4). Remote pages are never fetched at export time; they
    // get their captured poster and a link (Art. V.3).
    for node in &doc.scene.nodes {
        let NodeKind::Portal(portal) = &node.kind else {
            continue;
        };
        if portal.kind != slate_doc::scene::PortalKind::Web {
            continue;
        }
        if let Some(source) = opts.web_sources.get(&node.id) {
            if atlas_core::secrets::is_machine_private(source) {
                // Sign-in state stays on this machine. The portal exports as
                // a poster when the caller supplied one.
                missing += 1;
            } else if source.exists() {
                let dir_name = format!("web-{}", node.id.0);
                let dest = assets_dir.join(&dir_name);
                let entry = if source.is_dir() {
                    let entry = portal.web_ref().entry;
                    copied += atlas_core::workbook_assets::copy_tree(source, &dest)?;
                    format!("assets/{dir_name}/{entry}")
                } else {
                    let file_name = asset_file_name(source);
                    atlas_core::workbook_assets::copy_file(source, &dest.join(&file_name))?;
                    copied += 1;
                    format!("assets/{dir_name}/{file_name}")
                };
                assets_dir_ready = true;
                map.web_pages.insert(
                    node.id.0,
                    WebAsset {
                        url: entry,
                        origin: source.to_string_lossy().into_owned(),
                    },
                );
            } else {
                missing += 1;
            }
        }
        if let Some(poster) = opts.web_posters.get(&node.id) {
            if poster.exists() {
                let url = if opts.inline_assets {
                    data_uri(poster)?
                } else {
                    copy_file(poster, &mut assets_dir_ready, &mut copied)?
                };
                map.web_posters.insert(node.id.0, url);
            }
        }
    }

    Ok(AssetBuildReport {
        map,
        copied,
        missing,
    })
}

fn data_uri(path: &Path) -> io::Result<String> {
    atlas_core::workbook_assets::require_local_source(path)?;
    let bytes = fs::read(path)?;
    let mime = mime_for_path(path);
    Ok(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// Unique `(item, path)` pairs placed on the board, in deterministic order.
fn placed_items(scene: &Scene, doc: &SlateDoc) -> Vec<(ItemId, PathBuf)> {
    let mut ids: BTreeSet<ItemId> = BTreeSet::new();
    for node in &scene.nodes {
        if let NodeKind::Image(img) = &node.kind {
            ids.insert(img.item);
        }
    }

    let mut out: Vec<(ItemId, PathBuf)> = Vec::new();
    for id in ids {
        if let Some(item) = doc.item(id) {
            out.push((id, item.path.clone()));
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
    use slate_doc::scene::{NodeKind, PortalNode, SceneCmd, WorldRect};

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
    fn office_excerpt_extensions_are_structured_text() {
        for ext in atlas_core::office::EXCERPT_EXTENSIONS {
            let path = PathBuf::from(format!("file.{ext}"));
            assert!(
                slate_doc::media::structured_text_package(&path),
                "{ext} would fall through to raw bytes"
            );
            assert_eq!(media_kind(&path), MediaKind::Text, "{ext}");
        }
    }

    #[test]
    fn packaging_skips_a_portal_aimed_at_the_webview_folder() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("slate-priv-{nanos}"));
        let page = root.join("NativeFileAtlas").join("webview2");
        fs::create_dir_all(&page).unwrap();
        fs::write(page.join("index.html"), b"<p>session-cookie</p>").unwrap();

        let mut doc = SlateDoc::default();
        let node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 320.0, 180.0),
            NodeKind::Portal(PortalNode::bound_web("Bank", "https://bank.example/")),
        );
        let id = node.id;
        doc.scene.apply(&SceneCmd::Add { index: 0, node });
        let out = std::env::temp_dir().join(format!("slate-priv-out-{nanos}"));
        let _ = fs::remove_dir_all(&out);
        fs::create_dir_all(&out).unwrap();
        let mut opts = ExportOptions::default();
        opts.web_sources.insert(id, page.clone());
        let report = build_assets(&doc, &out, &opts).unwrap();
        assert_eq!(report.copied, 0);
        assert!(report.missing >= 1);
        assert!(report.map.web_page(id).is_none());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&out);
    }
}
