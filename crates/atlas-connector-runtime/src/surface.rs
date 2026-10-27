//! Borrowed native application surfaces. This guard never owns an application,
//! sends a close command, saves/discards a document, or terminates a process.
//!
//! Explicit `detach` reports restoration failures and can be retried. `Drop`
//! attempts the same restoration, but cannot run after an abrupt process kill.
//! Applications must provide their own recovery for a crashed host; this is not
//! crash-proof hosting. Native operations belong on the host window's UI thread.
//! Windows may normalize completely off-screen saved placement back onto a
//! monitor during restoration; exact off-screen bounds are not guaranteed.
//! Windows parenting can couple foreign input queues: native embedding remains
//! cooperative, best-effort hosting. Use a companion window for stall isolation.

use std::fmt;

#[cfg(windows)]
mod capture;
#[cfg(windows)]
pub use capture::capture_window_rgba;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::NativeSurface;

/// Physical pixels relative to the host window's client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    UnsupportedPlatform,
    InvalidWindow,
    WrongProcess { expected: u32, actual: u32 },
    WrongThread,
    FocusRequiresConnector,
    AlreadyAttached,
    IdentityChanged,
    HostUnavailable,
    Detached,
    InvalidRect,
    Win32 { operation: &'static str, code: u32 },
}

impl fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                f.write_str("Native application surfaces require Windows.")
            }
            Self::InvalidWindow => f.write_str("The application window is unavailable."),
            Self::WrongProcess { expected, actual } => {
                write!(
                    f,
                    "Window process {actual} does not match expected process {expected}."
                )
            }
            Self::WrongThread => {
                f.write_str("Native surfaces require the host window's UI thread.")
            }
            Self::FocusRequiresConnector => {
                f.write_str("Foreign focus must be requested through the application's connector.")
            }
            Self::AlreadyAttached => f.write_str("The application window is already attached."),
            Self::IdentityChanged => f.write_str("The application window identity changed."),
            Self::HostUnavailable => f.write_str("The host window is unavailable."),
            Self::Detached => f.write_str("The application window is detached."),
            Self::InvalidRect => {
                f.write_str("The application surface dimensions must be positive.")
            }
            Self::Win32 { operation, code } => {
                write!(f, "{operation} failed (Windows error {code}).")
            }
        }
    }
}

impl std::error::Error for SurfaceError {}

#[cfg(not(windows))]
pub struct NativeSurface;

#[cfg(not(windows))]
impl NativeSurface {
    pub fn attach(_: isize, _: u32, _: isize) -> Result<Self, SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn attach_full_ui(_: isize, _: u32, _: isize) -> Result<Self, SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn process_id(&self) -> Result<u32, SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn handle(&self) -> Result<isize, SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn place(&self, _: SurfaceRect) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn place_clipped(&self, _: SurfaceRect, _: SurfaceRect) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn place_clipped_ready(
        &self,
        _: SurfaceRect,
        _: SurfaceRect,
    ) -> Result<bool, SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn park(&self) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn host_is_foreground(&self) -> Result<bool, SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn hide(&self) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn focus(&self) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn blur(&self) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn detach(&mut self) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }

    pub fn detach_hidden(&mut self) -> Result<(), SurfaceError> {
        Err(SurfaceError::UnsupportedPlatform)
    }
}
