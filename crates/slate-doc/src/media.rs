//! Media-kind classification for linked files.
//!
//! One extension-based taxonomy shared by the Slate app (board rendering,
//! double-click behavior, inspector sections) and the artifact writer
//! (which HTML element a placed item becomes). Keeping it here — in the
//! document model — is what stops the two renderers from disagreeing about
//! what a file *is*.

use std::path::Path;

use crate::doc::SLATE_EXTENSION;
use crate::scene::Corner;

/// World-unit fillet for a text-document card that has no authored corner.
/// The same designed radius as a portal frame
/// (`PortalFrameTokens::default().corner_radius`).
pub const TEXT_CARD_FILLET: f32 = 8.0;

/// Square text documents pick up [`TEXT_CARD_FILLET`]. An authored fillet or
/// chamfer is left alone. Photos and other media keep their stored corner.
pub fn text_card_corner(path: &Path, corner: Corner) -> Corner {
    if matches!(corner, Corner::Square) && media_kind(path) == MediaKind::Text {
        Corner::Rounded {
            radius: TEXT_CARD_FILLET,
        }
    } else {
        corner
    }
}

const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "tif", "tiff", "avif", "ico",
];
const VIDEOS: &[&str] = &[
    "mp4", "webm", "ogv", "m4v", "mov", "avi", "mkv", "wmv", "mpg", "mpeg",
];
const POWERPOINT: &[&str] = &[
    "ppt", "pptx", "pps", "ppsx", "pptm", "ppsm", "pot", "potx", "potm",
];
/// Print and design files that stay on the Image picker (thumbnail cards).
const VISUAL_DOCS: &[&str] = &["odp", "key", "indd", "psd", "ai"];
const WORD: &[&str] = &[
    "doc", "docx", "docm", "dot", "dotx", "dotm", "odt", "rtf", "pages",
];
const SHEETS: &[&str] = &[
    "xls", "xlsx", "xlsm", "xlt", "xltx", "xltm", "ods", "csv", "tsv", "numbers",
];
/// Source and data files whose bytes are already the text.
const CODE: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "c", "cpp", "h", "hpp", "cs", "java", "go", "rb", "php",
    "sh", "ps1", "bat", "cmd", "lua", "swift", "kt", "sql", "html", "htm", "css", "scss", "json",
    "toml", "yaml", "yml", "xml", "ini", "cfg", "log",
];
const PLAIN: &[&str] = &["txt", "md", "markdown"];

/// Picker families use the same extension taxonomy as both interpreters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaGroup {
    Image,
    Text,
    Model,
    Video,
}

impl MediaGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Text => "Text documents",
            Self::Model => "3D",
            Self::Video => "Video",
        }
    }
    pub fn extensions(self) -> Vec<&'static str> {
        match self {
            Self::Image => IMAGES
                .iter()
                .chain(VISUAL_DOCS)
                .chain(POWERPOINT)
                .copied()
                .chain(["pdf"])
                .collect(),
            Self::Text => WORD
                .iter()
                .chain(SHEETS)
                .chain(CODE)
                .chain(PLAIN)
                .copied()
                .collect(),
            Self::Model => MODEL_EXTENSIONS.to_vec(),
            Self::Video => VIDEOS.to_vec(),
        }
    }
    pub fn accepts(self, path: &Path) -> bool {
        match self {
            Self::Image => matches!(
                media_kind(path),
                MediaKind::Image | MediaKind::Pdf | MediaKind::Doc
            ),
            Self::Text => media_kind(path) == MediaKind::Text,
            Self::Model => media_kind(path) == MediaKind::Model,
            Self::Video => media_kind(path) == MediaKind::Video,
        }
    }
}

/// Word, Excel, and other packages. The excerpt has to be extracted; showing
/// the file bytes would be a zip or OLE dump. CSV and source code are not
/// packages.
pub fn structured_text_package(path: &Path) -> bool {
    let Some(ext) = extension_lc(path) else {
        return false;
    };
    // CSV and TSV are spreadsheets whose bytes are already the text.
    WORD.contains(&ext.as_str()) || (SHEETS.contains(&ext.as_str()) && ext != "csv" && ext != "tsv")
}

fn extension_lc(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
}

fn text_document_ext(ext: &str) -> bool {
    WORD.contains(&ext) || SHEETS.contains(&ext) || CODE.contains(&ext) || PLAIN.contains(&ext)
}

pub fn is_powerpoint(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| POWERPOINT.contains(&s.to_ascii_lowercase().as_str()))
}

pub fn has_pages(path: &Path) -> bool {
    media_kind(path) == MediaKind::Pdf || is_powerpoint(path)
}

