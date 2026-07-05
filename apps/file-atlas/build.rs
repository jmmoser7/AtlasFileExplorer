//! Copy `vendor/pdfium.dll` beside the built exe when present so PDF previews
//! work out of the box during development and release builds on Windows.

use std::path::Path;

fn main() {
    let dll = Path::new("vendor/pdfium.dll");
    println!("cargo:rerun-if-changed=vendor/pdfium.dll");
    if !dll.exists() {
        return;
    }

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".into());
    let dest = Path::new(&target_dir).join(&profile).join("pdfium.dll");
    if std::fs::copy(dll, &dest).is_ok() {
        eprintln!("Copied vendor/pdfium.dll -> {}", dest.display());
    }
}
