//! Embed the Slate icon into the Windows executable (used by the taskbar,
//! Explorer, and the `.slate` file association's DefaultIcon).

fn main() {
    println!("cargo:rerun-if-changed=assets/slate.ico");
    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/slate.ico");
            if let Err(e) = res.compile() {
                println!("cargo:warning=icon resource not embedded: {e}");
            }
        }
    }
}
