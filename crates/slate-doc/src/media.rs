//! Media-kind classification for linked files.
//!
//! One extension-based taxonomy shared by the Slate app (board rendering,
//! double-click behavior, inspector sections) and the artifact writer
//! (which HTML element a placed item becomes). Keeping it here — in the
//! document model — is what stops the two renderers from disagreeing about
//! what a file *is*.

use std::path::Path;

use crate::doc::SLATE_EXTENSION;

const IMAGES: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "tif", "tiff", "avif", "ico",
];
const VIDEOS: &[&str] = &[
    "mp4", "webm", "ogv", "m4v", "mov", "avi", "mkv", "wmv", "mpg", "mpeg",
];
const POWERPOINT: &[&str] = &[
    "ppt", "pptx", "pps", "ppsx", "pptm", "ppsm", "pot", "potx", "potm",
];
const DOCUMENTS: &[&str] = &[
    "doc", "docx", "xls", "xlsx", "odt", "odp", "ods", "rtf", "key", "pages", "numbers", "indd",
    "psd", "ai",
];

/// Picker families use the same extension taxonomy as both interpreters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaGroup {
    Image,
    Model,
    Video,
}

impl MediaGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Model => "3D",
            Self::Video => "Video",
        }
    }
    pub fn extensions(self) -> Vec<&'static str> {
        match self {
            Self::Image => IMAGES
                .iter()
                .chain(DOCUMENTS)
                .chain(POWERPOINT)
                .copied()
                .chain(["pdf"])
                .collect(),
            Self::Model => vec!["3dm"],
            Self::Video => VIDEOS.to_vec(),
        }
    }
    pub fn accepts(self, path: &Path) -> bool {
        match self {
            Self::Image => matches!(
                media_kind(path),
                MediaKind::Image | MediaKind::Pdf | MediaKind::Doc
            ),
            Self::Model => media_kind(path) == MediaKind::Model,
            Self::Video => media_kind(path) == MediaKind::Video,
        }
    }
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
        for group in [MediaGroup::Image, MediaGroup::Model, MediaGroup::Video] {
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
        assert!(!MediaGroup::Model.accepts(Path::new("unsupported.obj")));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// Web-displayable raster/vector images (`<img>`).
    Image,
    /// Video files (`<video>` when web-safe, thumbnail card otherwise).
    Video,
    /// 3D model files with an interactive board viewport (Rhino `.3dm`).
    /// The board renders a live viewport or its frozen-camera poster; the
    /// artifact exports the poster image linking to the copied original.
    Model,
    Pdf,
    /// Plain-text-ish files whose content can be excerpted inline.
    Text,
    /// Office / rich documents (thumbnail card + link).
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

/// Classify a file by extension (lowercased).
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
        // Only .3dm for now: Model implies the board can extract meshes and
        // drive an interactive viewport (rhino-mesh crate). Other 3D formats
        // (obj/stl/gltf…) stay `Other` until they have a mesh loader too.
        "3dm" => MediaKind::Model,
        "pdf" => MediaKind::Pdf,
        "txt" | "md" | "markdown" | "log" | "csv" | "json" | "toml" | "yaml" | "yml" | "xml"
        | "rs" | "py" | "js" | "ts" | "html" | "css" | "sh" | "bat" | "ini" | "cfg" => {
            MediaKind::Text
        }
        e if DOCUMENTS.contains(&e) || POWERPOINT.contains(&e) => MediaKind::Doc,
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
            ("mesh.obj", MediaKind::Other),
            ("report.pdf", MediaKind::Pdf),
            ("notes.md", MediaKind::Text),
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
