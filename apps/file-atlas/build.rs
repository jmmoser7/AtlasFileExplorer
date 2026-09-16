//! Embed the File Atlas icon for Windows Explorer and shortcuts.
fn main() {
    println!("cargo:rerun-if-changed=assets/file-atlas.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/file-atlas.ico")
            .compile()
            .expect("embed File Atlas icon");
    }
}
