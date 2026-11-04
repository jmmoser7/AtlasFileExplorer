//! Saved native ownership and placement, restored by the shared surface guard.
use super::*;
use windows::Win32::Graphics::Gdi::ScreenToClient;

impl SavedWindow {
    pub(super) fn read(child: HWND) -> Result<Self, SurfaceError> {
        let style = get_long(child, GWL_STYLE)?;
        let exstyle = get_long(child, GWL_EXSTYLE)?;
        let parent = unsafe { GetParent(child) }.ok();
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        let mut rect = RECT::default();
        unsafe { GetWindowPlacement(child, &mut placement) }
            .map_err(|_| win32("GetWindowPlacement"))?;
        unsafe { GetWindowRect(child, &mut rect) }.map_err(|_| win32("GetWindowRect"))?;
        Ok(Self {
            parent: if style as u32 & WS_CHILD.0 != 0 {
                parent
            } else {
                None
            },
            owner: if style as u32 & WS_CHILD.0 == 0 {
                get_long(child, GWLP_HWNDPARENT)?
            } else {
                0
            },
            style,
            exstyle,
            placement,
            rect,
            // IsWindowVisible also tests ancestors. Preserve this window's own
            // visibility bit when its original parent happens to be hidden.
            visible: style as u32 & WS_VISIBLE.0 != 0,
            region: WindowRegion::read(child)?,
        })
    }

    pub(super) fn restore(&self, child: HWND, restore_visible: bool) -> Result<(), SurfaceError> {
        if !restore_visible {
            let _ = unsafe { ShowWindowAsync(child, SW_HIDE) };
        }
        self.region.restore(child)?;
        set_parent(child, self.parent)?;
        let style = if restore_visible {
            self.style
        } else {
            self.style & !(WS_VISIBLE.0 as isize)
        };
        set_long(child, GWL_STYLE, style)?;
        set_long(child, GWL_EXSTYLE, self.exstyle)?;
        if self.style as u32 & WS_CHILD.0 == 0 {
            set_long(child, GWLP_HWNDPARENT, self.owner)?;
        }
        let mut origin = POINT {
            x: self.rect.left,
            y: self.rect.top,
        };
        if let Some(parent) = self.parent {
            if !unsafe { ScreenToClient(parent, &mut origin).as_bool() } {
                return Err(win32("ScreenToClient"));
            }
        }
        unsafe {
            SetWindowPos(
                child,
                None,
                origin.x,
                origin.y,
                self.rect.right - self.rect.left,
                self.rect.bottom - self.rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        }
        .map_err(|_| win32("SetWindowPos"))?;
        let mut placement = self.placement;
        if !restore_visible {
            placement.showCmd = SW_HIDE.0 as u32;
        }
        unsafe { SetWindowPlacement(child, &placement) }
            .map_err(|_| win32("SetWindowPlacement"))?;
        unsafe {
            let _ = ShowWindow(
                child,
                if self.visible && restore_visible {
                    SHOW_WINDOW_CMD(self.placement.showCmd as i32)
                } else {
                    SW_HIDE
                },
            );
        }
        Ok(())
    }
}
