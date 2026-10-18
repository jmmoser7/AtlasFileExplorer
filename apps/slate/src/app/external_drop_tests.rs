use super::*;
use windows::{
    Win32::{
        Foundation::{DV_E_FORMATETC, E_NOTIMPL, S_OK},
        Graphics::Gdi::ClientToScreen,
        System::{
            Com::{
                IAdviseSink, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, STGMEDIUM_0,
                StructuredStorage::CreateStreamOnHGlobal,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc},
        },
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WINDOW_STYLE},
    },
    core::{BOOL, HRESULT},
};

#[implement(IDataObject)]
struct Data {
    formats: Vec<(u16, Vec<u8>)>,
    stream: bool,
    probe_only: bool,
}
#[allow(non_snake_case)]
impl IDataObject_Impl for Data_Impl {
    fn GetData(&self, format: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let format = unsafe { &*format };
        assert!(
            !self.probe_only || format.cfFormat == CF_HDROP.0,
            "hover requested lazy image bytes"
        );
        let contents = unsafe { RegisterClipboardFormatW(w!("FileContents")) } as u16;
        let tymed = if self.stream {
            TYMED_ISTREAM
        } else {
            TYMED_HGLOBAL
        };
        if format.lindex != if format.cfFormat == contents { 0 } else { -1 }
            || format.tymed & tymed.0 as u32 == 0
        {
            return Err(DV_E_FORMATETC.into());
        }
        let bytes = self
            .formats
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
            let medium = if self.stream {
                STGMEDIUM_0 {
                    pstm: std::mem::ManuallyDrop::new(Some(CreateStreamOnHGlobal(global, true)?)),
                }
            } else {
                STGMEDIUM_0 { hGlobal: global }
            };
            Ok(STGMEDIUM {
                tymed: tymed.0 as u32,
                u: medium,
                pUnkForRelease: std::mem::ManuallyDrop::new(None),
            })
        }
    }
    fn QueryGetData(&self, f: *const FORMATETC) -> HRESULT {
        let format = unsafe { &*f };
        let contents = unsafe { RegisterClipboardFormatW(w!("FileContents")) } as u16;
        let expected = if format.cfFormat == contents { 0 } else { -1 };
        if self.formats.iter().any(|(id, _)| *id == format.cfFormat) && format.lindex == expected {
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
    Data {
        formats: vec![(format, bytes)],
        stream: false,
        probe_only: false,
    }
    .into()
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
        assert!(
            matches!(read_hover_payload(&data(format, wide(url))), Some(Payload::Url(value)) if value == url)
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
    let object: IDataObject = Data {
        formats: vec![
            (CF_HDROP.0, bytes),
            (CF_UNICODETEXT.0, wide("https://example.com/page")),
            (
                unsafe { RegisterClipboardFormatW(w!("PNG")) } as u16,
                vec![1; 32],
            ),
        ],
        stream: false,
        probe_only: false,
    }
    .into();
    assert!(
        matches!(read_payload(&object), Some(Payload::Files(paths)) if paths == [PathBuf::from(path)])
    );
    assert!(
        matches!(read_hover_payload(&object), Some(Payload::Files(paths)) if paths == [PathBuf::from(path)])
    );
}

#[test]
fn image_drop_ole_prefers_picture_bytes_to_a_browser_page_url() {
    let png = atlas_core::clipboard_image::encode_png(1, 1, &[10, 20, 30, 255]).unwrap();
    for name in [w!("PNG"), w!("image/png")] {
        let object: IDataObject = Data {
            formats: vec![
                (CF_UNICODETEXT.0, wide("https://example.com/page")),
                (
                    unsafe { RegisterClipboardFormatW(name) } as u16,
                    png.clone(),
                ),
            ],
            stream: false,
            probe_only: false,
        }
        .into();
        assert!(
            matches!(read_payload(&object), Some(Payload::Image(ImageSource::Encoded(bytes))) if bytes.starts_with(&png))
        );
    }
    for format in [CF_DIBV5.0, CF_DIB.0] {
        let object: IDataObject = Data {
            formats: vec![
                (
                    unsafe { RegisterClipboardFormatW(w!("PNG")) } as u16,
                    vec![1; 32],
                ),
                (format, vec![1; 40]),
            ],
            stream: false,
            probe_only: false,
        }
        .into();
        assert!(
            matches!(read_payload(&object), Some(Payload::Image(ImageSource::Bitmap(bytes))) if bytes.starts_with(&[1; 40]))
        );
    }
}

#[test]
fn an_empty_browser_file_offer_does_not_hide_its_image_transfer() {
    let mut empty_files = vec![0; 24];
    empty_files[0..4].copy_from_slice(&20u32.to_le_bytes());
    empty_files[16..20].copy_from_slice(&1u32.to_le_bytes());
    let png = atlas_core::clipboard_image::encode_png(1, 1, &[10, 20, 30, 255]).unwrap();
    let object: IDataObject = Data {
        formats: vec![
            (CF_HDROP.0, empty_files),
            (
                unsafe { RegisterClipboardFormatW(w!("PNG")) } as u16,
                png.clone(),
            ),
        ],
        stream: false,
        probe_only: false,
    }
    .into();
    assert!(matches!(
        read_hover_payload(&object),
        Some(Payload::Image(_))
    ));
    assert!(
        matches!(read_payload(&object), Some(Payload::Image(ImageSource::Encoded(bytes)))
        if bytes.starts_with(&png))
    );
}

#[test]
fn image_drop_ole_reads_promised_file_contents_and_bounds_both_media() {
    let contents = unsafe { RegisterClipboardFormatW(w!("FileContents")) } as u16;
    let png = atlas_core::clipboard_image::encode_png(1, 1, &[10, 20, 30, 255]).unwrap();
    for stream in [false, true] {
        let object: IDataObject = Data {
            formats: vec![(contents, png.clone())],
            stream,
            probe_only: false,
        }
        .into();
        assert!(
            matches!(read_payload(&object), Some(Payload::Image(ImageSource::Encoded(bytes))) if bytes.starts_with(&png))
        );
        assert!(read_raw(&object, contents, 0, 3).is_none());
        assert!(read_raw(&object, contents, -1, 64).is_none());
    }
}

#[test]
fn image_drop_ole_keeps_virtual_shortcut_contents_as_a_link() {
    let contents = unsafe { RegisterClipboardFormatW(w!("FileContents")) } as u16;
    for stream in [false, true] {
        let object: IDataObject = Data {
            formats: vec![
                (
                    contents,
                    b"[InternetShortcut]\r\nURL=https://example.com/page\r\n".to_vec(),
                ),
                (CF_UNICODETEXT.0, wide("https://example.com/page")),
            ],
            stream,
            probe_only: false,
        }
        .into();
        assert!(
            matches!(read_payload(&object), Some(Payload::Url(url)) if url == "https://example.com/page")
        );
    }
}

#[test]
fn image_drop_ole_classifies_html_and_mozilla_image_urls() {
    let html = unsafe { RegisterClipboardFormatW(w!("HTML Format")) } as u16;
    let object: IDataObject = Data {
        formats: vec![
            (html, b"<!--StartFragment--><img src=\"https://example.com/picture.png\"><!--EndFragment-->\0".to_vec()),
            (CF_UNICODETEXT.0, wide("https://example.com/page")),
        ],
        stream: false,
        probe_only: false,
    }.into();
    assert!(
        matches!(read_payload(&object), Some(Payload::Image(ImageSource::Url(url))) if url == "https://example.com/picture.png")
    );
    let moz = unsafe { RegisterClipboardFormatW(w!("text/x-moz-url")) } as u16;
    assert!(matches!(
        read_payload(&data(moz, wide("https://example.com/picture.png\nPicture"))),
        Some(Payload::Image(ImageSource::Url(_)))
    ));
}

#[test]
fn image_drop_ole_hover_never_requests_pixels_or_promised_contents() {
    for (format, _) in image_formats() {
        for stream in [false, true] {
            let object: IDataObject = Data {
                formats: vec![(format, vec![1; 32])],
                stream,
                probe_only: true,
            }
            .into();
            assert!(matches!(read_hover_payload(&object),
                Some(Payload::Image(ImageSource::Encoded(bytes))) if bytes.is_empty()));
        }
    }
}

#[test]
fn image_drop_ole_accepts_pictures_only_in_the_canvas() {
    let target = Target {
        hwnd: HWND::default(),
        ctx: eframe::egui::Context::default(),
        inbox: Inbox::default(),
        hover: RefCell::new(None),
    };
    let payload = Payload::Image(ImageSource::Encoded(vec![1]));
    assert!(!target.accepts(&payload, Pos2::new(20.0, 20.0)));
    target.inbox.set_url_area(Some(Rect::from_min_max(
        Pos2::ZERO,
        Pos2::new(100.0, 100.0),
    )));
    assert!(target.accepts(&payload, Pos2::new(20.0, 20.0)));
    assert!(!target.accepts(&payload, Pos2::new(120.0, 20.0)));
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
