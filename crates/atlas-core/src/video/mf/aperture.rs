use crate::video::frame::FrameArea;
use windows::core::GUID;
use windows::Win32::Media::MediaFoundation::{
    IMFMediaType, IMFSourceReader, MFVideoArea, MF_E_ATTRIBUTENOTFOUND, MF_MT_FRAME_SIZE,
    MF_MT_GEOMETRIC_APERTURE, MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_PAN_SCAN_APERTURE,
    MF_MT_PAN_SCAN_ENABLED, MF_SOURCE_READER_CURRENT_TYPE_INDEX,
};

/// Microsoft GetVideoDisplayArea order: enabled pan/scan, minimum display,
/// geometric (legacy fallback), then the complete frame. Native dimensions can
/// constrain a coded output only when that current native rectangle fits it.
pub(super) fn display_area(
    media: &IMFMediaType,
    reader: &IMFSourceReader,
    stream: u32,
    coded: (u32, u32),
) -> Result<FrameArea, ()> {
    if unsafe { media.GetUINT32(&MF_MT_PAN_SCAN_ENABLED) }.unwrap_or(0) != 0 {
        if let Some(area) = attribute(media, &MF_MT_PAN_SCAN_APERTURE)? {
            return area.checked(coded.0, coded.1).ok_or(());
        }
    }
    for key in [MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_GEOMETRIC_APERTURE] {
        if let Some(area) = attribute(media, &key)? {
            return area.checked(coded.0, coded.1).ok_or(());
        }
    }
    let native =
        unsafe { reader.GetNativeMediaType(stream, MF_SOURCE_READER_CURRENT_TYPE_INDEX.0 as u32) }
            .ok();
    if let Some(size) = native.and_then(|media| unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) }.ok())
    {
        let area = FrameArea {
            x: 0,
            y: 0,
            w: (size >> 32) as u32,
            h: size as u32,
        };
        return area.checked(coded.0, coded.1).ok_or(());
    }
    Ok(FrameArea {
        x: 0,
        y: 0,
        w: coded.0,
        h: coded.1,
    })
}

pub(super) fn attribute(media: &IMFMediaType, key: &GUID) -> Result<Option<FrameArea>, ()> {
    let mut area = MFVideoArea::default();
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(
            &mut area as *mut MFVideoArea as *mut u8,
            std::mem::size_of::<MFVideoArea>(),
        )
    };
    let mut actual = 0;
    match unsafe { media.GetBlob(key, bytes, Some(&mut actual)) } {
        Err(error) if error.code() == MF_E_ATTRIBUTENOTFOUND => return Ok(None),
        Err(error) => {
            super::diagnostics::at("sample.aperture")(error);
            return Err(());
        }
        Ok(()) => {}
    }
    if actual as usize != bytes.len()
        || area.OffsetX.fract != 0
        || area.OffsetY.fract != 0
        || area.OffsetX.value < 0
        || area.OffsetY.value < 0
        || area.Area.cx <= 0
        || area.Area.cy <= 0
    {
        return Err(());
    }
    Ok(Some(FrameArea {
        x: area.OffsetX.value as u32,
        y: area.OffsetY.value as u32,
        w: area.Area.cx as u32,
        h: area.Area.cy as u32,
    }))
}
