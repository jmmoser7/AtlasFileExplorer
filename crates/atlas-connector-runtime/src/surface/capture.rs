//! Bounded GDI readback. Call on a worker: PrintWindow can wait for a foreign UI.
//! Native renderers can refuse this path; never treat an absent image as success.
#[cfg(windows)]
pub fn capture_window_rgba(handle: isize, expected_pid: u32) -> Option<(u32, u32, Vec<u8>)> {
    let hwnd = windows::Win32::Foundation::HWND(handle as *mut _);
    let mut actual_pid = 0;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            hwnd,
            Some(&mut actual_pid),
        );
    }
    if actual_pid == 0 || actual_pid != expected_pid {
        return None;
    }
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, PW_RENDERFULLCONTENT};
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).ok()?;
        let w = rect.right.max(0) as u32;
        let h = rect.bottom.max(0) as u32;
        if w < 8 || h < 8 || w > 4096 || h > 4096 {
            return None;
        }
        let window_dc = GetDC(Some(hwnd));
        if window_dc.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(window_dc));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: h as i32,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bitmap =
            CreateDIBSection(Some(window_dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok();
        let Some(bitmap) = bitmap else {
            let _ = DeleteDC(mem);
            ReleaseDC(Some(hwnd), window_dc);
            return None;
        };
        let old = SelectObject(mem, HGDIOBJ(bitmap.0));
        let printed = print_window(hwnd, mem, PW_RENDERFULLCONTENT).as_bool();
        let mut rgba = None;
        if printed && !bits.is_null() {
            let count = (w as usize) * (h as usize);
            let src = std::slice::from_raw_parts(bits as *const u8, count * 4);
            let mut out = vec![0u8; count * 4];
            let row = (w as usize) * 4;
            for y in 0..h as usize {
                let src_row = (h as usize - 1 - y) * row;
                let dst_row = y * row;
                for x in 0..w as usize {
                    let s = src_row + x * 4;
                    let d = dst_row + x * 4;
                    out[d] = src[s + 2];
                    out[d + 1] = src[s + 1];
                    out[d + 2] = src[s];
                    out[d + 3] = 255;
                }
            }
            rgba = Some((w, h, out));
        }
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem);
        ReleaseDC(Some(hwnd), window_dc);
        rgba
    }
}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn PrintWindow(
        hwnd: windows::Win32::Foundation::HWND,
        hdc: windows::Win32::Graphics::Gdi::HDC,
        nflags: u32,
    ) -> windows::core::BOOL;
}

#[cfg(windows)]
fn print_window(
    hwnd: windows::Win32::Foundation::HWND,
    hdc: windows::Win32::Graphics::Gdi::HDC,
    flags: u32,
) -> windows::core::BOOL {
    unsafe { PrintWindow(hwnd, hdc, flags) }
}
