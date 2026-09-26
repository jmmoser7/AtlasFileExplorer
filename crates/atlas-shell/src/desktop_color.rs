//! One desktop sampler for all color editors. Capture and the native modal input
//! loop run on a worker; no captured image is retained after the pick session.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub rgb: [u8; 3],
    pub alt: bool,
}
pub type PickResult = Result<Option<Sample>, String>;
pub struct DesktopColorPicker {
    rx: mpsc::Receiver<PickResult>,
    cancel: Arc<AtomicBool>,
}
impl DesktopColorPicker {
    pub fn begin(cancel_on_alt_release: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let _ = tx.send(platform::pick(stop, cancel_on_alt_release));
        });
        Self { rx, cancel }
    }
    pub fn poll(&self) -> Option<PickResult> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Desktop color sampling ended unexpectedly".into()))
            }
        }
    }
}
impl Drop for DesktopColorPicker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg_attr(not(windows), allow(unused_imports))]
use layout::*;

/// Sampler geometry, pure so it is tested on every platform.
#[cfg_attr(not(windows), allow(dead_code))]
mod layout {
    /// Captured pixels on each side of the magnifier: an odd count, so one
    /// cell is the pixel under the hotspot.
    pub(super) const LOUPE_PATCH: i32 = 11;
    /// Physical screen pixels per magnified pixel.
    pub(super) const LOUPE_CELL: i32 = 10;
    pub(super) const LOUPE_LABEL: [i32; 2] = [150, 16];

    /// The sampler's magnifier, in physical pixels of the capture.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) struct Loupe {
        /// Top-left of the captured patch it magnifies.
        pub patch: [i32; 2],
        /// Top-left of the magnified patch on screen.
        pub at: [i32; 2],
        /// The framed cell: the pixel a release samples.
        pub marked: [i32; 2],
        /// Top-left of the RGB readout.
        pub label: [i32; 2],
    }

    impl Loupe {
        pub fn side() -> i32 {
            LOUPE_PATCH * LOUPE_CELL
        }
    }

    /// The framed cell is centered on the hotspot, so what the magnifier marks
    /// is what the cursor points at. Near a desktop edge the patch stops at the
    /// capture and the magnifier slides past the edge rather than off the
    /// hotspot.
    pub(super) fn loupe(cursor: [i32; 2], size: [i32; 2]) -> Loupe {
        let side = Loupe::side();
        let axis = |i: usize| {
            let patch = (cursor[i] - LOUPE_PATCH / 2).clamp(0, (size[i] - LOUPE_PATCH).max(0));
            let marked = cursor[i] - LOUPE_CELL / 2;
            (patch, marked - (cursor[i] - patch) * LOUPE_CELL, marked)
        };
        let (x, y) = (axis(0), axis(1));
        let below = y.1 + side;
        let label_y = if below + LOUPE_LABEL[1] <= size[1] {
            below
        } else {
            y.1 - LOUPE_LABEL[1]
        };
        Loupe {
            patch: [x.0, y.0],
            at: [x.1, y.1],
            marked: [x.2, y.2],
            label: [
                x.1.min(size[0] - LOUPE_LABEL[0]).max(0),
                label_y.clamp(0, (size[1] - LOUPE_LABEL[1]).max(0)),
            ],
        }
    }

    /// The capture pixel under a physical screen point. The capture starts at
    /// the virtual desktop's `origin`, which is negative when a monitor sits
    /// left of or above the primary.
    pub(super) fn capture_pixel(screen: [i32; 2], origin: [i32; 2], size: [i32; 2]) -> [i32; 2] {
        [
            (screen[0] - origin[0]).clamp(0, size[0] - 1),
            (screen[1] - origin[1]).clamp(0, size[1] - 1),
        ]
    }
}

