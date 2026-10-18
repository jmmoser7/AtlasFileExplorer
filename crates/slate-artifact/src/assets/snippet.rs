//! One bounded, local-only excerpt reader for the board and HTML writer.

use super::{SNIPPET_MAX_CHARS, SNIPPET_MAX_LINES};
use std::{fs::File, io::Read, path::Path};

/// First 1200 characters / 30 lines of a local text file. Structured packages
/// are extracted before clamping. Machine-private and cloud-only sources are
/// refused before either reader opens them. Call this on a worker thread.
pub fn read_snippet(path: &Path) -> Option<String> {
    read_checked(path, |path| {
        atlas_core::workbook_assets::require_local_source(path).is_ok()
    })
}

fn read_checked(path: &Path, local: impl FnOnce(&Path) -> bool) -> Option<String> {
    if !local(path) {
        return None;
    }
    if slate_doc::media::structured_text_package(path) {
        return atlas_core::office::document_excerpt(path).and_then(|text| clamp(&text));
    }
    prefix(File::open(path).ok()?)
}

fn prefix(reader: impl Read) -> Option<String> {
    // Four bytes per scalar is sufficient for the entire UTF-8 excerpt. Take
    // caps the I/O itself, rather than slicing after a whole-file allocation.
    let mut bytes = Vec::new();
    reader
        .take((SNIPPET_MAX_CHARS * 4) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    clamp(&String::from_utf8_lossy(&bytes))
}

fn clamp(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut lines = 1;
    for (chars, ch) in text.chars().enumerate() {
        if chars >= SNIPPET_MAX_CHARS {
            break;
        }
        if ch == '\n' {
            lines += 1;
            if lines > SNIPPET_MAX_LINES {
                break;
            }
        }
        out.push(ch);
    }
    let out = out.trim_end().to_string();
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;

    #[test]
    fn prefix_caps_actual_bytes_read_and_keeps_utf8() {
        let consumed = Cell::new(0);
        struct Count<'a>(&'a Cell<usize>);
        impl Read for Count<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                bytes.fill(b'x');
                self.0.set(self.0.get() + bytes.len());
                Ok(bytes.len())
            }
        }
        assert_eq!(prefix(Count(&consumed)).unwrap().len(), SNIPPET_MAX_CHARS);
        assert_eq!(consumed.get(), SNIPPET_MAX_CHARS * 4);
        let unicode = "🦊".repeat(SNIPPET_MAX_CHARS + 1);
        assert_eq!(
            prefix(unicode.as_bytes()).unwrap(),
            "🦊".repeat(SNIPPET_MAX_CHARS)
        );
    }

    #[test]
    fn a_refused_source_never_reaches_plain_or_office_readers() {
        for ext in ["txt", "docx", "xlsx", "rtf"] {
            // Inject a directory-entry refusal; no real cloud file is touched.
            assert!(read_checked(Path::new(&format!("online-only.{ext}")), |_| false).is_none());
        }
    }

    #[test]
    fn machine_private_text_is_refused_even_when_local() {
        let root =
            std::env::temp_dir().join(format!("slate-private-snippet-{}", std::process::id()));
        let path = root.join("NativeFileAtlas/secrets/preview.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "must not be excerpted").unwrap();
        assert!(read_snippet(&path).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn docx_snippet_is_the_document_text() {
        let dir = std::env::temp_dir().join(format!(
            "slate-docx-snippet-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.docx");
        let file = fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("word/document.xml", opts).unwrap();
        use std::io::Write;
        zip.write_all(
            br#"<w:document><w:body><w:p><w:r><w:t>Quarterly note</w:t></w:r></w:p></w:body></w:document>"#,
        )
        .unwrap();
        zip.finish().unwrap();
        assert_eq!(read_snippet(&path).as_deref(), Some("Quarterly note"));
        let _ = fs::remove_dir_all(&dir);
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
