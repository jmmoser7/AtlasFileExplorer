//! Test-only observer of this probe's composed window. No input injection.
//! Reuses the web pixel backend's D3D device and staging-texture owner.
use super::super::*;
use crossbeam_channel::{bounded, Receiver};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Direct3D11::{D3D11_BOX, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ};
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

pub(super) type Pending = Arc<Mutex<Option<(u8, Instant, Option<i64>)>>>;

#[derive(serde::Serialize)]
pub(super) struct Observed {
    pub sequence: u8,
    pub input_to_capture_ms: f64,
    pub capture_clock_100ns: i64,
    pub request_clock_100ns: Option<i64>,
    pub observer_clock_100ns: Option<i64>,
    pub request_to_compositor_ms: Option<f64>,
    pub observer_after_compositor_ms: Option<f64>,
    pub clock_issue: Option<&'static str>,
    pub marker_available_clock_100ns: Option<i64>,
    pub input_to_marker_available_ms: Option<f64>,
    pub roi_readback_and_hash_ms: Option<f64>,
    pub marker_clock_issue: Option<&'static str>,
    pub pixels_changed: bool,
}

pub(super) struct WindowCapture {
    pub pending: Pending,
    pub received: Receiver<Observed>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WindowCapture {
    pub fn start(hwnd: usize, wake: egui::Context) -> Result<Self, String> {
        let pending = Pending::default();
        let watched = pending.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (ready, readiness) = bounded(1);
        let (send, received) = bounded(256);
        let thread = std::thread::spawn(move || {
            use windows::Win32::System::Com::{
                CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED,
            };
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let run = || -> windows::core::Result<()> {
                let (device, context) = create_d3d_device()?;
                let dxgi: IDXGIDevice = device.cast()?;
                let capture_device: IDirect3DDevice =
                    unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()?;
                let interop =
                    windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
                // Only the freshly created, owned benchmark HWND reaches here.
                let item: GraphicsCaptureItem =
                    unsafe { interop.CreateForWindow(HWND(hwnd as *mut _)) }?;
                let size = item.Size()?;
                let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
                    &capture_device,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    2,
                    size,
                )?;
                let session = pool.CreateCaptureSession(&item)?;
                session.SetIsCursorCaptureEnabled(false)?;
                session.StartCapture()?;
                let staging = super::super::readback::create_staging(&device, 1, 1)?;
                let scene_area = D3D11_BOX {
                    left: 0,
                    top: size.Height as u32 / 6,
                    right: size.Width as u32,
                    bottom: size.Height as u32 * 5 / 6,
                    front: 0,
                    back: 1,
                };
                let scene_staging = super::super::readback::create_staging(
                    &device,
                    scene_area.right,
                    scene_area.bottom - scene_area.top,
                )?;
                let mut previous_scene_signature = None;
                let _ = ready.send(Ok(()));
                while !stopping.load(Ordering::Relaxed) {
                    if let Ok(frame) = pool.TryGetNextFrame() {
                        let clock = frame.SystemRelativeTime()?.Duration;
                        let source = frame.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?;
                        let texture: ID3D11Texture2D = unsafe { source.GetInterface() }?;
                        let area = D3D11_BOX {
                            left: 16,
                            top: 16,
                            right: 17,
                            bottom: 17,
                            front: 0,
                            back: 1,
                        };
                        unsafe {
                            context.CopySubresourceRegion(
                                &staging,
                                0,
                                0,
                                0,
                                0,
                                &texture,
                                0,
                                Some(&area),
                            );
                        }
                        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                        // This diagnostic observer may wait on its own 1px copy;
                        // it never holds or calls the application's UI thread.
                        unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }?;
                        let pixel =
                            unsafe { std::slice::from_raw_parts(mapped.pData as *const u8, 4) };
                        let matched = if pixel[0].abs_diff(193) <= 2 && pixel[1].abs_diff(53) <= 2 {
                            let mut slot = watched.lock().unwrap();
                            if slot
                                .as_ref()
                                .is_some_and(|(sequence, _, _)| pixel[2] == *sequence)
                            {
                                slot.take()
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        unsafe {
                            context.Unmap(&staging, 0);
                        }
                        let observed = if let Some((sequence, start, request_clock)) = matched {
                            // Actual matching marker pixels are already CPU-readable.
                            // The same held capture frame's ROI is verified below;
                            // timestamp availability before that extra readback/hash.
                            let marker_clock = super::clock::now_100ns();
                            // The diagnostic marker is outside this middle two-thirds
                            // of the actual canvas. Its changing byte cannot satisfy
                            // the independent visible-scene evidence condition.
                            unsafe {
                                context.CopySubresourceRegion(
                                    &scene_staging,
                                    0,
                                    0,
                                    0,
                                    0,
                                    &texture,
                                    0,
                                    Some(&scene_area),
                                );
                            }
                            let mut scene = D3D11_MAPPED_SUBRESOURCE::default();
                            unsafe {
                                context.Map(&scene_staging, 0, D3D11_MAP_READ, 0, Some(&mut scene))
                            }?;
                            let mut signature = 0xcbf29ce484222325u64;
                            for y in (0..scene_area.bottom - scene_area.top).step_by(13) {
                                let row = unsafe {
                                    std::slice::from_raw_parts(
                                        (scene.pData as *const u8)
                                            .add(y as usize * scene.RowPitch as usize),
                                        scene_area.right as usize * 4,
                                    )
                                };
                                for pixel in row.chunks_exact(4).step_by(11) {
                                    signature = (signature
                                        ^ u32::from_le_bytes(pixel.try_into().unwrap()) as u64)
                                        .wrapping_mul(0x100000001b3);
                                }
                            }
                            unsafe {
                                context.Unmap(&scene_staging, 0);
                            }
                            let pixels_changed = previous_scene_signature
                                .replace(signature)
                                .is_some_and(|old| old != signature);
                            let observer_clock = super::clock::now_100ns();
                            let marker_clock_issue =
                                match (request_clock, marker_clock, observer_clock) {
                                    (Some(request), Some(marker), Some(observer))
                                        if request <= marker && marker <= observer =>
                                    {
                                        None
                                    }
                                    (Some(request), Some(marker), _) if marker < request => {
                                        Some("marker availability predates request clock")
                                    }
                                    (_, Some(marker), Some(observer)) if observer < marker => {
                                        Some("observer clock predates marker availability")
                                    }
                                    _ => Some("request, marker or observer QPC unavailable"),
                                };
                            let input_to_marker_available_ms = request_clock
                                .zip(marker_clock)
                                .filter(|_| marker_clock_issue.is_none())
                                .map(|(request, marker)| {
                                    (marker as i128 - request as i128) as f64 / 10_000.0
                                });
                            let roi_readback_and_hash_ms = marker_clock
                                .zip(observer_clock)
                                .filter(|_| marker_clock_issue.is_none())
                                .map(|(marker, observer)| {
                                    (observer as i128 - marker as i128) as f64 / 10_000.0
                                });
                            let clock_issue = match (request_clock, observer_clock) {
                                (Some(request), Some(observer))
                                    if request <= clock && clock <= observer =>
                                {
                                    None
                                }
                                (Some(request), _) if clock < request => {
                                    Some("marked frame predates its request")
                                }
                                (_, Some(observer)) if observer < clock => {
                                    Some("WGC timestamp is after the observer clock")
                                }
                                _ => Some("QPC read, frequency or normalization unavailable"),
                            };
                            let request_to_compositor_ms = request_clock
                                .filter(|_| clock_issue.is_none())
                                .map(|request| (clock as i128 - request as i128) as f64 / 10_000.0);
                            let observer_after_compositor_ms = observer_clock
                                .filter(|_| clock_issue.is_none())
                                .map(|observer| {
                                    (observer as i128 - clock as i128) as f64 / 10_000.0
                                });
                            Some(Observed {
                                sequence,
                                input_to_capture_ms: start.elapsed().as_secs_f64() * 1000.0,
                                capture_clock_100ns: clock,
                                request_clock_100ns: request_clock,
                                observer_clock_100ns: observer_clock,
                                request_to_compositor_ms,
                                observer_after_compositor_ms,
                                clock_issue,
                                marker_available_clock_100ns: marker_clock,
                                input_to_marker_available_ms,
                                roi_readback_and_hash_ms,
                                marker_clock_issue,
                                pixels_changed,
                            })
                        } else {
                            None
                        };
                        frame.Close()?;
                        if let Some(observed) = observed {
                            let _ = send.try_send(observed);
                            wake.request_repaint();
                        }
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                session.Close()?;
                pool.Close()?;
                Ok(())
            };
            if let Err(error) = run() {
                let _ = ready.try_send(Err(error.to_string()));
            }
            if initialized {
                unsafe {
                    CoUninitialize();
                }
            }
        });
        match readiness.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(())) => Ok(Self {
                pending,
                received,
                stop,
                thread: Some(thread),
            }),
            error => {
                stop.store(true, Ordering::Relaxed);
                let _ = thread.join();
                Err(format!("owned window capture failed: {error:?}"))
            }
        }
    }
}

impl Drop for WindowCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
