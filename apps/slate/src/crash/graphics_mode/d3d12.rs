//! Probe the protected Intel rendering path outside the user's workbook process.
use super::{packaged_provider, PROVIDER};
use eframe::glow::{self, HasContext};
use std::cell::Cell;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};
use windows::Win32::System::LibraryLoader::{
    LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_core::PCWSTR;

const PROBE: &str = "--probe-protected-d3d12";

pub(super) fn available(provider: &Path) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    // An incompatible runtime can terminate or hang during initialization or
    // teardown. Only a clean probe exit allows the main process to use it.
    let Ok(mut child) = Command::new(exe)
        .arg(PROBE)
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return load_compiler(provider),
            Ok(Some(status)) => {
                eprintln!(
                    "Protected Direct3D 12 probe exited with {status}. Using software graphics."
                );
                return false;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                eprintln!("Protected Direct3D 12 probe did not complete. Using software graphics.");
                return false;
            }
        }
    }
}

/// Mesa opens the shader validator by name after initialization. Hold references
/// to the packaged pair for the process lifetime, without changing DLL search
/// paths or passing new PATH entries to Rhino, WebView or the updater.
fn load_compiler(provider: &Path) -> bool {
    ["dxcompiler.dll", "dxil.dll"].iter().all(|file| {
        let path = provider.with_file_name(file);
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: the path is absolute, installation-local, and NUL terminated.
        unsafe {
            LoadLibraryExW(
                PCWSTR(wide.as_ptr()),
                None,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
            .is_ok()
        }
    })
}

pub(crate) fn graphics_probe(args: &[String]) -> Option<ExitCode> {
    if args.len() != 2 || args[1] != PROBE {
        return None;
    }
    let passed = packaged_provider().is_some_and(|provider| {
        if !load_compiler(&provider) {
            return false;
        }
        std::env::set_var(PROVIDER, provider);
        std::env::set_var("GALLIUM_DRIVER", "d3d12");
        check_window()
    });
    Some(if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn check_window() -> bool {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_visible(false)
            .with_active(false)
            .with_inner_size([32.0, 32.0]),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        persist_window: false,
        run_and_return: true,
        ..Default::default()
    };
    let passed = Cell::new(false);
    let result = eframe::run_native(
        "Slate graphics probe",
        options,
        Box::new(|cc| {
            if let Some(gl) = cc.gl.as_ref() {
                // SAFETY: eframe has created its window shaders and made this
                // window's GL context current on the calling thread.
                unsafe {
                    let renderer = gl.get_parameter_string(glow::RENDERER);
                    if renderer.starts_with("D3D12 (Intel") {
                        gl.disable(glow::SCISSOR_TEST);
                        gl.disable(glow::FRAMEBUFFER_SRGB);
                        gl.clear_color(0.25, 0.5, 0.75, 1.0);
                        gl.clear(glow::COLOR_BUFFER_BIT);
                        let mut pixel = [0u8; 4];
                        gl.read_pixels(
                            0,
                            0,
                            1,
                            1,
                            glow::RGBA,
                            glow::UNSIGNED_BYTE,
                            glow::PixelPackData::Slice(Some(&mut pixel)),
                        );
                        passed.set(
                            pixel[..3]
                                .iter()
                                .zip([64, 128, 191])
                                .all(|(actual, expected)| actual.abs_diff(expected) <= 1),
                        );
                    }
                }
            }
            // Stop after context creation and readback, before eframe displays its
            // first frame. Returning unwinds the native window and driver context.
            Err("graphics probe finished".into())
        }),
    );
    matches!(result, Err(eframe::Error::AppCreation(_))) && passed.get()
}