/// One screen pixel under the cursor. `None` off Windows and in tests, so a
/// headless alt-click falls through to the modal sampler.
pub fn sample_cursor() -> Option<[u8; 3]> {
    #[cfg(all(windows, not(test)))]
    {
        platform::sample_cursor()
    }
    #[cfg(not(all(windows, not(test))))]
    {
        None
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn pick(_: Arc<AtomicBool>, _: bool) -> PickResult {
        Err("Desktop sampling is available on Windows".into())
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::{
        core::w,
        Win32::{
            Foundation::*,
            Graphics::Gdi::*,
            System::LibraryLoader::GetModuleHandleW,
            UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
        },
    };

    struct Session {
        dc: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
        origin: POINT,
        width: i32,
        height: i32,
        cursor: POINT,
        down: bool,
        result: Option<Sample>,
        done: bool,
        stop: Arc<AtomicBool>,
        alt: bool,
    }
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                SelectObject(self.dc, self.previous);
                let _ = DeleteObject(self.bitmap.into());
                let _ = DeleteDC(self.dc);
            }
        }
    }
    impl Session {
        unsafe fn position(&mut self) {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_ok() {
                let [x, y] = capture_pixel(
                    [p.x, p.y],
                    [self.origin.x, self.origin.y],
                    [self.width, self.height],
                );
                self.cursor = POINT { x, y };
            }
        }
        unsafe fn candidate(&self) -> Option<[u8; 3]> {
            let color = GetPixel(self.dc, self.cursor.x, self.cursor.y).0;
            (color != 0xffffffff).then_some([color as u8, (color >> 8) as u8, (color >> 16) as u8])
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            return LRESULT(1);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Session;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let state = &mut *ptr;
        match msg {
            WM_MOUSEMOVE => {
                state.position();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                state.down = true;
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if state.down {
                    state.position();
                    state.result = state.candidate().map(|rgb| Sample {
                        rgb,
                        alt: GetAsyncKeyState(VK_MENU.0 as i32) < 0,
                    });
                    state.done = true;
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => {
                state.done = true;
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                if state.stop.load(Ordering::Relaxed)
                    || (state.alt && GetAsyncKeyState(VK_MENU.0 as i32) >= 0)
                {
                    state.done = true;
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_ACTIVATE if wparam.0 & 0xffff == WA_INACTIVE as usize => {
                state.done = true;
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                state.done = true;
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                state.done = true;
                LRESULT(0)
            }
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let _ = BitBlt(
                    dc,
                    0,
                    0,
                    state.width,
                    state.height,
                    Some(state.dc),
                    0,
                    0,
                    SRCCOPY,
                );
                let loupe = loupe(
                    [state.cursor.x, state.cursor.y],
                    [state.width, state.height],
                );
                let [x, y] = loupe.at;
                let side = Loupe::side();
                let _ = StretchBlt(
                    dc,
                    x,
                    y,
                    side,
                    side,
                    Some(state.dc),
                    loupe.patch[0],
                    loupe.patch[1],
                    LOUPE_PATCH,
                    LOUPE_PATCH,
                    SRCCOPY,
                );
                let black = GetStockObject(BLACK_BRUSH);
                let [label_x, label_y] = loupe.label;
                let panel = RECT {
                    left: label_x,
                    top: label_y,
                    right: label_x + LOUPE_LABEL[0],
                    bottom: label_y + LOUPE_LABEL[1],
                };
                FillRect(dc, &panel, HBRUSH(black.0));
                SetTextColor(dc, COLORREF(0xffffff));
                SetBkMode(dc, TRANSPARENT);
                let label = match state.candidate() {
                    Some([r, g, b]) => format!("RGB {r}, {g}, {b}"),
                    None => "Unavailable · Esc".into(),
                };
                let text: Vec<u16> = label.encode_utf16().collect();
                let _ = TextOutW(dc, label_x + 3, label_y, &text);
                let [cross_x, cross_y] = loupe.marked;
                let pixel = RECT {
                    left: cross_x,
                    top: cross_y,
                    right: cross_x + LOUPE_CELL,
                    bottom: cross_y + LOUPE_CELL,
                };
                FrameRect(dc, &pixel, HBRUSH(black.0));
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    pub fn sample_cursor() -> Option<[u8; 3]> {
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_err() {
                return None;
            }
            let dc = GetDC(None);
            if dc.is_invalid() {
                return None;
            }
            let color = GetPixel(dc, p.x, p.y).0;
            ReleaseDC(None, dc);
            (color != 0xffffffff).then_some([color as u8, (color >> 8) as u8, (color >> 16) as u8])
        }
    }

    pub fn pick(stop: Arc<AtomicBool>, alt: bool) -> PickResult {
        unsafe {
            // Every coordinate below is physical virtual-desktop space, including
            // monitors to the left/above the primary and mixed DPI configurations.
            let old_dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let result = pick_inner(stop, alt);
            if !old_dpi.0.is_null() {
                SetThreadDpiAwarenessContext(old_dpi);
            }
            result
        }
    }
    unsafe fn pick_inner(stop: Arc<AtomicBool>, alt: bool) -> PickResult {
        let origin = POINT {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
        };
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if width < 11 || height < 11 || width as i64 * height as i64 > 100_000_000 {
            return Err("Desktop is unavailable or exceeds the capture budget".into());
        }
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("Cannot access desktop pixels".into());
        }
        let dc = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if dc.is_invalid() || bitmap.is_invalid() {
            if !dc.is_invalid() {
                let _ = DeleteDC(dc);
            }
            if !bitmap.is_invalid() {
                let _ = DeleteObject(bitmap.into());
            }
            ReleaseDC(None, screen);
            return Err("Cannot allocate desktop sample".into());
        }
        let previous = SelectObject(dc, bitmap.into());
        let capture = BitBlt(
            dc,
            0,
            0,
            width,
            height,
            Some(screen),
            origin.x,
            origin.y,
            SRCCOPY | CAPTUREBLT,
        );
        ReleaseDC(None, screen);
        let mut state = Box::new(Session {
            dc,
            bitmap,
            previous,
            origin,
            width,
            height,
            cursor: POINT::default(),
            down: false,
            result: None,
            done: false,
            stop,
            alt,
        });
        capture.map_err(|e| e.to_string())?;
        state.position();
        let class = w!("AtlasDesktopColorSampler");
        let instance = HINSTANCE(GetModuleHandleW(None).map_err(|e| e.to_string())?.0);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            lpszClassName: class,
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_CROSS).map_err(|e| e.to_string())?,
            ..Default::default()
        };
        // The class can already exist from an earlier session in this process.
        RegisterClassW(&wc);
        let previous_window = GetForegroundWindow();
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class,
            w!("Pick a desktop color · Esc cancels"),
            WS_POPUP | WS_VISIBLE,
            origin.x,
            origin.y,
            width,
            height,
            None,
            None,
            Some(instance),
            Some((&mut *state as *mut Session).cast()),
        )
        .map_err(|e| e.to_string())?;
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if SetTimer(Some(hwnd), 1, 30, None) == 0 {
            let _ = DestroyWindow(hwnd);
            return Err("Cannot start desktop sampling input session".into());
        }
        let mut msg = MSG::default();
        // `done` is written from `window_proc` through the user-data pointer.
        #[allow(clippy::while_immutable_condition)]
        while !state.done {
            if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        if !previous_window.0.is_null() {
            let _ = SetForegroundWindow(previous_window);
        }
        Ok(state.result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A virtual desktop with a monitor left of and above the primary.
    const ORIGIN: [i32; 2] = [-1920, -300];
    const SIZE: [i32; 2] = [5360, 1740];

    #[test]
    fn the_sampled_pixel_is_the_hotspot_at_every_display_scale() {
        let window = [-1500, 120];
        for ppp in [1.0_f32, 1.25, 1.5, 2.0] {
            for points in [[0.0, 0.0], [37.2, 18.4], [611.0, 403.6], [1719.5, 611.0]] {
                // Windows reports a per-monitor-aware process's cursor in
                // physical pixels: the window origin plus points × scale.
                let hotspot = [
                    window[0] + (points[0] * ppp).round() as i32,
                    window[1] + (points[1] * ppp).round() as i32,
                ];
                let pixel = capture_pixel(hotspot, ORIGIN, SIZE);
                assert_eq!(
                    [pixel[0] + ORIGIN[0], pixel[1] + ORIGIN[1]],
                    hotspot,
                    "ppp {ppp}: the sample reads the pixel under the pointer"
                );
                let l = loupe(pixel, SIZE);
                for axis in 0..2 {
                    let marked = l.marked[axis];
                    assert!(
                        (marked..marked + LOUPE_CELL).contains(&pixel[axis]),
                        "ppp {ppp}: the framed cell {:?} sits under the hotspot {pixel:?}",
                        l.marked
                    );
                    assert_eq!(
                        l.patch[axis] + (marked - l.at[axis]) / LOUPE_CELL,
                        pixel[axis],
                        "ppp {ppp}: the framed cell magnifies the sampled pixel"
                    );
                }
            }
        }
    }

    #[test]
    fn the_framed_cell_stays_on_the_hotspot_at_the_desktop_edges() {
        for pixel in [
            [0, 0],
            [SIZE[0] - 1, SIZE[1] - 1],
            [2, SIZE[1] - 3],
            [SIZE[0] - 4, 1],
        ] {
            let l = loupe(pixel, SIZE);
            for axis in 0..2 {
                assert!((l.marked[axis]..l.marked[axis] + LOUPE_CELL).contains(&pixel[axis]));
                assert_eq!(
                    l.patch[axis] + (l.marked[axis] - l.at[axis]) / LOUPE_CELL,
                    pixel[axis]
                );
            }
        }
    }
}
