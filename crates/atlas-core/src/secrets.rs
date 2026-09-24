//! Machine-local secrets. A workbook may name a service; it never stores the value.
//!
//! Health is [`SecretHealth::Ok`] when this operating-system user has the
//! secret, and [`SecretHealth::Missing`] for everyone else — including another
//! person who opens the same `.slate` file. On Windows the value lives in
//! Credential Manager (`CRED_PERSIST_LOCAL_MACHINE`), which is encrypted to
//! that user's login. Elsewhere, and in tests that set `ATLAS_SECRET_DIR`,
//! the value is a file outside the workbook. Nothing here is journaled,
//! exported, or written beside a document.

use std::path::{Component, Path, PathBuf};

/// Whether this user has a secret for `slot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretHealth {
    Ok,
    Missing,
}

const MAX_BLOB: usize = 2560;

/// `true` when `path` is the WebView2 profile folder, the file secret store,
/// a legacy API-key file, or a per-user trust grant (web origin consent,
/// agent full access) under the Atlas data directory. Packaging and HTML
/// export must refuse these paths.
pub fn is_machine_private(path: &Path) -> bool {
    let mut in_atlas = false;
    for component in path.components() {
        let Component::Normal(raw) = component else {
            continue;
        };
        let Some(name) = raw.to_str() else {
            continue;
        };
        if !in_atlas && name.eq_ignore_ascii_case("NativeFileAtlas") {
            in_atlas = true;
            continue;
        }
        if in_atlas && is_private_component(name) {
            return true;
        }
    }
    false
}

fn is_private_component(name: &str) -> bool {
    name.eq_ignore_ascii_case("webview2")
        || name.eq_ignore_ascii_case("secrets")
        || name.eq_ignore_ascii_case("cursor-api-key")
        || name.eq_ignore_ascii_case("web-consent.json")
        || name.eq_ignore_ascii_case("agent-access.json")
}

/// Read a secret. `None` means this user does not have one.
pub fn load(slot: &str) -> Option<String> {
    if !valid_slot(slot) {
        return None;
    }
    match backend() {
        Backend::Dir(dir) => read_file(&dir.join(slot)),
        #[cfg(windows)]
        Backend::CredentialManager => read_credential(slot),
    }
}

pub fn health(slot: &str) -> SecretHealth {
    if load(slot).is_some() {
        SecretHealth::Ok
    } else {
        SecretHealth::Missing
    }
}

/// Store a secret for this user. The value is trimmed. An empty value is refused.
pub fn store(slot: &str, value: &str) -> Result<(), String> {
    if !valid_slot(slot) {
        return Err("Secret slot name is not valid.".into());
    }
    let value = value.trim();
    if value.is_empty() {
        return Err("Paste the secret first.".into());
    }
    if value.len() > MAX_BLOB {
        return Err("That secret is too long to store.".into());
    }
    match backend() {
        Backend::Dir(dir) => write_file(&dir, slot, value),
        #[cfg(windows)]
        Backend::CredentialManager => write_credential(slot, value),
    }
}

pub fn clear(slot: &str) -> Result<(), String> {
    if !valid_slot(slot) {
        return Err("Secret slot name is not valid.".into());
    }
    match backend() {
        Backend::Dir(dir) => {
            let path = dir.join(slot);
            if path.is_file() {
                std::fs::remove_file(path)
                    .map_err(|e| format!("Could not forget the secret: {e}"))?;
            }
            Ok(())
        }
        #[cfg(windows)]
        Backend::CredentialManager => delete_credential(slot),
    }
}

