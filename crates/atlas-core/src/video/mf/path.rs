use std::borrow::Cow;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

/// MFCreateFile takes a filesystem name, not a percent-encoded URL. Preserve
/// exact UTF16 drive/UNC/verbatim units; resolve only relative inputs against
/// the inherited working directory without rewriting stored/IPC source identity.
pub(super) fn reader_path(path: &Path) -> Result<Vec<u16>, ()> {
    let resolved = if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(std::path::absolute(path).map_err(|_| invalid_path())?)
    };
    let mut wide = Vec::new();
    wide.try_reserve_exact(resolved.as_os_str().len() + 1)
        .map_err(|_| ())?;
    wide.extend(resolved.as_os_str().encode_wide());
    if wide.contains(&0) {
        invalid_path();
        return Err(());
    }
    wide.push(0);
    Ok(wide)
}
fn invalid_path() {
    super::diagnostics::at("reader.file-path")(windows::core::Error::from_hresult(
        windows::Win32::Foundation::E_INVALIDARG,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn converted(path: &str) -> String {
        let wide = reader_path(Path::new(path)).unwrap();
        assert_eq!(wide.last(), Some(&0));
        String::from_utf16(&wide[..wide.len() - 1]).unwrap()
    }
    #[test]
    fn absolute_drive_and_verbatim_drive_units_are_preserved() {
        for path in [r"C:\Media\clip.mp4", r"\\?\C:\Media\clip.mp4"] {
            assert_eq!(converted(path), path);
        }
    }
    #[test]
    fn unc_and_verbatim_unc_units_are_preserved() {
        for path in [r"\\server\share\clip.mp4", r"\\?\UNC\server\share\clip.mp4"] {
            assert_eq!(converted(path), path);
        }
    }
    #[test]
    fn unicode_reserved_and_long_names_are_not_url_encoded_or_truncated() {
        let unicode = "C:\\Media\\\u{03bb} #100%.mp4";
        assert_eq!(converted(unicode), unicode);
        let long = format!(r"\\?\C:\{}clip.mp4", "deep-folder\\".repeat(30));
        assert!(long.len() > 260);
        assert_eq!(converted(&long), long);
    }
    #[test]
    fn relative_names_resolve_without_touching_global_working_directory() {
        let relative = Path::new("relative folder/clip#100%.mp4");
        let expected = std::path::absolute(relative).unwrap();
        let actual = reader_path(relative).unwrap();
        assert_eq!(
            actual,
            expected
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn embedded_nul_is_rejected_before_native_file_open() {
        assert!(reader_path(Path::new("C:\\Media\\clip\0.mp4")).is_err());
    }
}
