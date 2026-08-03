//! Native filesystem metadata helpers used during scanning.
//!
//! Modified and created times come from the same `Metadata` fetch as size.
//! Owner lookup uses Win32 security APIs on Windows only (one extra call per
//! file during scan).

use std::path::Path;
use std::time::UNIX_EPOCH;

pub fn mtime_of(md: &std::fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Creation / birth time when the platform exposes it; falls back to modified.
pub fn ctime_of(md: &std::fs::Metadata) -> i64 {
    md.created()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .or_else(|| {
            md.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
        })
        .unwrap_or(0)
}

/// Account name for the file owner (e.g. `jmoser`), empty when unavailable.
///
/// **Not for use inside a directory walk.** This is two Win32 round trips per
/// file — a security-descriptor query against the file and a SID translation —
/// measured at 0.26 ms on a local disk and considerably worse over SMB, where
/// the descriptor query is its own request/response. Calling it per entry made
/// it the dominant cost of discovery. The scanner leaves `owner` empty and
/// [`crate::owners`] fills it in afterwards.
pub fn owner_short(path: &Path) -> String {
    owner_short_impl(path)
}

#[cfg(windows)]
fn owner_short_impl(path: &Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows::Win32::Security::{OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut owner_sid = PSID(std::ptr::null_mut());
        // The descriptor out-param is not optional: the returned SID points
        // *into* that buffer, and the caller owns it. Passing null here left
        // Windows holding one allocation per file, which a 20k-file scan turned
        // into a 20k-descriptor leak.
        let mut sd = PSECURITY_DESCRIPTOR::default();
        if GetNamedSecurityInfoW(
            windows::core::PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner_sid),
            None,
            None,
            None,
            &mut sd,
        )
        .is_err()
        {
            return String::new();
        }
        let name = if owner_sid.0.is_null() {
            String::new()
        } else {
            name_for_sid(owner_sid)
        };
        if !sd.is_invalid() {
            let _ = LocalFree(Some(HLOCAL(sd.0)));
        }
        name
    }
}

/// Translate a SID to an account name, memoized.
///
/// Worth caching hard: for a domain SID this call can leave the machine to ask
/// a domain controller, and a directory's files nearly always share one owner,
/// so the uncached version paid a possible network round trip thousands of times
/// for one answer.
#[cfg(windows)]
unsafe fn name_for_sid(sid: windows::Win32::Security::PSID) -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use windows::core::PWSTR;
    use windows::Win32::Security::{GetLengthSid, IsValidSid, LookupAccountSidW, SID_NAME_USE};

    if !IsValidSid(sid).as_bool() {
        return String::new();
    }
    // Key on the SID's bytes; the pointer itself is only valid for this call.
    let len = GetLengthSid(sid) as usize;
    let key: Vec<u8> = std::slice::from_raw_parts(sid.0 as *const u8, len).to_vec();

    static CACHE: OnceLock<Mutex<HashMap<Vec<u8>, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }

    let mut name_len = 0u32;
    let mut domain_len = 0u32;
    let mut use_type = SID_NAME_USE::default();
    // First call sizes the buffers; it is expected to fail.
    let _ = LookupAccountSidW(
        None,
        sid,
        Some(PWSTR::null()),
        &mut name_len,
        Some(PWSTR::null()),
        &mut domain_len,
        &mut use_type,
    );
    if name_len == 0 {
        return String::new();
    }

    let mut name_buf = vec![0u16; name_len as usize];
    let mut domain_buf = vec![0u16; domain_len.max(1) as usize];
    let resolved = if LookupAccountSidW(
        None,
        sid,
        Some(PWSTR(name_buf.as_mut_ptr())),
        &mut name_len,
        Some(PWSTR(domain_buf.as_mut_ptr())),
        &mut domain_len,
        &mut use_type,
    )
    .is_ok()
    {
        // On success `name_len` excludes the terminator, so it *is* the length.
        // Subtracting one here truncated every owner by a character — the
        // account `jmoser` was reported as `jmose`.
        String::from_utf16_lossy(&name_buf[..name_len as usize])
            .trim()
            .to_ascii_lowercase()
    } else {
        String::new()
    };

    if let Ok(mut c) = cache.lock() {
        c.insert(key, resolved.clone());
    }
    resolved
}

#[cfg(not(windows))]
fn owner_short_impl(_path: &Path) -> String {
    String::new()
}

/// Last path segment of a `DOMAIN\account` string; identity for plain names.
pub fn owner_display(account: &str) -> &str {
    account.rsplit('\\').next().unwrap_or(account)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_display_takes_the_account_off_a_qualified_name() {
        assert_eq!(owner_display(r"OFFICE\jmoser"), "jmoser");
        assert_eq!(owner_display("jmoser"), "jmoser");
        assert_eq!(owner_display(""), "");
    }

    /// The name buffer length reported by `LookupAccountSidW` on success
    /// *excludes* the terminator, so the old `name_len - 1` slice silently
    /// dropped the last character of every owner: `jmoser` was stored, filtered
    /// and displayed as `jmose`.
    #[test]
    #[cfg(windows)]
    fn a_new_files_owner_is_the_current_account_in_full() {
        let dir = std::env::temp_dir().join(format!("atlas_owner_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mine.txt");
        std::fs::write(&file, b"x").unwrap();

        let owner = owner_short(&file);
        assert!(
            !owner.is_empty(),
            "a local file should have a resolvable owner"
        );

        // An elevated process creates files owned by Administrators instead, so
        // only compare when we are plainly the owner.
        let username = std::env::var("USERNAME").unwrap_or_default().to_lowercase();
        if !username.is_empty() && owner != "administrators" {
            assert_eq!(owner, username, "owner name was truncated or mangled");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
