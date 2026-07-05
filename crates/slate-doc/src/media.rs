//! Media-kind classification for linked files.
//!
//! One extension-based taxonomy shared by the Slate app (board rendering,
//! double-click behavior, inspector sections) and the artifact writer
//! (which HTML element a placed item becomes). Keeping it here — in the
//! document model — is what stops the two renderers from disagreeing about
//! what a file *is*.

use std::path::Path;

use crate::doc::SLATE_EXTENSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// Web-displayable raster/vector images (`<img>`).
    Image,
    /// Video files (`<video>` when web-safe, thumbnail card otherwise).
    Video,
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
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "tif" | "tiff" | "avif"
        | "ico" => MediaKind::Image,
        "mp4" | "webm" | "ogv" | "m4v" | "mov" | "avi" | "mkv" | "wmv" | "mpg" | "mpeg" => {
            MediaKind::Video
        }
        "pdf" => MediaKind::Pdf,
        "txt" | "md" | "markdown" | "log" | "csv" | "json" | "toml" | "yaml" | "yml" | "xml"
        | "rs" | "py" | "js" | "ts" | "html" | "css" | "sh" | "bat" | "ini" | "cfg" => {
            MediaKind::Text
        }
        "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" | "odt" | "odp" | "ods" | "rtf"
        | "key" | "pages" | "numbers" | "indd" | "psd" | "ai" => MediaKind::Doc,
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
