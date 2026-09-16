//! Incoming OS data only. Portal rules and journal commits live in board_web;
//! file drops continue through Slate's existing file-drop path.
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
    #[cfg(test)]
    pub fn push_test(&self, event: DropEvent) {
        self.0.lock().unwrap().pending.push_back(event);
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
            Foundation::{HWND, POINT, POINTL},
            Graphics::Gdi::ScreenToClient,
            System::{
                Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL},
                DataExchange::RegisterClipboardFormatW,
                Memory::{GlobalLock, GlobalSize, GlobalUnlock},
                Ole::{
                    IDropTarget, IDropTarget_Impl, OleInitialize, OleUninitialize,
                    RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, CF_HDROP, CF_UNICODETEXT,
                    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_NONE,
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
                    Payload::Url(_) => state.url_area.is_some_and(|r| r.contains(at)),
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
            *self.hover.borrow_mut() = data.as_ref().and_then(read_payload);
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
            unsafe {
                *effect = self.effect(pt, *effect);
            }
            Ok(())
        }
        fn DragLeave(&self) -> windows::core::Result<()> {
            self.hover.borrow_mut().take();
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
    fn medium(data: &IDataObject, format: u16) -> Option<Medium> {
        let format = FORMATETC {
            cfFormat: format,
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
            ..Default::default()
        };
        let result = Medium(unsafe { data.GetData(&format).ok()? });
        (result.0.tymed == TYMED_HGLOBAL.0 as u32).then_some(result)
    }
    fn read_payload(data: &IDataObject) -> Option<Payload> {
        if let Some(medium) = medium(data, CF_HDROP.0) {
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
            return Some(Payload::Files(paths));
        }
        let formats = unsafe {
            [
                (
                    RegisterClipboardFormatW(w!("UniformResourceLocatorW")) as u16,
                    true,
                    false,
                ),
                (
                    RegisterClipboardFormatW(w!("text/uri-list")) as u16,
                    false,
                    true,
                ),
                (CF_UNICODETEXT.0, true, false),
            ]
        };
        for (format, wide, uri_list) in formats {
            let Some(text) = read_text(data, format, wide) else {
                continue;
            };
            let text = if uri_list {
                text.lines()
                    .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                text
            };
            if let Some(url) = super::super::board_web::web_url_text(&text) {
                return Some(Payload::Url(url.to_owned()));
            }
        }
        None
    }
    fn read_text(data: &IDataObject, format: u16, wide: bool) -> Option<String> {
        if format == 0 {
            return None;
        }
        let medium = medium(data, format)?;
        let global = unsafe { medium.0.u.hGlobal };
        let size = unsafe { GlobalSize(global) };
        if size == 0 || size > 131_074 || (wide && size % 2 != 0) {
            return None;
        }
        let ptr = unsafe { GlobalLock(global) };
        if ptr.is_null() {
            return None;
        }
        let text = if wide {
            let units = unsafe { std::slice::from_raw_parts(ptr.cast::<u16>(), size / 2) };
            units
                .iter()
                .position(|&c| c == 0)
                .and_then(|end| String::from_utf16(&units[..end]).ok())
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) };
            let end = bytes.iter().position(|&c| c == 0).unwrap_or(size);
            String::from_utf8(bytes[..end].to_vec()).ok()
        };
        unsafe {
            let _ = GlobalUnlock(global);
        }
        text
    }
}