#[cfg(test)]
mod picker_tests {
    use super::*;
    #[test]
    fn picker_filters_and_media_kinds_agree() {
        for group in [
            MediaGroup::Image,
            MediaGroup::Text,
            MediaGroup::Model,
            MediaGroup::Video,
        ] {
            for ext in group.extensions() {
                assert!(group.accepts(Path::new(&format!("file.{ext}"))), "{ext}");
            }
            assert!(!group.accepts(Path::new("workbook.slate")));
        }
        for file in ["photo.JPG", "pages.pdf", "deck.PPTX", "deck.ppt"] {
            assert!(MediaGroup::Image.accepts(Path::new(file)));
        }
        assert!(has_pages(Path::new("deck.PPTX")));
        assert!(!has_pages(Path::new("photo.jpg")));
        assert!(MediaGroup::Text.accepts(Path::new("essay.DOCX")));
        assert!(MediaGroup::Text.accepts(Path::new("budget.xlsx")));
        assert!(MediaGroup::Text.accepts(Path::new("rows.csv")));
        assert!(MediaGroup::Text.accepts(Path::new("main.rs")));
        assert!(!MediaGroup::Image.accepts(Path::new("essay.docx")));
        assert!(!MediaGroup::Image.accepts(Path::new("rows.csv")));
        assert!(!MediaGroup::Text.accepts(Path::new("deck.pptx")));
        assert!(!MediaGroup::Text.accepts(Path::new("photo.jpg")));
        assert!(structured_text_package(Path::new("essay.docx")));
        assert!(structured_text_package(Path::new("budget.xlsx")));
        assert!(!structured_text_package(Path::new("rows.csv")));
        assert!(!structured_text_package(Path::new("main.rs")));
        use crate::scene::Corner;
        assert_eq!(
            text_card_corner(Path::new("notes.md"), Corner::Square),
            Corner::Rounded {
                radius: TEXT_CARD_FILLET
            }
        );
        assert_eq!(
            text_card_corner(Path::new("notes.md"), Corner::Rounded { radius: 3.0 }),
            Corner::Rounded { radius: 3.0 }
        );
        assert_eq!(
            text_card_corner(Path::new("photo.png"), Corner::Square),
            Corner::Square
        );
        assert!(MediaGroup::Model.accepts(Path::new("mass.obj")));
        assert!(MediaGroup::Model.accepts(Path::new("house.blend")));
        assert!(!MediaGroup::Model.accepts(Path::new("walk.exe")));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// Web-displayable raster/vector images (`<img>`).
    Image,
    /// Video files (`<video>` when web-safe, thumbnail card otherwise).
    Video,
    /// 3D model files. The board uses one viewport for every extension in
    /// [`MODEL_EXTENSIONS`]. Some extensions decode to a mesh; others are
    /// recognized and say that a preview is not available yet. The artifact
    /// exports the frozen-camera poster, linking to the copied original.
    Model,
    Pdf,
    /// Plain text, source code, CSV, and Word / spreadsheet files. An excerpt
    /// is shown when the file can be read; a package with no excerpt falls
    /// back to the document card.
    Text,
    /// Print and design files that render as a thumbnail card (PowerPoint,
    /// Photoshop, Illustrator, InDesign).
    Doc,
    /// A `.slate` workbook. **Never becomes an item**: workbooks open as
    /// tabs. This is the guard against a workbook embedding another
    /// workbook (or itself) and recursing.
    Workbook,
    Other,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Image => "Image",
            MediaKind::Video => "Video",
            MediaKind::Model => "3D model",
            MediaKind::Pdf => "PDF",
            MediaKind::Text => "Text",
            MediaKind::Doc => "Document",
            MediaKind::Workbook => "Slate workbook",
            MediaKind::Other => "File",
        }
    }
}

/// Extensions that place as one 3D model node. The preview crate
/// (`model-preview`) owns which of these decode to a mesh and which stay a
/// gap card. Both lists are checked against each other by
/// `every_model_extension_has_one_preview_owner`.
pub const MODEL_EXTENSIONS: &[&str] = &[
    "3dm", "obj", "stl", "gltf", "glb", "blend", "dwg", "dxf", "skp", "fbx",
];

/// Classify a file by extension (lowercased).
/// How an ambiguous file can be shown. The catalog is this function: a format
/// with more than one face asks, instead of guessing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewFace {
    /// Source or excerpt.
    Text,
    /// Rendered page or picture.
    Graphic,
}

impl PreviewFace {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Display as text",
            Self::Graphic => "Display as graphic",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Graphic => "graphic",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "text" => Some(Self::Text),
            "graphic" => Some(Self::Graphic),
            _ => None,
        }
    }
}

