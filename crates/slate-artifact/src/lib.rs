//! HTML artifact writer for Slate boards.
//!
//! The native output format of a Slate presentation is HTML+CSS (+ a tiny
//! self-contained JS runtime for slide navigation). Because the scene model
//! in `slate-doc` is constrained to CSS-expressible styling, this crate is a
//! *serializer*, not a converter — the artifact shows exactly what the board
//! shows. PDF / PPT exports are future downstream conversions of this HTML.

mod assets;
mod render;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use slate_doc::{ItemId, SlateDoc};

pub use assets::{read_snippet, AssetMap};
pub use render::render_html;

/// Options controlling how the HTML artifact is written to disk.
#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// Inline image assets as base64 data URIs instead of copying to assets/.
    /// Videos, documents, and other card-backed originals are always copied.
    pub inline_assets: bool,
    /// Pre-rendered poster thumbnails per item (PDF pages, doc previews,
    /// video posters), supplied by the app from its shared thumbnail cache.
    /// Best effort: items without an entry fall back to a labeled card.
    pub thumbs: BTreeMap<ItemId, PathBuf>,
}

/// Summary returned after a successful export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    pub html_path: PathBuf,
    pub slides: usize,
    pub assets_copied: usize,
    pub missing_assets: usize,
}

/// Writes `out_dir/index.html` for `doc`, copying or inlining linked assets.
pub fn export_html(
    doc: &SlateDoc,
    out_dir: &Path,
    opts: &ExportOptions,
) -> io::Result<ExportReport> {
    fs::create_dir_all(out_dir)?;

    let asset_report = assets::build_assets(doc, out_dir, opts)?;
    let html = render_html(doc, &asset_report.map);

    let html_path = out_dir.join("index.html");
    fs::write(&html_path, &html)?;

    let slides = slide_count(&doc.scene);

    Ok(ExportReport {
        html_path,
        slides,
        assets_copied: asset_report.copied,
        missing_assets: asset_report.missing,
    })
}

