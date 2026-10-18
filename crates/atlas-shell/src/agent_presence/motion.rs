//! Native accessibility input, cached independently of the avatar animation.
pub fn system_reduced_motion(ctx: &eframe::egui::Context) -> bool {
    let now = ctx.input(|i| i.time);
    let key = eframe::egui::Id::new("atlas_system_reduced_motion");
    ctx.data_mut(|data| {
        if let Some((at, value)) = data.get_temp::<(f64, bool)>(key) {
            if now >= at && now - at < 2.0 {
                return value;
            }
        }
        let value = read_system_preference();
        data.insert_temp(key, (now, value));
        value
    })
}

#[cfg(windows)]
fn read_system_preference() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut enabled = 1i32;
    // SPI writes one Win32 BOOL to our valid stack allocation.
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut i32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
            && enabled == 0
    }
}

#[cfg(not(windows))]
fn read_system_preference() -> bool {
    false
}
