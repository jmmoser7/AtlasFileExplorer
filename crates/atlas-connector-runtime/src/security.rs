//! The Windows ABI owner for local transport identities and pipe ACLs.

use crate::transport::PeerIdentity;
use std::{io, mem::size_of};
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, LocalFree, FILETIME, HANDLE, HLOCAL, WAIT_TIMEOUT},
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            },
            GetTokenInformation, TokenSessionId, TokenUser, PSECURITY_DESCRIPTOR,
            SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
        },
        System::Threading::{
            GetCurrentProcessId, GetProcessTimes, OpenProcess, OpenProcessToken,
            WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        },
    },
};

/// Unique kernel handle. It may move to a worker but is never shared concurrently.
pub(crate) struct Handle(pub HANDLE);
// SAFETY: all owned handles here are kernel pipe/process/token objects; moving
// their unique ownership between threads is supported by Windows.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(crate) fn win_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0 & 0xffff)
}

pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) struct ExpectedPeer {
    pub identity: PeerIdentity,
    process: Handle,
}

impl ExpectedPeer {
    pub fn current_user(pid: u32) -> io::Result<Self> {
        let (process, identity) = identity_for(pid)?;
        let (_, local) = identity_for(unsafe { GetCurrentProcessId() })?;
        if identity.user_sid != local.user_sid || identity.session_id != local.session_id {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "connector peer must have the current OS user and session",
            ));
        }
        let peer = Self { identity, process };
        peer.ensure_alive()?;
        Ok(peer)
    }

    pub fn ensure_alive(&self) -> io::Result<()> {
        if unsafe { WaitForSingleObject(self.process.0, 0) } != WAIT_TIMEOUT {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "connector peer process exited",
            ));
        }
        Ok(())
    }

    pub fn verify(&self, pid: u32, session_id: u32) -> io::Result<()> {
        self.ensure_alive()?;
        if pid != self.identity.process_id || session_id != self.identity.session_id {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "unexpected named-pipe peer",
            ));
        }
        let (_, observed) = identity_for(pid)?;
        if observed != self.identity {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "named-pipe process identity changed",
            ));
        }
        Ok(())
    }
}

fn identity_for(pid: u32) -> io::Result<(Handle, PeerIdentity)> {
    let process = Handle(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .map_err(win_error)?,
    );
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) }.map_err(win_error)?;
    let token = Handle(token);
    let user_sid = token_sid(token.0)?;
    let mut session_id = 0u32;
    let mut returned = 0;
    unsafe {
        GetTokenInformation(
            token.0,
            TokenSessionId,
            Some((&mut session_id as *mut u32).cast()),
            size_of::<u32>() as u32,
            &mut returned,
        )
    }
    .map_err(win_error)?;
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe { GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .map_err(win_error)?;
    let created_at_100ns = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    Ok((
        process,
        PeerIdentity {
            process_id: pid,
            session_id,
            user_sid,
            created_at_100ns,
        },
    ))
}

fn token_sid(token: HANDLE) -> io::Result<String> {
    let mut needed = 0;
    // Query size first; the expected ERROR_INSUFFICIENT_BUFFER still fills needed.
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
    if needed < size_of::<TOKEN_USER>() as u32 || needed > 65536 {
        return Err(io::Error::other("invalid token user size"));
    }
    // Pointer-bearing TOKEN_USER requires native alignment, unlike a Vec<u8>.
    let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
    }
    .map_err(win_error)?;
    let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    let mut text = PWSTR::null();
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) }.map_err(win_error)?;
    let result =
        unsafe { text.to_string() }.map_err(|_| io::Error::other("invalid Windows SID text"));
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
    }
    result
}

pub(crate) struct PipeSecurity(PSECURITY_DESCRIPTOR);

impl PipeSecurity {
    pub fn current_user_only() -> io::Result<Self> {
        let (_, current) = identity_for(unsafe { GetCurrentProcessId() })?;
        // Protected DACL: exactly one allow ACE, for the actual TokenUser SID.
        // No inherited, Everyone, Anonymous, Network, or default-DACL access.
        let sddl = wide(&format!("D:P(A;;GA;;;{})", current.user_sid));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                1,
                &mut descriptor,
                None,
            )
        }
        .map_err(win_error)?;
        Ok(Self(descriptor))
    }

    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0 .0,
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0 .0)));
        }
    }
}
