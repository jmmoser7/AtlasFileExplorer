//! Windows `.slate` file association (per-user, no admin rights needed).
//!
//! Registers `HKCU\Software\Classes\.slate` → `Slate.Workbook` with the exe's
//! embedded icon and an open command, so double-clicking a workbook in
//! Explorer launches Slate. Re-registers automatically when the exe moves.

#[cfg(windows)]
pub fn ensure_file_association() {
    std::thread::spawn(|| {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let exe = exe.display().to_string();

        // Skip the registry writes when we already point at this exe.
        let marker = atlas_core::index::data_dir().join("slate-assoc.txt");
        if std::fs::read_to_string(&marker)
            .map(|s| s == exe)
            .unwrap_or(false)
        {
            return;
        }

        const PROGID: &str = "Slate.Workbook";
        let sets: [(String, String); 4] = [
            (r"HKCU\Software\Classes\.slate".into(), PROGID.into()),
            (
                format!(r"HKCU\Software\Classes\{PROGID}"),
                "Slate workbook".into(),
            ),
            (
                format!(r"HKCU\Software\Classes\{PROGID}\DefaultIcon"),
                format!("\"{exe}\",0"),
            ),
            (
                format!(r"HKCU\Software\Classes\{PROGID}\shell\open\command"),
                format!("\"{exe}\" \"%1\""),
            ),
        ];
        let mut ok = true;
        for (key, value) in &sets {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let status = std::process::Command::new("reg")
                .args(["add", key, "/ve", "/d", value, "/f"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
            ok &= status.map(|s| s.success()).unwrap_or(false);
        }
        if ok {
            let _ = std::fs::write(&marker, exe);
        }
    });
}

#[cfg(not(windows))]
pub fn ensure_file_association() {}