/// Faces worth offering. Empty means the file has one ordinary presentation.
pub fn preview_choices(path: &Path) -> &'static [PreviewFace] {
    const HTML: &[PreviewFace] = &[PreviewFace::Text, PreviewFace::Graphic];
    if ext_is(path, "html") || ext_is(path, "htm") {
        HTML
    } else {
        &[]
    }
}

/// Spreadsheet files a chart wire can read. CSV and Excel packages.
pub fn is_table_source(path: &Path) -> bool {
    ext_is(path, "csv")
        || ext_is(path, "tsv")
        || ext_is(path, "xlsx")
        || ext_is(path, "xlsm")
        || ext_is(path, "xltx")
        || ext_is(path, "xltm")
}

fn ext_is(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

pub fn media_kind(path: &Path) -> MediaKind {
    let Some(ext) = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return MediaKind::Other;
    };
    match ext.as_str() {
        e if IMAGES.contains(&e) => MediaKind::Image,
        e if VIDEOS.contains(&e) => MediaKind::Video,
        ext if MODEL_EXTENSIONS.contains(&ext) => MediaKind::Model,
        "pdf" => MediaKind::Pdf,
        e if text_document_ext(e) => MediaKind::Text,
        e if VISUAL_DOCS.contains(&e) || POWERPOINT.contains(&e) => MediaKind::Doc,
        e if e == SLATE_EXTENSION => MediaKind::Workbook,
        _ => MediaKind::Other,
    }
}

/// Whether browsers can be expected to play this video natively. Non-web-safe
/// videos (mov/avi/mkv/…) export as thumbnail cards linking to the file.
pub fn web_safe_video(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mp4") | Some("webm") | Some("ogv") | Some("m4v")
    )
}

/// Uppercase extension badge for cards ("PDF", "DOCX", …), clamped to 5 chars.
pub fn ext_badge(path: &Path) -> String {
    let mut ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    ext.truncate(5);
    ext
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classification_covers_the_families() {
        let cases = [
            ("photo.JPG", MediaKind::Image),
            ("clip.mp4", MediaKind::Video),
            ("clip.MOV", MediaKind::Video),
            ("tower.3dm", MediaKind::Model),
            ("Tower.3DM", MediaKind::Model),
            ("mesh.obj", MediaKind::Model),
            ("part.STL", MediaKind::Model),
            ("scene.glb", MediaKind::Model),
            ("scene.gltf", MediaKind::Model),
            ("house.blend", MediaKind::Model),
            ("plan.dwg", MediaKind::Model),
            ("mass.skp", MediaKind::Model),
            ("prop.fbx", MediaKind::Model),
            ("walk.exe", MediaKind::Other),
            ("report.pdf", MediaKind::Pdf),
            ("notes.md", MediaKind::Text),
            ("essay.docx", MediaKind::Text),
            ("legacy.doc", MediaKind::Text),
            ("budget.xlsx", MediaKind::Text),
            ("budget.XLS", MediaKind::Text),
            ("rows.csv", MediaKind::Text),
            ("main.rs", MediaKind::Text),
            ("page.html", MediaKind::Text),
            ("layout.psd", MediaKind::Doc),
            ("deck.pptx", MediaKind::Doc),
            ("moodboard.slate", MediaKind::Workbook),
            ("archive.zip", MediaKind::Other),
            ("no_extension", MediaKind::Other),
        ];
        for (name, expected) in cases {
            assert_eq!(media_kind(&PathBuf::from(name)), expected, "{name}");
        }
    }

    #[test]
    fn html_asks_text_or_graphic_and_a_spreadsheet_does_not() {
        let html = preview_choices(Path::new("dash.HTML"));
        assert_eq!(html, &[PreviewFace::Text, PreviewFace::Graphic]);
        assert!(preview_choices(Path::new("notes.md")).is_empty());
        assert!(preview_choices(Path::new("photo.png")).is_empty());
        assert!(is_table_source(Path::new("Budget.XLSX")));
        assert!(is_table_source(Path::new("rows.csv")));
        assert!(!is_table_source(Path::new("dash.html")));
    }

    #[test]
    fn web_safe_video_subset() {
        assert!(web_safe_video(Path::new("a.mp4")));
        assert!(web_safe_video(Path::new("a.webm")));
        assert!(!web_safe_video(Path::new("a.mov")));
        assert!(!web_safe_video(Path::new("a.mkv")));
        assert!(!web_safe_video(Path::new("a.png")));
    }

    #[test]
    fn badge_is_upper_and_clamped() {
        assert_eq!(ext_badge(Path::new("a.pdf")), "PDF");
        assert_eq!(ext_badge(Path::new("a.markdown")), "MARKD");
        assert_eq!(ext_badge(Path::new("bare")), "");
    }
}
