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
pub fn owner_short(path: &Path) -> String {
    owner_short_impl(path)
}

#[cfg(windows)]
fn owner_short_impl(path: &Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PWSTR;
    use windows::Win32::Security::Authorization::{GetNamedSecurityInfoW, LookupAccountSidW};
    use windows::Win32::Security::{OWNER_SECURITY_INFORMATION, SE_FILE_OBJECT};

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut owner_sid = windows::Win32::Security::PSID::default();
        if GetNamedSecurityInfoW(
            windows::core::PCWSTR(wide.as_ptr()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner_sid as *mut _ as *mut _),
            None,
            None,
            None,
            None,
        )
        .is_err()
        {
            return String::new();
        }
        if owner_sid.is_invalid() {
            return String::new();
        }

        let mut name_len = 0u32;
        let mut domain_len = 0u32;
        let mut use_type = windows::Win32::Security::SID_NAME_USE::default();
        let _ = LookupAccountSidW(
            None,
            owner_sid,
            PWSTR::null(),
            &mut name_len,
            PWSTR::null(),
            &mut domain_len,
            &mut use_type,
        );

        if name_len == 0 {
            return String::new();
        }

        let mut name_buf = vec![0u16; name_len as usize];
        let mut domain_buf = vec![0u16; domain_len as usize];
        if LookupAccountSidW(
            None,
            owner_sid,
            PWSTR(name_buf.as_mut_ptr()),
            &mut name_len,
            PWSTR(domain_buf.as_mut_ptr()),
            &mut domain_len,
            &mut use_type,
        )
        .is_err()
        {
            return String::new();
        }

        let account = String::from_utf16_lossy(&name_buf[..name_len.saturating_sub(1) as usize]);
        account.trim().to_ascii_lowercase()
    }
}

#[cfg(not(windows))]
fn owner_short_impl(_path: &Path) -> String {
    String::new()
}

/// Last path segment of a `DOMAIN\account` string; identity for plain names.
pub fn owner_display(account: &str) -> &str {
    account.rsplit('\\').next().unwrap_or(account)
}
