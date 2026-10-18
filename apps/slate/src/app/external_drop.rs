//! Incoming OS data only. Placement and journal commits live in the app;
//! file drops continue through Slate's existing file-drop path.
use super::external_drop_classify::ImageSource;
use eframe::egui::{Pos2, Rect};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[cfg_attr(not(windows), allow(dead_code))] // OS drop construction is Windows-only.
pub(super) enum Payload {
    Url(String),
    Files(Vec<PathBuf>),
    Image(ImageSource),
}
pub(super) struct DropEvent {
    pub payload: Payload,
    pub at: Pos2,
    pub alt: bool,
}
#[derive(Default)]
struct State {
    pending: VecDeque<DropEvent>,
    url_area: Option<Rect>,
    /// Files an OS drag is holding over the window, and where.
    hover: Option<(Vec<PathBuf>, Pos2)>,
}
#[derive(Clone, Default)]
pub(super) struct Inbox(Arc<Mutex<State>>);
impl Inbox {
    pub fn pop(&self) -> Option<DropEvent> {
        self.0.lock().unwrap().pending.pop_front()
    }
    pub fn set_url_area(&self, area: Option<Rect>) {
        self.0.lock().unwrap().url_area = area;
    }
    /// The one file an OS drag is holding over the window, and where.
    pub fn hover_file(&self) -> Option<(PathBuf, Pos2)> {
        match &self.0.lock().unwrap().hover {
            Some((paths, at)) if paths.len() == 1 => Some((paths[0].clone(), *at)),
            _ => None,
        }
    }
    #[cfg(test)]
    pub fn push_test(&self, event: DropEvent) {
        self.0.lock().unwrap().pending.push_back(event);
    }
    #[cfg(test)]
    pub fn set_hover_test(&self, hover: Option<(Vec<PathBuf>, Pos2)>) {
        self.0.lock().unwrap().hover = hover;
    }
}

