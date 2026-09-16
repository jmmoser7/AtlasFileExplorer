//! System folder pickers.
//!
//! `IFileOpenDialog` stays off the UI thread so the frame loop keeps pumping
//! (Art. II). On Windows the Atlas HWND is the dialog owner — `Show(NULL)`
//! often opens behind the undecorated window, and clicking Atlas then looks
//! like a freeze.

use std::path::PathBuf;

/// HWND of the Atlas window, stored as an integer so it can ride a picker
/// thread. `RawWindowHandle` itself is not `Send`.
#[derive(Clone, Copy)]
pub(crate) struct DialogOwner {
    #[cfg(windows)]
    hwnd: std::num::NonZeroIsize,
}

impl DialogOwner {
    pub(crate) fn from_window(handle: &impl raw_window_handle::HasWindowHandle) -> Option<Self> {
        #[cfg(windows)]
        {
            match handle.window_handle().ok()?.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(win32) => Some(Self { hwnd: win32.hwnd }),
                _ => None,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = handle;
            None
        }
    }
}

#[cfg(windows)]
impl raw_window_handle::HasWindowHandle for DialogOwner {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::Win32WindowHandle::new(self.hwnd);
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(raw_window_handle::RawWindowHandle::Win32(
                handle,
            ))
        })
    }
}

#[cfg(windows)]
impl raw_window_handle::HasDisplayHandle for DialogOwner {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(
                raw_window_handle::RawDisplayHandle::Windows(
                    raw_window_handle::WindowsDisplayHandle::new(),
                ),
            )
        })
    }
}

pub(crate) fn pick_folders(title: &str, owner: Option<DialogOwner>) -> Option<Vec<PathBuf>> {
    let mut dlg = rfd::FileDialog::new().set_title(title);
    #[cfg(windows)]
    if let Some(owner) = owner {
        dlg = dlg.set_parent(&owner);
    }
    dlg.pick_folders()
}

pub(crate) fn pick_folder(title: &str, owner: Option<DialogOwner>) -> Option<PathBuf> {
    let mut dlg = rfd::FileDialog::new().set_title(title);
    #[cfg(windows)]
    if let Some(owner) = owner {
        dlg = dlg.set_parent(&owner);
    }
    dlg.pick_folder()
}
