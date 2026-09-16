use super::*;
use windows::{
    core::{BOOL, HRESULT},
    Win32::{
        Foundation::{DV_E_FORMATETC, E_NOTIMPL, S_OK},
        Graphics::Gdi::ClientToScreen,
        System::{
            Com::{IAdviseSink, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, STGMEDIUM_0},
            Memory::{GlobalAlloc, GMEM_MOVEABLE},
        },
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE},
    },
};

#[implement(IDataObject)]
struct Data(Vec<(u16, Vec<u8>)>);
#[allow(non_snake_case)]
impl IDataObject_Impl for Data_Impl {
    fn GetData(&self, format: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let format = unsafe { &*format };
        let bytes = self
            .0
            .iter()
            .find(|(id, _)| *id == format.cfFormat)
            .map(|(_, bytes)| bytes)
            .ok_or_else(|| windows::core::Error::from_hresult(DV_E_FORMATETC))?;
        unsafe {
            let global = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
            let ptr = GlobalLock(global);
            assert!(!ptr.is_null());
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast(), bytes.len());
            let _ = GlobalUnlock(global);
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: global },
                pUnkForRelease: std::mem::ManuallyDrop::new(None),
            })
        }
    }
    fn QueryGetData(&self, f: *const FORMATETC) -> HRESULT {
        if self.0.iter().any(|(id, _)| *id == unsafe { (*f).cfFormat }) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }
    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
        E_NOTIMPL
    }
    fn SetData(
        &self,
        _: *const FORMATETC,
        _: *const STGMEDIUM,
        _: BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn EnumFormatEtc(&self, _: u32) -> windows::core::Result<IEnumFORMATETC> {
        Err(E_NOTIMPL.into())
    }
    fn DAdvise(
        &self,
        _: *const FORMATETC,
        _: u32,
        _: Ref<'_, IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(E_NOTIMPL.into())
    }
    fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(E_NOTIMPL.into())
    }
}
fn wide(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}
fn data(format: u16, bytes: Vec<u8>) -> IDataObject {
    Data(vec![(format, bytes)]).into()
}

#[test]
fn web_url_drop_ole_decodes_browser_url_and_selected_unicode_text() {
    let url = "https://example.com/path?q=1#anchor";
    for format in [CF_UNICODETEXT.0, unsafe {
        RegisterClipboardFormatW(w!("UniformResourceLocatorW")) as u16
    }] {
        assert!(
            matches!(read_payload(&data(format, wide(url))), Some(Payload::Url(value)) if value == url)
        );
    }
    let uri = unsafe { RegisterClipboardFormatW(w!("text/uri-list")) as u16 };
    assert!(
        matches!(read_payload(&data(uri, format!("# comment\r\n{url}\r\n\0").into_bytes())), Some(Payload::Url(value)) if value == url)
    );
    assert!(read_payload(&data(uri, b"https://a.test\nhttps://b.test\0".to_vec())).is_none());
    assert!(read_payload(&data(CF_UNICODETEXT.0, wide("ordinary text"))).is_none());
    assert!(read_payload(&data(CF_UNICODETEXT.0, vec![0x41; 131_076])).is_none());
}

#[test]
fn web_url_drop_ole_reads_file_names_without_opening_files() {
    let path = "C:\\nonexistent\\cloud-placeholder.html";
    let mut bytes = vec![0; 20]; // DROPFILES header, followed by double-NUL paths
    bytes[0..4].copy_from_slice(&20u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes()); // fWide
    bytes.extend(wide(path));
    bytes.extend([0, 0]);
    assert!(
        matches!(read_payload(&data(CF_HDROP.0, bytes)), Some(Payload::Files(paths)) if paths == [PathBuf::from(path)])
    );
}

#[test]
fn web_url_drop_ole_target_negotiates_copy_and_maps_client_coordinates() {
    unsafe {
        OleInitialize(None).unwrap();
    }
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("Slate drop test"),
            WINDOW_STYLE::default(),
            100,
            100,
            400,
            400,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    };
    let ctx = eframe::egui::Context::default();
    ctx.set_pixels_per_point(2.0);
    let _ = ctx.run(eframe::egui::RawInput::default(), |_| {});
    let inbox = Inbox::default();
    inbox.set_url_area(Some(Rect::from_min_max(
        Pos2::ZERO,
        Pos2::new(100.0, 100.0),
    )));
    let target: IDropTarget = Target {
        hwnd,
        ctx,
        inbox: inbox.clone(),
        hover: RefCell::new(None),
    }
    .into();
    unsafe {
        RegisterDragDrop(hwnd, &target).unwrap();
    }
    let registration = Registration {
        hwnd,
        _target: target.clone(),
    };
    let data = data(CF_UNICODETEXT.0, wide("https://example.com"));
    let mut point = POINT { x: 80, y: 60 };
    assert!(unsafe { ClientToScreen(hwnd, &mut point) }.as_bool());
    let pt = POINTL {
        x: point.x,
        y: point.y,
    };
    let mut effect = DROPEFFECT_COPY;
    unsafe {
        target
            .DragEnter(&data, MODIFIERKEYS_FLAGS(0), pt, &mut effect)
            .unwrap();
        assert_eq!(effect, DROPEFFECT_COPY);
        target.DragLeave().unwrap();
        assert!(inbox.pop().is_none());
        effect = windows::Win32::System::Ole::DROPEFFECT_MOVE;
        target
            .DragEnter(&data, MODIFIERKEYS_FLAGS(0), pt, &mut effect)
            .unwrap();
        assert_eq!(effect, DROPEFFECT_NONE);
        effect = DROPEFFECT_COPY;
        target
            .Drop(&data, MODIFIERKEYS_FLAGS(0), pt, &mut effect)
            .unwrap();
    }
    assert_eq!(effect, DROPEFFECT_COPY);
    let event = inbox.pop().unwrap();
    assert_eq!(event.at, Pos2::new(40.0, 30.0));
    assert!(matches!(event.payload, Payload::Url(_)));
    drop(registration);
    unsafe {
        DestroyWindow(hwnd).unwrap();
    }
}
