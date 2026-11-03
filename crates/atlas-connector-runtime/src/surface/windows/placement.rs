//! Shared clipped placement and reversible hidden parking for borrowed HWNDs.
use super::*;
use windows::Win32::Graphics::Gdi::ClientToScreen;

struct Placement {
    origin: POINT,
    expected: RECT,
    width: i32,
    height: i32,
    clip: [i32; 4],
}
impl Placement {
    fn new(
        surface: &NativeSurface,
        rect: SurfaceRect,
        clip: SurfaceRect,
    ) -> Result<Self, SurfaceError> {
        surface.validate_host()?;
        if rect.width <= 0 || rect.height <= 0 || clip.width <= 0 || clip.height <= 0 {
            return Err(SurfaceError::InvalidRect);
        }
        let left = i64::from(rect.x).max(i64::from(clip.x)) - i64::from(rect.x);
        let top = i64::from(rect.y).max(i64::from(clip.y)) - i64::from(rect.y);
        let right = (i64::from(rect.x) + i64::from(rect.width))
            .min(i64::from(clip.x) + i64::from(clip.width))
            - i64::from(rect.x);
        let bottom = (i64::from(rect.y) + i64::from(rect.height))
            .min(i64::from(clip.y) + i64::from(clip.height))
            - i64::from(rect.y);
        if left >= right || top >= bottom {
            return Err(SurfaceError::InvalidRect);
        }
        let mut screen = POINT {
            x: rect.x,
            y: rect.y,
        };
        if !unsafe { ClientToScreen(surface.host, &mut screen).as_bool() } {
            return Err(win32("ClientToScreen"));
        }
        let expected = RECT {
            left: screen.x,
            top: screen.y,
            right: screen
                .x
                .checked_add(rect.width)
                .ok_or(SurfaceError::InvalidRect)?,
            bottom: screen
                .y
                .checked_add(rect.height)
                .ok_or(SurfaceError::InvalidRect)?,
        };
        Ok(Self {
            origin: if surface.kind == AttachmentKind::OwnedFullUi {
                screen
            } else {
                POINT {
                    x: rect.x,
                    y: rect.y,
                }
            },
            expected,
            width: rect.width,
            height: rect.height,
            clip: [left as i32, top as i32, right as i32, bottom as i32],
        })
    }
    fn matches(&self, child: HWND) -> Result<bool, SurfaceError> {
        let mut actual = RECT::default();
        unsafe { GetWindowRect(child, &mut actual) }.map_err(|_| win32("GetWindowRect"))?;
        Ok(actual.left == self.expected.left
            && actual.top == self.expected.top
            && actual.right == self.expected.right
            && actual.bottom == self.expected.bottom)
    }
    fn clip(&self, child: HWND) -> Result<(), SurfaceError> {
        WindowRegion::clip(
            child,
            self.clip[0],
            self.clip[1],
            self.clip[2],
            self.clip[3],
        )
    }
    fn position(&self, child: HWND, show: bool) -> Result<(), SurfaceError> {
        let flags = SWP_NOZORDER
            | SWP_NOACTIVATE
            | SWP_ASYNCWINDOWPOS
            | if show {
                SWP_SHOWWINDOW
            } else {
                SET_WINDOW_POS_FLAGS(0)
            };
        unsafe {
            SetWindowPos(
                child,
                None,
                self.origin.x,
                self.origin.y,
                self.width,
                self.height,
                flags,
            )
        }
        .map_err(|_| win32("SetWindowPos"))
    }
}

impl NativeSurface {
    /// An owned native UI counts as part of its authenticated host. Unrelated
    /// foreground applications must never be activated over by queued focus.
    pub fn host_is_foreground(&self) -> Result<bool, SurfaceError> {
        self.validate_host()?;
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_invalid() {
            return Ok(false);
        }
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(foreground, Some(&mut pid));
        }
        Ok(pid == unsafe { GetCurrentProcessId() } || pid == self.pid)
    }
    /// Fits the outer window to host-client physical pixels without focusing it.
    pub fn place(&self, rect: SurfaceRect) -> Result<(), SurfaceError> {
        self.place_clipped(rect, rect)
    }
    pub fn place_clipped(&self, rect: SurfaceRect, clip: SurfaceRect) -> Result<(), SurfaceError> {
        let placement = Placement::new(self, rect, clip)?;
        placement.clip(self.child)?;
        placement.position(self.child, true)
    }
    /// Keep ownership and an empty region while asynchronous placement is
    /// pending. Returns true only once actual OS bounds match the requested
    /// host rectangle and the visible clip has been installed.
    pub fn place_clipped_ready(
        &self,
        rect: SurfaceRect,
        clip: SurfaceRect,
    ) -> Result<bool, SurfaceError> {
        let placement = Placement::new(self, rect, clip)?;
        if !placement.matches(self.child)? {
            self.park()?;
            placement.position(self.child, false)?;
            return Ok(false);
        }
        placement.clip(self.child)?;
        placement.position(self.child, true)?;
        Ok(true)
    }
    /// Retain the authenticated host slot with no visible/input region. A
    /// queued foreign Show cannot expose the original desktop window bounds.
    pub fn park(&self) -> Result<(), SurfaceError> {
        self.validate_host()?;
        WindowRegion::clip(self.child, 0, 0, 0, 0)?;
        self.hide()
    }
}
