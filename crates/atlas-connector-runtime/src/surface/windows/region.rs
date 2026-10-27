//! Reversible outer-window clipping for borrowed full-UI surfaces.
use super::*;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, GetWindowRgn, SetWindowRgn, HRGN, RGN_COPY,
};

pub(super) struct WindowRegion(Option<HRGN>);

impl WindowRegion {
    pub(super) fn read(window: HWND) -> Result<Self, SurfaceError> {
        let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
        if region.is_invalid() {
            return Err(win32("CreateRectRgn"));
        }
        if unsafe { GetWindowRgn(window, region) }.0 == 0 {
            let _ = unsafe { DeleteObject(region.into()) };
            Ok(Self(None))
        } else {
            Ok(Self(Some(region)))
        }
    }

    pub(super) fn clip(
        window: HWND,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    ) -> Result<(), SurfaceError> {
        let region = unsafe { CreateRectRgn(left, top, right, bottom) };
        if region.is_invalid() {
            return Err(win32("CreateRectRgn"));
        }
        Self::install(window, Some(region))
    }

    pub(super) fn restore(&self, window: HWND) -> Result<(), SurfaceError> {
        let copy = if let Some(saved) = self.0 {
            let copy = unsafe { CreateRectRgn(0, 0, 0, 0) };
            if copy.is_invalid() {
                return Err(win32("CreateRectRgn"));
            }
            if unsafe { CombineRgn(Some(copy), Some(saved), None, RGN_COPY) }.0 == 0 {
                let _ = unsafe { DeleteObject(copy.into()) };
                return Err(win32("CombineRgn"));
            }
            Some(copy)
        } else {
            None
        };
        Self::install(window, copy)
    }

    fn install(window: HWND, region: Option<HRGN>) -> Result<(), SurfaceError> {
        if unsafe { SetWindowRgn(window, region, true) } == 0 {
            if let Some(region) = region {
                let _ = unsafe { DeleteObject(region.into()) };
            }
            return Err(win32("SetWindowRgn"));
        }
        // A successful SetWindowRgn transfers ownership to Windows.
        Ok(())
    }
}

impl Drop for WindowRegion {
    fn drop(&mut self) {
        if let Some(region) = self.0 {
            let _ = unsafe { DeleteObject(region.into()) };
        }
    }
}
