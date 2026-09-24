//! Local Cursor user API key — so the sidecar can start without a system env var.
//!
//! The key is a machine secret ([`atlas_core::secrets`]), never journaled and
//! never written into a workbook. On Windows it is stored in Credential
//! Manager for this user. `CURSOR_API_KEY` still wins when set. A plaintext
//! file left by an older build is copied into that store on the next read
//! and then deleted.

use std::path::PathBuf;

use atlas_core::secrets::{self, SecretHealth};

/// Cursor dashboard page that mints a user API key.
pub const DASHBOARD_URL: &str = "https://cursor.com/dashboard/api";

/// Official write-up of how the key is used.
pub const AUTH_DOCS_URL: &str = "https://cursor.com/docs/cli/reference/authentication";

/// Slot name inside [`atlas_core::secrets`]. Not a secret by itself.
pub const SLOT: &str = "cursor-api-key";

/// Env, then the per-user secret store.
pub fn resolve() -> Option<String> {
    if let Some(key) = env_key() {
        return Some(key);
    }
    if let Some(path) = override_path() {
        return read_file(&path);
    }
    if let Some(key) = secrets::load(SLOT) {
        let _ = std::fs::remove_file(legacy_path());
        return Some(key);
    }
    migrate_legacy()
}

pub fn is_configured() -> bool {
    env_key().is_some()
        || override_path().is_some_and(|path| path.is_file())
        || secrets::health(SLOT) == SecretHealth::Ok
        || legacy_path().is_file()
}

pub fn save(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Paste the API key first.".into());
    }
    if let Some(path) = override_path() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, key).map_err(|e| format!("Could not save API key: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        return Ok(());
    }
    secrets::store(SLOT, key)?;
    let _ = std::fs::remove_file(legacy_path());
    Ok(())
}

fn env_key() -> Option<String> {
    std::env::var("CURSOR_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn override_path() -> Option<PathBuf> {
    let path = std::env::var_os("ATLAS_CURSOR_KEY_PATH")?;
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn legacy_path() -> PathBuf {
    atlas_core::index::data_dir().join("cursor-api-key")
}

fn read_file(path: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let key = text.trim();
    (!key.is_empty()).then(|| key.to_string())
}

fn migrate_legacy() -> Option<String> {
    let path = legacy_path();
    let key = read_file(&path)?;
    match secrets::store(SLOT, &key) {
        Ok(()) => {
            let _ = std::fs::remove_file(path);
            Some(key)
        }
        Err(_) => Some(key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "atlas_cursor_key_{tag}_{}_{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn save_then_load_from_the_override_path() {
        let path = unique_path("roundtrip");
        std::env::set_var("ATLAS_CURSOR_KEY_PATH", &path);
        save("cursor_test_key").unwrap();
        let loaded = std::fs::read_to_string(&path).unwrap();
        assert_eq!(loaded.trim(), "cursor_test_key");
        let _ = std::fs::remove_file(path);
        std::env::remove_var("ATLAS_CURSOR_KEY_PATH");
    }
}
