//! Local Cursor user API key — so the sidecar can start without a system env var.
//!
//! The key is never journaled and never written into a workbook. Environment
//! `CURSOR_API_KEY` still wins when set.

use std::path::PathBuf;

/// Cursor dashboard page that mints a user API key.
pub const DASHBOARD_URL: &str = "https://cursor.com/dashboard/api";

/// Official write-up of how the key is used.
pub const AUTH_DOCS_URL: &str = "https://cursor.com/docs/cli/reference/authentication";

/// Env, then the per-user file next to `ai-config.json`.
pub fn resolve() -> Option<String> {
    env_key().or_else(load_stored)
}

pub fn is_configured() -> bool {
    env_key().is_some() || key_path().is_file()
}

pub fn save(key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Paste the API key first.".into());
    }
    let path = key_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, key).map_err(|e| format!("Could not save API key: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn env_key() -> Option<String> {
    std::env::var("CURSOR_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn load_stored() -> Option<String> {
    let text = std::fs::read_to_string(key_path()).ok()?;
    let key = text.trim();
    (!key.is_empty()).then(|| key.to_string())
}

fn key_path() -> PathBuf {
    if let Ok(p) = std::env::var("ATLAS_CURSOR_KEY_PATH") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    atlas_core::index::data_dir().join("cursor-api-key")
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
        let loaded = load_stored().expect("stored key");
        assert_eq!(loaded, "cursor_test_key");
        let _ = std::fs::remove_file(path);
        std::env::remove_var("ATLAS_CURSOR_KEY_PATH");
    }
}
