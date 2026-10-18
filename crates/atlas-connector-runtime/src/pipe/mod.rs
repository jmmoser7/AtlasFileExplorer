//! Windows-only byte-mode local IPC. Nonblocking kernel calls are polled only
//! on caller workers; there are no pending overlapped buffers during teardown.

mod stream;

use crate::{
    deadline::Deadline,
    security::{wide, win_error, ExpectedPeer, Handle, PipeSecurity},
    transport::{Cancellation, PeerIdentity, PipeAddress},
};
use std::{io, time::Duration};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{
            ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING,
            GENERIC_READ, GENERIC_WRITE,
        },
        Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_MODE, OPEN_EXISTING,
            PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
        },
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
            GetNamedPipeClientSessionId, GetNamedPipeServerProcessId, GetNamedPipeServerSessionId,
            SetNamedPipeHandleState, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE,
        },
    },
};

const PIPE_BUFFER_BYTES: usize = 64 * 1024;

pub(crate) fn fresh_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    let status = unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status.0 < 0 {
        return Err(io::Error::other("Windows secure random generation failed"));
    }
    let mut text = String::with_capacity(64);
    use std::fmt::Write;
    for byte in bytes {
        let _ = write!(&mut text, "{byte:02x}");
    }
    Ok(text)
}

pub(crate) struct Listener {
    handle: Handle,
    expected: ExpectedPeer,
}

impl Listener {
    pub fn bind(address: &PipeAddress, expected_pid: u32) -> io::Result<Self> {
        let expected = ExpectedPeer::current_user(expected_pid)?;
        let security = PipeSecurity::current_user_only()?;
        let attributes = security.attributes();
        let name = wide(&address.pipe_name());
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                PIPE_BUFFER_BYTES as u32,
                PIPE_BUFFER_BYTES as u32,
                0,
                Some(&attributes),
            )
        };
        if handle.is_invalid() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: Handle(handle),
            expected,
        })
    }

    pub fn accept(self, timeout: Duration, cancel: &Cancellation) -> io::Result<Connection> {
        let budget = Deadline::new(timeout, cancel)?;
        loop {
            budget.check()?;
            self.expected.ensure_alive()?;
            // In NOWAIT mode an initial success can mean "listening". Only
            // ERROR_PIPE_CONNECTED proves that a client has actually connected.
            match unsafe { ConnectNamedPipe(self.handle.0, None) } {
                Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => break,
                Ok(()) => (),
                Err(error) if error.code() == ERROR_PIPE_LISTENING.to_hresult() => (),
                Err(error) => return Err(win_error(error)),
            }
            budget.pause()?;
        }
        let (mut pid, mut session) = (0, 0);
        unsafe { GetNamedPipeClientProcessId(self.handle.0, &mut pid) }.map_err(win_error)?;
        unsafe { GetNamedPipeClientSessionId(self.handle.0, &mut session) }.map_err(win_error)?;
        self.expected.verify(pid, session)?;
        Ok(Connection {
            handle: Some(self.handle),
            expected: self.expected,
        })
    }
}

pub(crate) struct Connection {
    handle: Option<Handle>,
    expected: ExpectedPeer,
}

impl Connection {
    pub fn connect(
        address: &PipeAddress,
        expected_pid: u32,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> io::Result<Self> {
        let expected = ExpectedPeer::current_user(expected_pid)?;
        let budget = Deadline::new(timeout, cancel)?;
        let name = wide(&address.pipe_name());
        let handle = loop {
            budget.check()?;
            expected.ensure_alive()?;
            // Never WaitNamedPipe: each attempt and every kernel IO call returns
            // immediately; deadline/cancellation stays under worker control.
            match unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    None,
                )
            } {
                Ok(handle) => break Handle(handle),
                Err(error)
                    if error.code() == ERROR_PIPE_BUSY.to_hresult()
                        || error.code() == ERROR_FILE_NOT_FOUND.to_hresult() =>
                {
                    budget.pause()?
                }
                Err(error) => return Err(win_error(error)),
            }
        };
        // Check OS identity before any protocol bytes; a pack's own claims are
        // never consulted by this layer. Restrict impersonation even on failure.
        let (mut pid, mut session) = (0, 0);
        unsafe { GetNamedPipeServerProcessId(handle.0, &mut pid) }.map_err(win_error)?;
        unsafe { GetNamedPipeServerSessionId(handle.0, &mut session) }.map_err(win_error)?;
        expected.verify(pid, session)?;
        let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
        unsafe { SetNamedPipeHandleState(handle.0, Some(&mode), None, None) }.map_err(win_error)?;
        Ok(Self {
            handle: Some(handle),
            expected,
        })
    }

    pub fn peer(&self) -> &PeerIdentity {
        &self.expected.identity
    }
    pub fn is_open(&self) -> bool {
        self.handle.is_some()
    }
    pub fn close(&mut self) {
        self.handle.take();
    }

    pub fn stream<'a>(
        &'a mut self,
        timeout: Duration,
        cancel: &'a Cancellation,
    ) -> io::Result<stream::Stream<'a>> {
        let handle = self.handle.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "connector pipe is closed")
        })?;
        stream::Stream::new(handle, &self.expected, timeout, cancel)
    }
}