#[cfg(windows)]
pub(super) mod win {
    #[cfg(test)]
    mod tests {
        include!("external_drop_tests.rs");
    }
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{cell::RefCell, ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::{
        core::{implement, w, Ref},
        Win32::{
            Foundation::{HWND, POINT, POINTL, S_FALSE, S_OK},
            Graphics::Gdi::ScreenToClient,
            System::{
                Com::{
                    IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL,
                    TYMED_ISTREAM,
                },
                DataExchange::RegisterClipboardFormatW,
                Memory::{GlobalLock, GlobalSize, GlobalUnlock},
                Ole::{
                    IDropTarget, IDropTarget_Impl, OleInitialize, OleUninitialize,
                    RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_DIB, CF_DIBV5, CF_HDROP,
                    CF_TEXT, CF_UNICODETEXT, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
                    DROPEFFECT_NONE,
                },
                SystemServices::MODIFIERKEYS_FLAGS,
            },
            UI::Shell::{DragQueryFileW, HDROP},
        },
    };

    pub struct Registration {
        hwnd: HWND,
        _target: IDropTarget,
    }
    impl Registration {
        pub fn install(cc: &eframe::CreationContext<'_>, inbox: Inbox) -> Result<Self, String> {
            let handle = cc.window_handle().map_err(|e| e.to_string())?;
            let RawWindowHandle::Win32(raw) = handle.as_raw() else {
                return Err("No Windows handle".into());
            };
            let hwnd = HWND(raw.hwnd.get() as *mut _);
            unsafe {
                OleInitialize(None).map_err(|e| e.to_string())?;
            }
            let target: IDropTarget = Target {
                hwnd,
                inbox,
                ctx: cc.egui_ctx.clone(),
                hover: RefCell::new(None),
            }
            .into();
            if let Err(error) = unsafe { RegisterDragDrop(hwnd, &target) } {
                unsafe {
                    OleUninitialize();
                }
                return Err(error.to_string());
            }
            Ok(Self {
                hwnd,
                _target: target,
            })
        }
    }
    impl Drop for Registration {
        fn drop(&mut self) {
            unsafe {
                let _ = RevokeDragDrop(self.hwnd);
                OleUninitialize();
            }
        }
    }

    #[implement(IDropTarget)]
    struct Target {
        hwnd: HWND,
        inbox: Inbox,
        ctx: eframe::egui::Context,
        hover: RefCell<Option<Payload>>,
    }
    impl Target {
        fn position(&self, pt: &POINTL) -> Option<Pos2> {
            let mut point = POINT { x: pt.x, y: pt.y };
            if !unsafe { ScreenToClient(self.hwnd, &mut point) }.as_bool() {
                return None;
            }
            Some(Pos2::new(point.x as f32, point.y as f32) / self.ctx.pixels_per_point())
        }
        fn accepts(&self, payload: &Payload, at: Pos2) -> bool {
            let state = self.inbox.0.lock().unwrap();
            state.pending.len() < 16
                && match payload {
                    Payload::Files(_) => true,
                    Payload::Url(_) | Payload::Image(_) => {
                        state.url_area.is_some_and(|r| r.contains(at))
                    }
                }
        }
        fn effect(&self, pt: &POINTL, offered: DROPEFFECT) -> DROPEFFECT {
            if self.position(pt).is_some_and(|at| {
                self.hover
                    .borrow()
                    .as_ref()
                    .is_some_and(|p| self.accepts(p, at))
            }) {
                if offered.0 & DROPEFFECT_COPY.0 != 0 {
                    return DROPEFFECT_COPY;
                }
                if offered.0 & DROPEFFECT_LINK.0 != 0 {
                    return DROPEFFECT_LINK;
                }
            }
            DROPEFFECT_NONE
        }
        /// Tell the frame loop which files are held where (`None` = gone).
        fn publish_hover(&self, pt: Option<&POINTL>) {
            let hover = pt.and_then(|pt| {
                let at = self.position(pt)?;
                match self.hover.borrow().as_ref()? {
                    Payload::Files(paths) => Some((paths.clone(), at)),
                    Payload::Url(_) | Payload::Image(_) => None,
                }
            });
            self.inbox.0.lock().unwrap().hover = hover;
            self.ctx.request_repaint();
        }
    }
    #[allow(non_snake_case)]
    impl IDropTarget_Impl for Target_Impl {
        fn DragEnter(
            &self,
            data: Ref<'_, IDataObject>,
            _: MODIFIERKEYS_FLAGS,
            pt: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            *self.hover.borrow_mut() = data.as_ref().and_then(read_hover_payload);
            self.publish_hover(Some(pt));
            unsafe {
                *effect = self.effect(pt, *effect);
            }
            Ok(())
        }
        fn DragOver(
            &self,
            _: MODIFIERKEYS_FLAGS,
            pt: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            self.publish_hover(Some(pt));
            unsafe {
                *effect = self.effect(pt, *effect);
            }
            Ok(())
        }
        fn DragLeave(&self) -> windows::core::Result<()> {
            self.hover.borrow_mut().take();
            self.publish_hover(None);
            Ok(())
        }
        fn Drop(
            &self,
            data: Ref<'_, IDataObject>,
            keys: MODIFIERKEYS_FLAGS,
            pt: &POINTL,
            effect: *mut DROPEFFECT,
        ) -> windows::core::Result<()> {
            // Re-read the final data: a source may render it lazily or change it.
            *self.hover.borrow_mut() = data.as_ref().and_then(read_payload);
            let accepted = unsafe { self.effect(pt, *effect) };
            unsafe {
                *effect = accepted;
            }
            let payload = self.hover.borrow_mut().take();
            self.publish_hover(None);
            if accepted != DROPEFFECT_NONE {
                if let (Some(payload), Some(at)) = (payload, self.position(pt)) {
                    // OLE's MK_ALT bit; egui does not receive keys during the drag.
                    self.inbox.0.lock().unwrap().pending.push_back(DropEvent {
                        payload,
                        at,
                        alt: keys.0 & 0x20 != 0,
                    });
                    self.ctx.request_repaint();
                }
            }
            Ok(())
        }
    }

    // IDataObject owns the transfer medium. Release it exactly once, including
    // malformed/unsupported payloads; never DragFinish borrowed OLE storage.
    struct Medium(STGMEDIUM);
    impl Drop for Medium {
        fn drop(&mut self) {
            unsafe {
                ReleaseStgMedium(&mut self.0);
            }
        }
    }
    fn medium(data: &IDataObject, format: u16, lindex: i32, tymed: u32) -> Option<Medium> {
        if format == 0 {
            return None;
        }
        let format = transfer_format(format, lindex, tymed);
        let result = Medium(unsafe { data.GetData(&format).ok()? });
        (result.0.tymed.is_power_of_two() && result.0.tymed & tymed != 0).then_some(result)
    }
    fn transfer_format(format: u16, lindex: i32, tymed: u32) -> FORMATETC {
        FORMATETC {
            cfFormat: format,
            dwAspect: DVASPECT_CONTENT.0,
            lindex,
            tymed,
            ..Default::default()
        }
    }
    fn read_hover_payload(data: &IDataObject) -> Option<Payload> {
        if let Some(medium) = medium(data, CF_HDROP.0, -1, TYMED_HGLOBAL.0 as u32) {
            if let Some(files) = read_files(medium) {
                return Some(files);
            }
        }
        // Query lazy image transfers during hover; pixels are requested only
        // after the user releases. The placeholder is solely for canvas gating.
        let has_image = image_formats().into_iter().any(|(format, lindex)| {
            format != 0
                && unsafe {
                    data.QueryGetData(&transfer_format(
                        format,
                        lindex,
                        (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32,
                    ))
                } == S_OK
        });
        if has_image {
            Some(Payload::Image(ImageSource::Encoded(Vec::new())))
        } else {
            read_links(data, None)
        }
    }
    fn read_payload(data: &IDataObject) -> Option<Payload> {
        if let Some(medium) = medium(data, CF_HDROP.0, -1, TYMED_HGLOBAL.0 as u32) {
            if let Some(files) = read_files(medium) {
                return Some(files);
            }
        }
        read_links(data, read_image(data))
    }
    fn read_files(medium: Medium) -> Option<Payload> {
        let drop = HDROP(unsafe { medium.0.u.hGlobal }.0);
        let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
        if count == 0 || count > atlas_core::shell_drag::MAX_DRAG_PATHS as u32 {
            return None;
        }
        let mut paths = Vec::with_capacity(count as usize);
        for i in 0..count {
            let len = unsafe { DragQueryFileW(drop, i, None) } as usize;
            if len == 0 || len > 32_767 {
                return None;
            }
            let mut wide = vec![0; len + 1];
            if unsafe { DragQueryFileW(drop, i, Some(&mut wide)) } as usize != len {
                return None;
            }
            paths.push(PathBuf::from(OsString::from_wide(&wide[..len])));
        }
        Some(Payload::Files(paths))
    }
    fn read_links(data: &IDataObject, image: Option<ImageSource>) -> Option<Payload> {
        let formats = unsafe {
            [
                (
                    RegisterClipboardFormatW(w!("UniformResourceLocatorW")) as u16,
                    true,
                    0,
                ),
                (
                    RegisterClipboardFormatW(w!("UniformResourceLocator")) as u16,
                    false,
                    0,
                ),
                (
                    RegisterClipboardFormatW(w!("text/uri-list")) as u16,
                    false,
                    1,
                ),
                (
                    RegisterClipboardFormatW(w!("text/x-moz-url")) as u16,
                    true,
                    2,
                ),
                (CF_UNICODETEXT.0, true, 0),
                (CF_TEXT.0, false, 0),
            ]
        };
        let mut urls = Vec::new();
        for (format, wide, kind) in formats {
            let Some(text) = read_text(data, format, wide) else {
                continue;
            };
            let text = match kind {
                1 => text
                    .lines()
                    .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
                2 => text.lines().next().unwrap_or_default().to_owned(),
                _ => text,
            };
            if !urls.contains(&text) {
                urls.push(text);
            }
        }
        let html = [w!("HTML Format"), w!("text/html")]
            .into_iter()
            .find_map(|name| {
                read_text(
                    data,
                    unsafe { RegisterClipboardFormatW(name) } as u16,
                    false,
                )
            });
        super::super::external_drop_classify::classify(html.as_deref(), &urls, image)
    }
    fn image_formats() -> [(u16, i32); 5] {
        unsafe {
            [
                (RegisterClipboardFormatW(w!("PNG")) as u16, -1),
                (RegisterClipboardFormatW(w!("image/png")) as u16, -1),
                (CF_DIBV5.0, -1),
                (CF_DIB.0, -1),
                (RegisterClipboardFormatW(w!("FileContents")) as u16, 0),
            ]
        }
    }
    fn read_image(data: &IDataObject) -> Option<ImageSource> {
        let max = atlas_core::clipboard_image::MAX_PASTE_PNG_BYTES;
        for (format, lindex) in image_formats() {
            let Some(bytes) = read_raw(data, format, lindex, max) else {
                continue;
            };
            if matches!(format, id if id == CF_DIBV5.0 || id == CF_DIB.0) {
                return Some(ImageSource::Bitmap(bytes));
            }
            if matches!(image::guess_format(&bytes), Ok(kind) if lindex == 0 || kind == image::ImageFormat::Png)
            {
                return Some(ImageSource::Encoded(bytes));
            }
        }
        None
    }
    fn read_text(data: &IDataObject, format: u16, wide: bool) -> Option<String> {
        let bytes = read_raw(data, format, -1, 131_074)?;
        if wide {
            if !bytes.len().is_multiple_of(2) {
                return None;
            }
            let units: Vec<_> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            units
                .iter()
                .position(|&c| c == 0)
                .and_then(|end| String::from_utf16(&units[..end]).ok())
        } else {
            let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
            String::from_utf8(bytes[..end].to_vec()).ok()
        }
    }
    fn read_raw(data: &IDataObject, format: u16, lindex: i32, max: usize) -> Option<Vec<u8>> {
        let medium = medium(
            data,
            format,
            lindex,
            (TYMED_HGLOBAL.0 | TYMED_ISTREAM.0) as u32,
        )?;
        if medium.0.tymed == TYMED_HGLOBAL.0 as u32 {
            let global = unsafe { medium.0.u.hGlobal };
            let size = unsafe { GlobalSize(global) };
            if size == 0 || size > max {
                return None;
            }
            let ptr = unsafe { GlobalLock(global) };
            if ptr.is_null() {
                return None;
            }
            let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) }.to_vec();
            unsafe {
                let _ = GlobalUnlock(global);
            }
            return Some(bytes);
        }
        if medium.0.tymed != TYMED_ISTREAM.0 as u32 {
            return None;
        }
        let stream = unsafe { medium.0.u.pstm.as_ref()? };
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 64 * 1024];
        loop {
            let take = chunk.len().min(max + 1 - bytes.len());
            let mut read = 0;
            let result =
                unsafe { stream.Read(chunk.as_mut_ptr().cast(), take as u32, Some(&mut read)) };
            if result.is_err() || read as usize > take || bytes.len() + read as usize > max {
                return None;
            }
            bytes.extend_from_slice(&chunk[..read as usize]);
            if read == 0 || result == S_FALSE {
                return (!bytes.is_empty()).then_some(bytes);
            }
        }
    }
}
