use super::PIPE_BUFFER_BYTES;
use crate::{
    deadline::Deadline,
    security::{win_error, ExpectedPeer, Handle},
    transport::Cancellation,
};
use std::{
    io::{self, Read, Write},
    time::Duration,
};
use windows::Win32::{
    Foundation::{ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED},
    Storage::FileSystem::{ReadFile, WriteFile},
};

pub(crate) struct Stream<'a> {
    handle: &'a Handle,
    expected: &'a ExpectedPeer,
    budget: Deadline<'a>,
}

impl<'a> Stream<'a> {
    pub fn new(
        handle: &'a Handle,
        expected: &'a ExpectedPeer,
        timeout: Duration,
        cancel: &'a Cancellation,
    ) -> io::Result<Self> {
        Ok(Self {
            handle,
            expected,
            budget: Deadline::new(timeout, cancel)?,
        })
    }
}

impl Read for Stream<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            self.budget.check()?;
            let mut count = 0;
            match unsafe { ReadFile(self.handle.0, Some(buffer), Some(&mut count), None) } {
                Ok(()) if count != 0 => return Ok(count as usize),
                Ok(()) => (),
                Err(error)
                    if error.code() == ERROR_BROKEN_PIPE.to_hresult()
                        || error.code() == ERROR_PIPE_NOT_CONNECTED.to_hresult() =>
                {
                    return Ok(0)
                }
                Err(error) if error.code() == ERROR_NO_DATA.to_hresult() => (),
                Err(error) => return Err(win_error(error)),
            }
            self.expected.ensure_alive()?;
            self.budget.pause()?;
        }
    }
}

impl Write for Stream<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        // Keep NOWAIT kernel writes within the pipe buffer so large frames can
        // progress while the peer polls. Framing's write_all advances the body.
        let buffer = &buffer[..buffer.len().min(PIPE_BUFFER_BYTES)];
        loop {
            self.budget.check()?;
            let mut count = 0;
            match unsafe { WriteFile(self.handle.0, Some(buffer), Some(&mut count), None) } {
                Ok(()) if count != 0 => return Ok(count as usize),
                Ok(()) => (),
                Err(error) => return Err(win_error(error)),
            }
            self.expected.ensure_alive()?;
            self.budget.pause()?;
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        // FlushFileBuffers waits for the remote application and cannot be used
        // on this bounded path. write_frame needs no out-of-band flush operation.
        self.budget.check()
    }
}