fn slide_count(scene: &slate_doc::scene::Scene) -> usize {
    let frames = scene.frames_in_order();
    if !frames.is_empty() {
        return frames.len();
    }
    if scene.nodes.iter().any(|n| !n.is_frame()) {
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use slate_doc::scene::{
        Corner, Crop, Dash, FrameNode, ImageAdjust, ImageNode, NodeId, NodeKind, Rgba, Scene,
        SceneCmd, Stroke, TextNode, WorldRect,
    };
    use slate_doc::{ItemId, SlateDoc};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{nanos}-{n}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn add_frame(scene: &mut Scene, order: u32, rect: WorldRect) -> NodeId {
        let node = scene.build_node(
            rect,
            NodeKind::Frame(FrameNode {
                title: format!("Slide {order}"),
                order,
                fill: Rgba::WHITE,
                assignments: BTreeMap::new(),
            }),
        );
        let id = node.id;
        let index = scene.nodes.len();
        scene.apply(&SceneCmd::Add { index, node });
        id
    }

    fn add_image(
        scene: &mut Scene,
        rect: WorldRect,
        item: ItemId,
        crop: Crop,
        corner: Corner,
        stroke: Stroke,
        adjust: ImageAdjust,
    ) -> NodeId {
        let node = scene.build_node(
            rect,
            NodeKind::Image(ImageNode {
                item,
                crop,
                corner,
                stroke,
                adjust,
                video: Default::default(),
            }),
        );
        let id = node.id;
        let index = scene.nodes.len();
        scene.apply(&SceneCmd::Add { index, node });
        id
    }

    fn add_text(scene: &mut Scene, rect: WorldRect, text: &str) {
        let node = scene.build_node(
            rect,
            NodeKind::Text(TextNode {
                text: text.into(),
                family: Default::default(),
                size: 24.0,
                color: Rgba::BLACK,
                align: Default::default(),
            }),
        );
        let index = scene.nodes.len();
        scene.apply(&SceneCmd::Add { index, node });
    }

    #[test]
    fn frame_with_image_and_text() {
        let dir = unique_temp_dir("slate-artifact-frame");
        let img_path = dir.join("photo.png");
        fs::write(&img_path, b"\x89PNG\r\n").expect("write png");

        let mut doc = SlateDoc::new("Demo");
        let item = doc.add_item(img_path, "photo.png", 8, 0, "k");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 800.0, 450.0));
        add_image(
            &mut doc.scene,
            WorldRect::new(100.0, 80.0, 200.0, 150.0),
            item,
            Crop::full(),
            Corner::Square,
            Stroke::none(),
            ImageAdjust::default(),
        );
        add_text(
            &mut doc.scene,
            WorldRect::new(100.0, 250.0, 300.0, 40.0),
            r#"<Hello> & "world""#,
        );

        let out = dir.join("out");
        let report = export_html(&doc, &out, &ExportOptions::default()).expect("export");
        assert_eq!(report.slides, 1);
        let html = fs::read_to_string(out.join("index.html")).expect("read html");

        assert_eq!(html.matches("<section class=\"slide").count(), 1);
        assert!(html.contains("left:100.0px;top:80.0px"));
        assert!(html.contains("left:100.0px;top:250.0px"));
        assert!(html.contains("&lt;Hello&gt; &amp; &quot;world&quot;"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn crop_math_in_output() {
        let mut doc = SlateDoc::new("Crop");
        let path = PathBuf::from("/tmp/slate-artifact-crop-test.png");
        let item = doc.add_item(path.clone(), "crop.png", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 400.0, 300.0));
        add_image(
            &mut doc.scene,
            WorldRect::new(50.0, 50.0, 200.0, 200.0),
            item,
            Crop {
                x: 0.25,
                y: 0.25,
                w: 0.5,
                h: 0.5,
            },
            Corner::Square,
            Stroke::none(),
            ImageAdjust::default(),
        );

        let mut assets = AssetMap::default();
        assets.insert(path, "assets/crop.png".into());
        let html = render_html(&doc, &assets);
        assert!(html.contains("width:200.0000%"));
        assert!(html.contains("left:-50.0000%"));
    }

    #[test]
    fn corner_stroke_styles() {
        let mut doc = SlateDoc::new("Styles");
        let item = doc.add_item(PathBuf::from("/x/a.png"), "a.png", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 600.0, 400.0));
        add_image(
            &mut doc.scene,
            WorldRect::new(20.0, 20.0, 100.0, 100.0),
            item,
            Crop::full(),
            Corner::Rounded { radius: 12.0 },
            Stroke {
                width: 2.0,
                color: Rgba::BLACK,
                dash: Dash::Dashed,
            },
            ImageAdjust::default(),
        );

        let html = render_html(&doc, &AssetMap::default());
        assert!(html.contains("border-radius:12.0px"));
        assert!(html.contains("border:2.0px dashed"));

        let mut doc2 = SlateDoc::new("Chamfer");
        let item2 = doc2.add_item(PathBuf::from("/x/b.png"), "b.png", 0, 0, "");
        add_frame(&mut doc2.scene, 0, WorldRect::new(0.0, 0.0, 600.0, 400.0));
        add_image(
            &mut doc2.scene,
            WorldRect::new(20.0, 20.0, 100.0, 100.0),
            item2,
            Crop::full(),
            Corner::Chamfer { cut: 8.0 },
            Stroke::none(),
            ImageAdjust::default(),
        );
        let html2 = render_html(&doc2, &AssetMap::default());
        assert!(html2.contains("clip-path:polygon(8.0px 0,calc(100% - 8.0px) 0"));
    }

    #[test]
    fn missing_image_placeholder() {
        let dir = unique_temp_dir("slate-artifact-missing");
        let mut doc = SlateDoc::new("Missing");
        let item = doc.add_item(dir.join("gone.png"), "gone.png", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 400.0, 300.0));
        add_image(
            &mut doc.scene,
            WorldRect::new(10.0, 10.0, 100.0, 100.0),
            item,
            Crop::full(),
            Corner::Square,
            Stroke::none(),
            ImageAdjust::default(),
        );

        let out = dir.join("out");
        let report = export_html(&doc, &out, &ExportOptions::default()).expect("export");
        assert_eq!(report.missing_assets, 1);
        let html = fs::read_to_string(out.join("index.html")).expect("read");
        assert!(html.contains("class=\"missing\""));
        assert!(html.contains("gone.png"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_frames_bounding_box_slide() {
        let mut doc = SlateDoc::new("BBox");
        let node = doc.scene.build_node(
            WorldRect::new(100.0, 50.0, 80.0, 60.0),
            NodeKind::Text(TextNode {
                text: "solo".into(),
                family: Default::default(),
                size: 16.0,
                color: Rgba::BLACK,
                align: Default::default(),
            }),
        );
        doc.scene.apply(&SceneCmd::Add { index: 0, node });

        let dir = unique_temp_dir("slate-artifact-bbox");
        let out = dir.join("out");
        let report = export_html(&doc, &out, &ExportOptions::default()).expect("export");
        assert_eq!(report.slides, 1);

        let html = fs::read_to_string(out.join("index.html")).expect("read");
        // Bbox min (100,50) − 40px padding origin → node at relative (40,40).
        assert!(html.contains("left:40.0px;top:40.0px"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn frames_sorted_by_order() {
        let mut doc = SlateDoc::new("Order");
        add_frame(&mut doc.scene, 2, WorldRect::new(0.0, 0.0, 100.0, 100.0));
        add_frame(&mut doc.scene, 0, WorldRect::new(200.0, 0.0, 100.0, 100.0));

        let html = render_html(&doc, &AssetMap::default());
        assert_eq!(html.matches("<section class=\"slide").count(), 2);

        let frames = doc.scene.frames_in_order();
        assert_eq!(frames[0].rect.x, 200.0);
        assert_eq!(frames[1].rect.x, 0.0);

        let pos0 = html.find("<section class=\"slide").unwrap();
        let pos1 = html[pos0 + 1..].find("<section class=\"slide").unwrap() + pos0 + 1;
        // Order-0 frame (x=200) is emitted before order-2 frame (x=0).
        let first_chunk = &html[pos0..pos0 + 200];
        let second_chunk = &html[pos1..pos1 + 200];
        assert!(first_chunk.contains("data-w=\"100.0\""));
        assert!(second_chunk.contains("data-w=\"100.0\""));
        assert!(pos0 < pos1);
    }

    #[test]
    fn determinism_identical_bytes() {
        let dir = unique_temp_dir("slate-artifact-det");
        let img_path = dir.join("pic.png");
        fs::write(&img_path, b"pngbytes").expect("write");

        let mut doc = SlateDoc::new("Deterministic");
        let item = doc.add_item(img_path, "pic.png", 8, 0, "k");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 320.0, 240.0));
        add_image(
            &mut doc.scene,
            WorldRect::new(10.0, 10.0, 50.0, 50.0),
            item,
            Crop::full(),
            Corner::Square,
            Stroke::none(),
            ImageAdjust::default(),
        );

        let out_a = dir.join("out_a");
        let out_b = dir.join("out_b");
        export_html(&doc, &out_a, &ExportOptions::default()).expect("a");
        export_html(&doc, &out_b, &ExportOptions::default()).expect("b");

        let bytes_a = fs::read(out_a.join("index.html")).expect("read a");
        let bytes_b = fs::read(out_b.join("index.html")).expect("read b");
        assert_eq!(bytes_a, bytes_b);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn inline_assets_data_uri() {
        let dir = unique_temp_dir("slate-artifact-inline");
        let img_path = dir.join("tiny.png");
        fs::write(&img_path, b"\x89PNG").expect("write");

        let mut doc = SlateDoc::new("Inline");
        let item = doc.add_item(img_path, "tiny.png", 4, 0, "k");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 200.0, 200.0));
        add_image(
            &mut doc.scene,
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            item,
            Crop::full(),
            Corner::Square,
            Stroke::none(),
            ImageAdjust::default(),
        );

        let out = dir.join("out");
        let report = export_html(
            &doc,
            &out,
            &ExportOptions {
                inline_assets: true,
                ..Default::default()
            },
        )
        .expect("export");
        assert_eq!(report.assets_copied, 0);
        let html = fs::read_to_string(out.join("index.html")).expect("read");
        assert!(html.contains("data:image/png;base64,"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn image_adjust_filter_and_overlay() {
        let mut doc = SlateDoc::new("Adjust");
        let path = PathBuf::from("/tmp/slate-artifact-adjust.png");
        let item = doc.add_item(path.clone(), "c.png", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 300.0, 200.0));
        let adjust = ImageAdjust {
            brightness: 1.2,
            overlay: Some(Rgba::opaque(255, 0, 0)),
            ..ImageAdjust::default()
        };
        add_image(
            &mut doc.scene,
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            item,
            Crop::full(),
            Corner::Square,
            Stroke::none(),
            adjust,
        );

        let mut assets = AssetMap::default();
        assets.insert(path, "assets/c.png".into());
        let html = render_html(&doc, &assets);
        assert!(html.contains("filter:brightness(1.200)"));
        assert!(html.contains("class=\"ovl\""));
        assert!(html.contains("rgba(255,0,0,1.000)"));
    }

    fn add_media(scene: &mut Scene, rect: WorldRect, item: ItemId) -> NodeId {
        let node = scene.build_node(rect, NodeKind::Image(ImageNode::new(item)));
        let id = node.id;
        let index = scene.nodes.len();
        scene.apply(&SceneCmd::Add { index, node });
        id
    }

    #[test]
    fn video_trim_and_attrs() {
        use slate_doc::scene::VideoOpts;

        let mut doc = SlateDoc::new("Video");
        let path = PathBuf::from("/tmp/slate-artifact-clip.mp4");
        let item = doc.add_item(path.clone(), "clip.mp4", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 800.0, 450.0));
        let id = add_media(&mut doc.scene, WorldRect::new(0.0, 0.0, 400.0, 225.0), item);
        if let Some(node) = doc.scene.node_mut(id) {
            if let NodeKind::Image(img) = &mut node.kind {
                img.video = VideoOpts {
                    start: 2.5,
                    end: Some(9.0),
                    autoplay: true,
                    looped: true,
                    muted: true,
                    controls: false,
                };
            }
        }

        let mut assets = AssetMap::default();
        assets.insert(path, "assets/clip.mp4".into());
        let html = render_html(&doc, &assets);
        assert!(html.contains("<video playsinline autoplay loop muted"));
        assert!(!html.contains(" controls"));
        assert!(html.contains("data-tstart=\"2.500\""));
        assert!(html.contains("data-tend=\"9.000\""));
        assert!(html.contains("assets/clip.mp4#t=2.500,9.000"));
        // Trim-enforcement runtime is present.
        assert!(html.contains("video[data-tstart]"));
    }

    #[test]
    fn untrimmed_video_has_no_fragment() {
        let mut doc = SlateDoc::new("Video2");
        let path = PathBuf::from("/tmp/slate-artifact-clip2.webm");
        let item = doc.add_item(path.clone(), "clip2.webm", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 800.0, 450.0));
        add_media(&mut doc.scene, WorldRect::new(0.0, 0.0, 400.0, 225.0), item);

        let mut assets = AssetMap::default();
        assets.insert(path, "assets/clip2.webm".into());
        let html = render_html(&doc, &assets);
        assert!(html.contains("assets/clip2.webm\""));
        assert!(!html.contains("clip2.webm#t="));
        assert!(!html.contains("data-tstart="));
    }

    #[test]
    fn text_file_becomes_snippet_card() {
        let dir = unique_temp_dir("slate-artifact-text");
        let txt = dir.join("notes.md");
        fs::write(&txt, "# Heading\n<b>not html</b>\nline three").expect("write");

        let mut doc = SlateDoc::new("Text");
        let item = doc.add_item(txt, "notes.md", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 800.0, 450.0));
        add_media(&mut doc.scene, WorldRect::new(0.0, 0.0, 300.0, 200.0), item);

        let out = dir.join("out");
        export_html(&doc, &out, &ExportOptions::default()).expect("export");
        let html = fs::read_to_string(out.join("index.html")).expect("read");
        assert!(html.contains("class=\"textcard\""));
        assert!(html.contains("&lt;b&gt;not html&lt;/b&gt;"));
        assert!(html.contains("notes.md"));
        // Original copied and linked.
        assert!(html.contains("href=\"assets/notes-"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pdf_gets_thumb_card_with_link() {
        let dir = unique_temp_dir("slate-artifact-pdf");
        let pdf = dir.join("report.pdf");
        fs::write(&pdf, b"%PDF-1.4 fake").expect("write pdf");
        let thumb = dir.join("report-thumb.jpg");
        fs::write(&thumb, b"\xff\xd8 fake jpg").expect("write thumb");

        let mut doc = SlateDoc::new("Pdf");
        let item = doc.add_item(pdf, "report.pdf", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 800.0, 450.0));
        add_media(&mut doc.scene, WorldRect::new(0.0, 0.0, 200.0, 260.0), item);

        let mut opts = ExportOptions::default();
        opts.thumbs.insert(item, thumb);
        let out = dir.join("out");
        export_html(&doc, &out, &opts).expect("export");
        let html = fs::read_to_string(out.join("index.html")).expect("read");
        assert!(html.contains("class=\"thumbcard\""));
        assert!(html.contains("href=\"assets/report-"));
        assert!(html.contains("assets/report-thumb-"));
        assert!(html.contains("<span class=\"badge\">PDF</span>"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn doc_without_thumb_gets_badge_card() {
        let dir = unique_temp_dir("slate-artifact-doc");
        let docx = dir.join("essay.docx");
        fs::write(&docx, b"PK fake docx").expect("write");

        let mut doc = SlateDoc::new("Doc");
        let item = doc.add_item(docx, "essay.docx", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 800.0, 450.0));
        add_media(&mut doc.scene, WorldRect::new(0.0, 0.0, 200.0, 260.0), item);

        let out = dir.join("out");
        export_html(&doc, &out, &ExportOptions::default()).expect("export");
        let html = fs::read_to_string(out.join("index.html")).expect("read");
        assert!(html.contains("class=\"filecard\""));
        assert!(html.contains("<span class=\"badge\">DOCX</span>"));
        assert!(html.contains("essay.docx"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn non_web_safe_video_becomes_card() {
        let dir = unique_temp_dir("slate-artifact-mov");
        let mov = dir.join("raw.mov");
        fs::write(&mov, b"fake mov").expect("write");

        let mut doc = SlateDoc::new("Mov");
        let item = doc.add_item(mov, "raw.mov", 0, 0, "");
        add_frame(&mut doc.scene, 0, WorldRect::new(0.0, 0.0, 800.0, 450.0));
        add_media(&mut doc.scene, WorldRect::new(0.0, 0.0, 320.0, 180.0), item);

        let out = dir.join("out");
        export_html(&doc, &out, &ExportOptions::default()).expect("export");
        let html = fs::read_to_string(out.join("index.html")).expect("read");
        assert!(!html.contains("<video"));
        assert!(html.contains("<span class=\"badge\">MOV</span>"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_scene() {
        let dir = unique_temp_dir("slate-artifact-empty");
        let doc = SlateDoc::new("Empty");
        let out = dir.join("out");
        let report = export_html(&doc, &out, &ExportOptions::default()).expect("export");
        assert_eq!(report.slides, 0);
        let html = fs::read_to_string(out.join("index.html")).expect("read");
        assert!(html.contains("Empty board"));
        assert!(!html.contains("<section class=\"slide"));
        let _ = fs::remove_dir_all(dir);
    }
}
