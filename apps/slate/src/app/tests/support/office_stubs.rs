//! Office render stand-ins: missing, slow, and must-not-run.

use super::*;

pub(crate) fn render_office_missing(
    _: &std::path::Path,
    _: &std::path::Path,
) -> Result<(), String> {
    Err("PowerPoint is not installed.".into())
}

pub(crate) fn render_office_slow(
    source: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(400));
    render_three_page_pdf(source, dest)
}

pub(crate) fn render_must_not_run(_: &std::path::Path, _: &std::path::Path) -> Result<(), String> {
    Err("RENDERED".into())
}

pub(crate) fn install_office(
    h: &mut Harness,
    powerpoint: pdf::documents::OfficeRender,
    word: pdf::documents::OfficeRender,
) {
    h.app
        .documents
        .set_office_renderers(pdf::documents::OfficeRenderers { powerpoint, word });
}
