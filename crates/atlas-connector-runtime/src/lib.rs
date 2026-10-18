//! OS adapters for the application connector contract.
//!
//! Blocking transport belongs on workers. Native surface methods belong on
//! the window's host thread. Neither layer owns a foreign application's edits.

pub mod artifact;
pub(crate) mod deadline;
pub mod host;
#[cfg(windows)]
mod pipe;
#[cfg(windows)]
mod security;
pub mod surface;
pub mod transport;
