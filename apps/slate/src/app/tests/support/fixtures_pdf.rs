//! Minimal PDF bytes and the pdfium render stand-ins.

pub(crate) fn fixture_pdf(pages: u16) -> Vec<u8> {
    fixture_pdf_boxes(&vec![(200.0, 100.0); pages as usize])
}

/// One page per `(width, height)` in PDF points. Page order follows the vector.
pub(crate) fn fixture_pdf_boxes(boxes: &[(f32, f32)]) -> Vec<u8> {
    let pages = boxes.len();
    let mut kids = String::new();
    let mut body = String::new();
    for (index, (w, h)) in boxes.iter().enumerate() {
        let id = 3 + index;
        if index > 0 {
            kids.push(' ');
        }
        kids.push_str(&format!("{id} 0 R"));
        body.push_str(&format!(
            "{id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] >> endobj\n"
        ));
    }
    format!(
        "%PDF-1.4\n\
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
2 0 obj << /Type /Pages /Kids [{kids}] /Count {pages} >> endobj\n\
{body}\
trailer << /Root 1 0 R >>\n\
%%EOF"
    )
    .into_bytes()
}

pub(crate) fn render_pdf_pages(pages: u16, dest: &std::path::Path) -> Result<(), String> {
    std::fs::write(dest, fixture_pdf(pages)).map_err(|e| e.to_string())
}

pub(crate) fn render_two_page_pdf(
    _: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), String> {
    render_pdf_pages(2, dest)
}

pub(crate) fn render_three_page_pdf(
    _: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), String> {
    render_pdf_pages(3, dest)
}