fn valid_slot(slot: &str) -> bool {
    let len = slot.len();
    (1..=64).contains(&len)
        && slot
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

enum Backend {
    Dir(PathBuf),
    #[cfg(windows)]
    CredentialManager,
}

fn backend() -> Backend {
    if let Ok(dir) = std::env::var("ATLAS_SECRET_DIR") {
        if !dir.is_empty() {
            return Backend::Dir(PathBuf::from(dir));
        }
    }
    #[cfg(windows)]
    {
        Backend::CredentialManager
    }
    #[cfg(not(windows))]
    {
        Backend::Dir(crate::index::data_dir().join("secrets"))
    }
}

fn read_file(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value = text.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn write_file(dir: &Path, slot: &str, value: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not store the secret: {e}"))?;
    let path = dir.join(slot);
    std::fs::write(&path, value).map_err(|e| format!("Could not store the secret: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

#[cfg(windows)]
fn target_name(slot: &str) -> Vec<u16> {
    format!("NativeFileAtlas:{slot}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn write_credential(slot: &str, value: &str) -> Result<(), String> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let mut target = target_name(slot);
    let mut user = wide(slot);
    let mut comment = wide("Atlas machine secret. Not stored in a workbook.");
    let mut blob = value.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        Comment: PWSTR(comment.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    unsafe { CredWriteW(&cred, 0) }.map_err(|e| format!("Could not store the secret: {e}"))?;
    blob.fill(0);
    Ok(())
}

#[cfg(windows)]
fn read_credential(slot: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredFree, CredReadW, CRED_TYPE_GENERIC};

    let target = target_name(slot);
    let mut cred = std::ptr::null_mut();
    unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            Some(0),
            &mut cred,
        )
    }
    .ok()?;
    if cred.is_null() {
        return None;
    }
    let value = unsafe {
        let header = &*cred;
        let len = header.CredentialBlobSize as usize;
        let bytes = std::slice::from_raw_parts(header.CredentialBlob, len);
        let text = String::from_utf8_lossy(bytes).trim().to_string();
        CredFree(cred as *const core::ffi::c_void);
        text
    };
    (!value.is_empty()).then_some(value)
}

#[cfg(windows)]
fn delete_credential(slot: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = target_name(slot);
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, Some(0)) } {
        Ok(()) => Ok(()),
        Err(e) => {
            if read_credential(slot).is_none() {
                Ok(())
            } else {
                Err(format!("Could not forget the secret: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// `ATLAS_SECRET_DIR` is process-global, so secret tests take turns.
    static SECRET_TEST: Mutex<()> = Mutex::new(());

    fn isolated_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas_secrets_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_slot_roundtrips_without_touching_the_workbook() {
        let _guard = SECRET_TEST.lock().unwrap();
        let dir = isolated_dir("roundtrip");
        std::env::set_var("ATLAS_SECRET_DIR", &dir);
        assert_eq!(health("cursor-api-key"), SecretHealth::Missing);
        store("cursor-api-key", "  cursor_test_key  ").unwrap();
        assert_eq!(load("cursor-api-key").as_deref(), Some("cursor_test_key"));
        assert_eq!(health("cursor-api-key"), SecretHealth::Ok);
        assert!(dir.join("cursor-api-key").is_file());
        clear("cursor-api-key").unwrap();
        assert_eq!(health("cursor-api-key"), SecretHealth::Missing);
        std::env::remove_var("ATLAS_SECRET_DIR");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_secret_and_a_path_slot_are_refused() {
        let _guard = SECRET_TEST.lock().unwrap();
        let dir = isolated_dir("refuse");
        std::env::set_var("ATLAS_SECRET_DIR", &dir);
        assert!(store("cursor-api-key", "   ").is_err());
        assert!(store("../cursor-api-key", "nope").is_err());
        assert_eq!(health("cursor-api-key"), SecretHealth::Missing);
        std::env::remove_var("ATLAS_SECRET_DIR");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_webview_folder_and_secret_store_are_machine_private() {
        let root = Path::new(r"C:\Users\ada\AppData\Local\NativeFileAtlas");
        assert!(is_machine_private(&root.join("webview2").join("EBWebView")));
        assert!(is_machine_private(
            &root.join("secrets").join("cursor-api-key")
        ));
        assert!(is_machine_private(&root.join("cursor-api-key")));
        assert!(!is_machine_private(&root.join("thumbs").join("a.jpg")));
        assert!(!is_machine_private(Path::new(
            r"D:\boards\reports\webview2\index.html"
        )));
    }
}
