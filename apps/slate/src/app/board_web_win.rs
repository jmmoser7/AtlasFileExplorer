//! The Windows pixel backend for web portals.
//!
//! WebView2 has no render-to-texture API, so the supported route to offscreen
//! pixels is the one this file takes: host the browser through
//! `CreateCoreWebView2CompositionController`, point it at a composition visual
//! we own, capture that visual with `Windows.Graphics.Capture`, and read the
//! captured D3D11 texture back into an `egui::ColorImage`. That is what buys a
//! web portal the properties the contract promises — z-order, rotation,
//! opacity, and many pages at once — none of which an airspace child window
//! could give.
//!
//! Everything here is derived state. Nothing in this file touches the journal.
//! The page still cannot post messages or host objects into Slate (Art. VII.4).
//! The one exception is a user export: a download named `slate-canvas-*.png`
//! is saved and handed back so the board can place it under the portal.
//!
//! A page's sign-in style pop-up (`window.open` with a size or position) opens
//! in a small Slate-owned window on the same environment and profile, so the
//! opener survives and the sign-in lands in the portal's cookie jar. Ordinary
//! `_blank` links keep navigating the portal in place (D15 / D22, amended 25
//! September 2026).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use eframe::egui;
use slate_doc::scene::NodeId;

use windows::core::{Interface, PCWSTR};
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::System::DispatcherQueueController;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_FLAG_DO_NOT_WAIT,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DispatcherQueueOptions, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    LoadCursorW, HCURSOR, IDC_ARROW, IDC_HAND, IDC_IBEAM, IDC_SIZEALL, IDC_SIZENS, IDC_SIZEWE,
    IDC_WAIT,
};
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows_numerics::Vector2;

use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, GetAvailableCoreWebView2BrowserVersionString,
    ICoreWebView2, ICoreWebView2CompositionController, ICoreWebView2Controller,
    ICoreWebView2Controller2, ICoreWebView2Controller3, ICoreWebView2ControllerOptions,
    ICoreWebView2Environment, ICoreWebView2Environment10, ICoreWebView2Environment3,
    ICoreWebView2ExecuteScriptCompletedHandler, ICoreWebView2NewWindowRequestedEventArgs,
    ICoreWebView2_4, COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS, COREWEBVIEW2_COLOR,
    COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL,
    COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
    COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP, COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
    COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP, COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
    COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN, COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
    COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_LEFT_BUTTON,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_MIDDLE_BUTTON,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_RIGHT_BUTTON,
    COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CallDevToolsProtocolMethodCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, DownloadStartingEventHandler,
    ExecuteScriptCompletedHandler, NavigationCompletedEventHandler, NewWindowRequestedEventHandler,
    SourceChangedEventHandler, StateChangedEventHandler, WindowCloseRequestedEventHandler,
};

use super::board_web::{
    popup_disposition, popup_title, popup_window_rect, PopupBook, PopupDisposition, PopupFeatures,
    PxRect, WebHost, WebInput, WebRequest,
};

/// A download the board should place under the portal. Every other download
/// stays cancelled. The saved name is ours, so a repeated export never
/// overwrites the previous picture.
fn canvas_export_file(suggested: &str) -> Option<std::path::PathBuf> {
    let name = std::path::Path::new(suggested)
        .file_name()?
        .to_string_lossy();
    if !(name.starts_with("slate-canvas-")
        && name.to_ascii_lowercase().ends_with(".png")
        && !name.contains(".."))
    {
        return None;
    }
    let dir = atlas_core::index::data_dir().join("canvas-exports");
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut path = dir.join(format!("slate-canvas-{stamp}.png"));
    let mut n = 0u32;
    while path.exists() {
        n += 1;
        path = dir.join(format!("slate-canvas-{stamp}-{n}.png"));
        if n > 1000 {
            return None;
        }
    }
    Some(path)
}

fn take_pwstr(p: windows::core::PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let text = unsafe { p.to_string() }.ok();
    unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _)) };
    text
}

/// Wide, NUL-terminated, kept alive for the duration of the call.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A per-origin cookie jar inside the shared user-data folder. `None` when
/// this runtime cannot partition profiles; the caller then uses the single
/// default profile.
fn profile_options(
    env: &ICoreWebView2Environment3,
    profile: &str,
) -> Option<ICoreWebView2ControllerOptions> {
    let env10 = env.cast::<ICoreWebView2Environment10>().ok()?;
    let options = unsafe { env10.CreateCoreWebView2ControllerOptions() }.ok()?;
    let name = wide(profile);
    unsafe { options.SetProfileName(PCWSTR(name.as_ptr())) }.ok()?;
    Some(options)
}

/// Composition-controller mouse coordinates use its raw-pixel bounds.
fn capture_point(x: f32, y: f32, scale: f64) -> POINT {
    POINT {
        x: (x as f64 * scale).round() as i32,
        y: (y as f64 * scale).round() as i32,
    }
}

/// `Navigate` takes a URI, not a path: a bare `C:\dir\page.html` is rejected
/// outright, which is why a local dashboard used to sit at `Loading` forever.
/// Remote locators pass through untouched.
pub(crate) fn navigate_uri(target: &str) -> String {
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file://")
        || lower.starts_with("about:")
    {
        return target.to_string();
    }
    let mut out = String::from("file:///");
    for ch in target.chars() {
        match ch {
            '\\' => out.push('/'),
            // Percent-encode what a URI cannot carry literally. Keeping this to
            // the characters that actually appear in paths avoids mangling
            // non-ASCII folder names, which WebView2 accepts as-is.
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '%' => out.push_str("%25"),
            c => out.push(c),
        }
    }
    out
}

/// What the async WebView2 callbacks fill in for one view. The callbacks land
/// on this same (UI) thread through the message loop, so a `RefCell` is the
/// whole synchronisation story.
#[derive(Default)]
struct Pending {
    comp: Option<ICoreWebView2CompositionController>,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    /// A failed navigation, as a human-readable reason (P1.portal.health).
    error: Option<String>,
    /// Where the page actually is, which diverges from the locator as soon as
    /// the human follows a link.
    url: Option<String>,
    /// Set once the visual tree has been handed to the controller.
    attached: bool,
    cancelled: bool,
    document_generation: u64,
    /// Latest dashboard wire script. Re-run after each navigation.
    link_script: String,
    /// The portal on screen, client physical pixels. Pop-ups centre on it.
    anchor: Option<PxRect>,
}

struct View {
    /// Captured root. WebView2 draws into a child of this visual.
    root: ContainerVisual,
    child: ContainerVisual,
    item: Option<GraphicsCaptureItem>,
    pool: Option<Direct3D11CaptureFramePool>,
    session: Option<GraphicsCaptureSession>,
    staging: [Option<ID3D11Texture2D>; 2],
    pending_copy: [bool; 2],
    next_copy: usize,
    scale: f64,
    size: (u32, u32),
    #[allow(dead_code)] // Set on create; not read yet.
    target: String,
    /// Origin profile requested when this view was created. A different
    /// authored origin rebuilds the webview so cookies do not cross sites.
    profile: String,
    /// The most recent readback, kept so a demoted portal still has a poster.
    last: Option<super::board_web::WebFrame>,
    scrollbar_style: Option<(u64, bool, u32, egui::Color32)>,
    shared: Rc<RefCell<Pending>>,
}

pub struct Webview2Host {
    parent: HWND,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    capture_device: IDirect3DDevice,
    compositor: Compositor,
    /// Holding the controller is what keeps this thread's dispatcher queue —
    /// and therefore the compositor — alive.
    _queue: Option<DispatcherQueueController>,
    env: Rc<RefCell<Option<ICoreWebView2Environment3>>>,
    /// Set when the environment callback reports failure. Until then a missing
    /// env just means "still creating", and portals stay on Loading.
    env_failed: Rc<RefCell<bool>>,
    views: HashMap<NodeId, View>,
    /// Admissions that arrived before the environment finished creating.
    deferred: HashMap<NodeId, WebRequest>,
    escape: Rc<Cell<bool>>,
    /// Finished `slate-canvas-*.png` exports, drained on the frame that places them.
    canvas_drops: Rc<RefCell<Vec<(NodeId, std::path::PathBuf)>>>,
    /// Page text read for an agent run, waiting to be taken.
    texts: Rc<RefCell<HashMap<NodeId, Result<String, String>>>>,
    /// Sign-in pop-up windows, by the portal whose page opened them.
    popups: Rc<RefCell<PopupBook<isize>>>,
    wake: egui::Context,
}

impl Drop for Webview2Host {
    fn drop(&mut self) {
        // Fields drop in declaration order, so the D3D device and the
        // dispatcher queue would otherwise die while capture sessions and
        // WebView2 controllers are still alive. That deadlock freezes the
        // desktop compositor on workbook close. Shut the views down first,
        // while both are still alive.
        let ids: Vec<NodeId> = self.views.keys().copied().collect();
        for id in ids {
            self.evict(id);
        }
        let orphans = self.popups.borrow_mut().drain();
        for window in orphans {
            destroy_popup(window);
        }
        self.deferred.clear();
        self._queue.take();
    }
}

impl Webview2Host {
    /// `None` when there is no Evergreen runtime, no GPU device, or no
    /// composition support — the caller keeps the null host and every portal
    /// reports `NoRuntime` rather than stalling (D29).
    pub fn new(parent: HWND, user_data: &std::path::Path, wake: egui::Context) -> Option<Self> {
        if !runtime_installed() {
            return None;
        }
        // A `Compositor` needs a dispatcher queue on this thread. winit has
        // already put the main thread in an STA, so ask for one to match; if a
        // queue is somehow already running, `Compositor::new` below still
        // succeeds and the error here is not fatal.
        let queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_STA,
            })
        }
        .ok();
        let compositor = Compositor::new().ok()?;
        let (device, context) = create_d3d_device().ok()?;
        let dxgi: IDXGIDevice = device.cast().ok()?;
        let capture_device: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
                .ok()?
                .cast()
                .ok()?;

        let env: Rc<RefCell<Option<ICoreWebView2Environment3>>> = Rc::new(RefCell::new(None));
        let env_failed: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let sink = env.clone();
        let fail = env_failed.clone();
        let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
            move |result: windows::core::Result<()>,
                  environment: Option<ICoreWebView2Environment>| {
                match (result, environment) {
                    (Ok(()), Some(environment)) => {
                        match environment.cast::<ICoreWebView2Environment3>() {
                            Ok(e) => *sink.borrow_mut() = Some(e),
                            Err(_) => *fail.borrow_mut() = true,
                        }
                    }
                    _ => *fail.borrow_mut() = true,
                }
                Ok(())
            },
        ));
        let folder = wide(&user_data.to_string_lossy());
        unsafe {
            CreateCoreWebView2EnvironmentWithOptions(
                PCWSTR::null(),
                PCWSTR(folder.as_ptr()),
                None,
                &handler,
            )
        }
        .ok()?;

        Some(Self {
            parent,
            device,
            context,
            capture_device,
            compositor,
            _queue: queue,
            env,
            env_failed,
            views: HashMap::new(),
            deferred: HashMap::new(),
            escape: Rc::new(Cell::new(false)),
            canvas_drops: Rc::new(RefCell::new(Vec::new())),
            texts: Rc::new(RefCell::new(HashMap::new())),
            popups: Rc::new(RefCell::new(PopupBook::default())),
            wake,
        })
    }

    /// Build the visual tree, the browser, and the capture session for one
    /// portal. Everything after the controller callback is asynchronous.
    fn create_view(&mut self, id: NodeId, req: &WebRequest) -> windows::core::Result<()> {
        let env = match self.env.borrow().clone() {
            Some(env) => env,
            None => {
                self.deferred.insert(id, req.clone());
                return Ok(());
            }
        };

        let (w, h) = (req.raster_w.max(1), req.raster_h.max(1));
        let scale = req.rasterization_scale();
        let root = self.compositor.CreateContainerVisual()?;
        root.SetSize(Vector2 {
            X: w as f32,
            Y: h as f32,
        })?;
        root.SetIsVisible(true)?;
        let child = self.compositor.CreateContainerVisual()?;
        child.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        root.Children()?.InsertAtTop(&child)?;

        let shared: Rc<RefCell<Pending>> = Rc::new(RefCell::new(Pending {
            link_script: req.link_script.clone(),
            anchor: req.anchor_px,
            ..Pending::default()
        }));
        let sink = shared.clone();
        let visual = child.clone();
        let target = req.target.clone();
        let bounds = RECT {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        };
        let escape = self.escape.clone();
        let profile = req.profile.clone();
        let hooks = PageHooks {
            portal: id,
            wake: self.wake.clone(),
            drops: self.canvas_drops.clone(),
            env: env.clone(),
            profile: profile.clone(),
            owner: self.parent,
            compositor: self.compositor.clone(),
            popups: self.popups.clone(),
            anchor: PopupAnchor::Portal(Rc::downgrade(&shared)),
        };
        let handler = CreateCoreWebView2CompositionControllerCompletedHandler::create(Box::new(
            move |result: windows::core::Result<()>,
                  comp: Option<ICoreWebView2CompositionController>| {
                let Some(comp) = comp else {
                    sink.borrow_mut().error = Some(match result {
                        Err(e) => format!("WebView2 could not start: {e}"),
                        Ok(()) => "WebView2 could not start".into(),
                    });
                    return Ok(());
                };
                if sink.borrow().cancelled {
                    if let Ok(controller) = comp.cast::<ICoreWebView2Controller>() {
                        let _ = unsafe { controller.Close() };
                    }
                    return Ok(());
                }
                if let Err(e) = attach(
                    &comp,
                    &visual,
                    bounds,
                    scale,
                    &target,
                    &sink,
                    escape.clone(),
                    hooks.clone(),
                ) {
                    sink.borrow_mut().error = Some(format!("WebView2 could not start: {e}"));
                }
                Ok(())
            },
        ));
        if let Some(options) = profile_options(&env, &profile) {
            let env10 = env.cast::<ICoreWebView2Environment10>()?;
            unsafe {
                env10.CreateCoreWebView2CompositionControllerWithOptions(
                    self.parent,
                    &options,
                    &handler,
                )
            }?;
        } else {
            unsafe { env.CreateCoreWebView2CompositionController(self.parent, &handler) }?;
        }

        self.views.insert(
            id,
            View {
                root,
                child,
                item: None,
                pool: None,
                session: None,
                staging: [None, None],
                pending_copy: [false; 2],
                next_copy: 0,
                scale,
                size: (w, h),
                target: req.target.clone(),
                profile,
                last: None,
                scrollbar_style: None,
                shared,
            },
        );
        Ok(())
    }

    /// Start capturing a view once its controller exists. Idempotent.
    fn start_capture(&mut self, id: NodeId) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        if view.pool.is_some() || !view.shared.borrow().attached {
            return;
        }
        let size = SizeInt32 {
            Width: view.size.0 as i32,
            Height: view.size.1 as i32,
        };
        let Ok(item) = GraphicsCaptureItem::CreateFromVisual(&view.root) else {
            return;
        };
        let Ok(pool) = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &self.capture_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        ) else {
            return;
        };
        let Ok(session) = pool.CreateCaptureSession(&item) else {
            return;
        };
        // A captured page is not a screenshot of the user's desktop: no cursor,
        // no capture border.
        let _ = session.SetIsCursorCaptureEnabled(false);
        let _ = session.SetIsBorderRequired(false);
        if session.StartCapture().is_err() {
            return;
        }
        view.item = Some(item);
        view.pool = Some(pool);
        view.session = Some(session);
    }

    fn resize(&mut self, id: NodeId, req: &WebRequest) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        if !view.shared.borrow().attached {
            return;
        }
        let (w, h) = (req.raster_w.max(1), req.raster_h.max(1));
        // Retain an already-sharp tier while zooming out. Only a changed CSS
        // viewport or a quality upgrade should recreate capture buffers.
        let same_layout = (view.size.0 as f64 / view.scale - req.width_css as f64).abs() <= 1.0
            && (view.size.1 as f64 / view.scale - req.height_css as f64).abs() <= 1.0;
        if same_layout && w <= view.size.0 && h <= view.size.1 {
            return;
        }
        if view.size == (w, h) && view.scale == req.rasterization_scale() {
            return;
        }
        if view.size == (w, h) {
            view.scale = req.rasterization_scale();
            if let Some(controller) = view.shared.borrow().controller.clone() {
                if let Ok(c3) = controller.cast::<ICoreWebView2Controller3>() {
                    let _ = unsafe { c3.SetRasterizationScale(req.rasterization_scale()) };
                }
            }
            return;
        }
        view.size = (w, h);
        view.scale = req.rasterization_scale();
        view.staging = [None, None];
        view.pending_copy = [false; 2];
        view.next_copy = 0;
        let _ = view.root.SetSize(Vector2 {
            X: w as f32,
            Y: h as f32,
        });
        if let Some(controller) = view.shared.borrow().controller.clone() {
            if let Ok(c3) = controller.cast::<ICoreWebView2Controller3>() {
                let _ = unsafe { c3.SetRasterizationScale(req.rasterization_scale()) };
            }
            let _ = unsafe {
                controller.SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: w as i32,
                    bottom: h as i32,
                })
            };
        }
        if let Some(pool) = &view.pool {
            let _ = pool.Recreate(
                &self.capture_device,
                DirectXPixelFormat::B8G8R8A8UIntNormalized,
                2,
                SizeInt32 {
                    Width: w as i32,
                    Height: h as i32,
                },
            );
        }
    }

    /// Read a completed GPU copy without waiting, then queue the newest frame.
    /// Capture dimensions and the per-frame upload budget bound the CPU work.
    fn read_frame(&mut self, id: NodeId) -> Option<super::board_web::WebFrame> {
        let _span = atlas_core::session_log::span("slate.web.readback");
        let view = self.views.get_mut(&id)?;
        let pool = view.pool.as_ref()?;
        let size = view.size;
        let read = (view.next_copy + 1) % 2;
        let mut image = None;
        if view.pending_copy[read] {
            let staging = view.staging[read].as_ref()?;
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            // Never wait for the GPU on the UI thread. Retry this copy on a
            // later frame; the existing portal texture remains on screen.
            unsafe {
                self.context.Map(
                    staging,
                    0,
                    D3D11_MAP_READ,
                    D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                    Some(&mut mapped),
                )
            }
            .ok()?;
            image = Some(std::sync::Arc::new(bgra_to_color_image(
                &mapped,
                size.0 as usize,
                size.1 as usize,
            )));
            unsafe {
                self.context.Unmap(staging, 0);
            }
            view.pending_copy[read] = false;
        }
        // The capture pool has two buffers. Prefer the newest and close every
        // acquired frame, including stale frames from a previous resize.
        let mut newest = pool.TryGetNextFrame().ok();
        if let Ok(frame) = pool.TryGetNextFrame() {
            if let Some(old) = newest.replace(frame) {
                let _ = old.Close();
            }
        }
        if let Some(frame) = newest {
            let source = frame
                .Surface()
                .ok()
                .and_then(|s| s.cast::<IDirect3DDxgiInterfaceAccess>().ok())
                .and_then(|a| unsafe { a.GetInterface::<ID3D11Texture2D>() }.ok());
            if let Some(source) = source {
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                unsafe {
                    source.GetDesc(&mut desc);
                }
                if (desc.Width, desc.Height) == size {
                    let write = view.next_copy;
                    if view.staging[write].is_none() {
                        view.staging[write] = create_staging(&self.device, size.0, size.1).ok();
                    }
                    if let Some(staging) = &view.staging[write] {
                        unsafe {
                            self.context.CopyResource(staging, &source);
                            self.context.Flush();
                        }
                        view.pending_copy[write] = true;
                        view.next_copy = read;
                    }
                }
            }
            let _ = frame.Close();
        }
        let image = image.filter(|img| super::board_web::web_frame_has_content(img));
        if let Some(img) = &image {
            view.last = Some(std::sync::Arc::clone(img));
        }
        image
    }

    fn webview(&self, id: NodeId) -> Option<ICoreWebView2> {
        self.views.get(&id)?.shared.borrow().webview.clone()
    }

    fn devtools(&self, id: NodeId, method: &str, params: &str) {
        let Some(webview) = self
            .views
            .get(&id)
            .and_then(|v| v.shared.borrow().webview.clone())
        else {
            return;
        };
        let m = wide(method);
        let p = wide(params);
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())));
        let _ = unsafe {
            webview.CallDevToolsProtocolMethod(PCWSTR(m.as_ptr()), PCWSTR(p.as_ptr()), &handler)
        };
    }

    fn refresh_link_script(&mut self, id: NodeId, script: &str) {
        let Some(view) = self.views.get(&id) else {
            return;
        };
        let webview = {
            let pending = view.shared.borrow();
            if pending.link_script == script {
                return;
            }
            pending.webview.clone()
        };
        view.shared.borrow_mut().link_script = script.to_string();
        if script.is_empty() {
            return;
        }
        if let Some(webview) = webview {
            let script = wide(script);
            let _ = unsafe {
                webview.ExecuteScript(
                    PCWSTR(script.as_ptr()),
                    None::<&ICoreWebView2ExecuteScriptCompletedHandler>,
                )
            };
        }
    }
}

impl WebHost for Webview2Host {
    fn request_text(&mut self, id: NodeId) -> bool {
        let Some(webview) = self
            .views
            .get(&id)
            .and_then(|view| view.shared.borrow().webview.clone())
        else {
            return false;
        };
        let texts = self.texts.clone();
        let wake = self.wake.clone();
        // A fixed, read-only expression: no interpolation, and nothing the
        // page can call back into.
        let script = wide("document.body ? document.body.innerText : ''");
        let handler = ExecuteScriptCompletedHandler::create(Box::new(
            move |result: windows::core::Result<()>, json: String| {
                let text = result
                    .map_err(|e| format!("The page could not be read: {}", e.message()))
                    .and_then(|()| {
                        serde_json::from_str::<String>(&json)
                            .map_err(|_| "The page returned no text.".to_string())
                    });
                texts.borrow_mut().insert(id, text);
                wake.request_repaint();
                Ok(())
            },
        ));
        unsafe { webview.ExecuteScript(PCWSTR(script.as_ptr()), &handler) }.is_ok()
    }

    fn take_text(&mut self, id: NodeId) -> Option<Result<String, String>> {
        self.texts.borrow_mut().remove(&id)
    }

    fn set_scrollbars(&mut self, id: NodeId, visible: bool, width_css: f32, color: egui::Color32) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        let pending = view.shared.borrow();
        let Some(webview) = pending.webview.as_ref() else {
            return;
        };
        let width = (width_css.max(0.1) * 100.0).round() as u32;
        let key = (pending.document_generation, visible, width, color);
        if view.scrollbar_style == Some(key) {
            return;
        }
        let css = atlas_shell::tabs::web_scrollbar_style(width as f32 / 100.0, color, visible);
        // Fixed host chrome only; no URL interpolation or message bridge. The
        // one DOM read is `request_text`, started by the human (D15 / D27).
        // Reinstall after navigation replaces the document.
        let script = format!("(()=>{{let s=document.getElementById('slate-web-scrollbar-chrome');if(!s){{s=document.createElement('style');s.id='slate-web-scrollbar-chrome';(document.head||document.documentElement).appendChild(s);}}s.textContent={};}})()", serde_json::to_string(&css).unwrap());
        let script = wide(&script);
        if unsafe {
            webview.ExecuteScript(
                PCWSTR(script.as_ptr()),
                None::<&ICoreWebView2ExecuteScriptCompletedHandler>,
            )
        }
        .is_ok()
        {
            view.scrollbar_style = Some(key);
        }
    }

    fn take_escape(&mut self) -> bool {
        self.escape.replace(false)
    }
    fn release_keyboard(&self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
        use windows::Win32::UI::WindowsAndMessaging::IsChild;
        let focused = unsafe { GetFocus() };
        // Only a portal's browser, a child of the board window, is ours to take
        // back. A sign-in pop-up is a separate window the person is typing in,
        // and a focused board field calls this every frame.
        if !focused.is_invalid()
            && focused != self.parent
            && unsafe { IsChild(self.parent, focused) }.as_bool()
        {
            let _ = unsafe { SetFocus(Some(self.parent)) };
        }
    }
    fn take_canvas_drops(&mut self) -> Vec<(NodeId, std::path::PathBuf)> {
        std::mem::take(&mut *self.canvas_drops.borrow_mut())
    }
    fn available(&self) -> bool {
        !*self.env_failed.borrow()
    }

    fn admit(&mut self, id: NodeId, req: &WebRequest) {
        if *self.env_failed.borrow() {
            self.deferred.clear();
            return;
        }
        // The environment may have arrived since an earlier admission.
        if !self.deferred.is_empty() && self.env.borrow().is_some() {
            for (pending_id, pending) in std::mem::take(&mut self.deferred) {
                let _ = self.create_view(pending_id, &pending);
            }
        }
        if self
            .views
            .get(&id)
            .is_some_and(|view| view.profile != req.profile)
        {
            // Rebind to another origin. In-page hops stay in the profile of
            // the authored locator; only a new locator changes the jar.
            self.evict(id);
        }
        if self.views.contains_key(&id) {
            // In-page navigation and camera zoom must not rebuild the webview.
            self.resize(id, req);
            self.refresh_link_script(id, &req.link_script);
            if let Some(view) = self.views.get(&id) {
                view.shared.borrow_mut().anchor = req.anchor_px;
            }
        } else {
            let _ = self.create_view(id, req);
        }
        self.start_capture(id);
    }

    fn holds_popup(&self, id: NodeId) -> bool {
        self.popups.borrow().holds(id)
    }

    fn evict(&mut self, id: NodeId) {
        self.deferred.remove(&id);
        // A pop-up without its opener can only fail; close it with the portal.
        let orphans = self.popups.borrow_mut().evicted(id);
        for window in orphans {
            destroy_popup(window);
        }
        let Some(view) = self.views.remove(&id) else {
            return;
        };
        view.shared.borrow_mut().cancelled = true;
        if let Some(session) = &view.session {
            let _ = session.Close();
        }
        if let Some(pool) = &view.pool {
            let _ = pool.Close();
        }
        if let Some(controller) = view.shared.borrow().controller.clone() {
            let _ = unsafe { controller.Close() };
        }
        let _ = view.child.SetIsVisible(false);
        let _ = view.root.SetIsVisible(false);
    }

    fn take_frame(&mut self, id: NodeId) -> Option<super::board_web::WebFrame> {
        self.start_capture(id);
        self.read_frame(id)
    }

    fn capture_poster(&mut self, id: NodeId) -> Option<super::board_web::WebFrame> {
        self.read_frame(id)
            .or_else(|| self.views.get(&id).and_then(|v| v.last.clone()))
    }

    fn last_frame(&self, id: NodeId) -> Option<super::board_web::WebFrame> {
        self.views.get(&id).and_then(|v| v.last.clone())
    }

    fn send_input(&mut self, id: NodeId, input: WebInput) {
        let Some(comp) = self
            .views
            .get(&id)
            .and_then(|v| v.shared.borrow().comp.clone())
        else {
            return;
        };
        let scale = self.views.get(&id).map_or(1.0, |v| v.scale);
        let point = |x: f32, y: f32| capture_point(x, y, scale);
        let none = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE;
        let mouse = |kind: COREWEBVIEW2_MOUSE_EVENT_KIND,
                     data: u32,
                     p: POINT,
                     keys: COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS| {
            let _ = unsafe { comp.SendMouseInput(kind, keys, data, p) };
        };
        match input {
            WebInput::Move { x, y, buttons } => mouse(
                COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
                0,
                point(x, y),
                held_keys(buttons),
            ),
            WebInput::Down { x, y, button } => mouse(
                match button {
                    1 => COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
                    2 => COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
                    _ => COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
                },
                0,
                point(x, y),
                none,
            ),
            WebInput::Up { x, y, button } => mouse(
                match button {
                    1 => COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
                    2 => COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP,
                    _ => COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
                },
                0,
                point(x, y),
                none,
            ),
            WebInput::Wheel {
                x,
                y,
                delta,
                horizontal,
            } => mouse(
                if horizontal {
                    COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL
                } else {
                    COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL
                },
                wheel_data(delta),
                point(x, y),
                none,
            ),
            WebInput::Leave => mouse(
                COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
                0,
                POINT { x: 0, y: 0 },
                none,
            ),
            // Keyboard goes through DevTools rather than the parent window's
            // focus, so egui keeps deciding what reaches the page and Esc can
            // still peel focus off it (D22).
            WebInput::Key { key, pressed } => {
                let Some(code) = virtual_key(key) else { return };
                let kind = if pressed { "keyDown" } else { "keyUp" };
                self.devtools(
                    id,
                    "Input.dispatchKeyEvent",
                    &format!(
                        "{{\"type\":\"{kind}\",\"windowsVirtualKeyCode\":{code},\
                         \"nativeVirtualKeyCode\":{code}}}"
                    ),
                );
            }
            WebInput::Text(c) => {
                let mut buf = [0u8; 4];
                let text = json_escape(c.encode_utf8(&mut buf));
                self.devtools(id, "Input.insertText", &format!("{{\"text\":\"{text}\"}}"));
            }
        }
    }

    fn cursor(&self, id: NodeId) -> Option<egui::CursorIcon> {
        let comp = self
            .views
            .get(&id)
            .and_then(|v| v.shared.borrow().comp.clone())?;
        let mut cursor = HCURSOR::default();
        unsafe { comp.Cursor(&mut cursor) }.ok()?;
        map_cursor(cursor)
    }

    fn load_error(&self, id: NodeId) -> Option<String> {
        self.views.get(&id)?.shared.borrow().error.clone()
    }

    fn current_url(&self, id: NodeId) -> Option<String> {
        self.views.get(&id)?.shared.borrow().url.clone()
    }

    fn go_back(&mut self, id: NodeId) -> bool {
        let Some(webview) = self.webview(id) else {
            return false;
        };
        let mut can = windows::core::BOOL(0);
        let _ = unsafe { webview.CanGoBack(&mut can) };
        can.as_bool() && unsafe { webview.GoBack() }.is_ok()
    }

    fn go_forward(&mut self, id: NodeId) -> bool {
        let Some(webview) = self.webview(id) else {
            return false;
        };
        let mut can = windows::core::BOOL(0);
        let _ = unsafe { webview.CanGoForward(&mut can) };
        can.as_bool() && unsafe { webview.GoForward() }.is_ok()
    }

    fn reload(&mut self, id: NodeId) -> bool {
        let Some(webview) = self.webview(id) else {
            return false;
        };
        unsafe { webview.Reload() }.is_ok()
    }

    fn navigate(&mut self, id: NodeId, target: &str) -> bool {
        let Some(webview) = self.webview(id) else {
            return false;
        };
        let url = wide(&navigate_uri(target));
        unsafe { webview.Navigate(PCWSTR(url.as_ptr())) }.is_ok()
    }
}

/// Buttons held during a move, so a drag reads as a drag inside the page.
fn held_keys(buttons: u8) -> COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS {
    let mut keys = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE;
    if buttons & 1 != 0 {
        keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_LEFT_BUTTON;
    }
    if buttons & 2 != 0 {
        keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_RIGHT_BUTTON;
    }
    if buttons & 4 != 0 {
        keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_MIDDLE_BUTTON;
    }
    keys
}

/// egui reports scroll in points; Windows counts notches of `WHEEL_DELTA`.
/// Without the conversion a notch moved a page by a couple of lines and
/// scrolling felt broken.
fn wheel_data(delta: f32) -> u32 {
    const WHEEL_DELTA: f32 = 120.0;
    // egui's Windows backend reports roughly 50 points per notch.
    let notches = delta / 50.0;
    let scaled = (notches * WHEEL_DELTA).round() as i32;
    let clamped = if scaled == 0 {
        WHEEL_DELTA as i32 * delta.signum() as i32
    } else {
        scaled
    };
    clamped as u32
}

/// Wire a freshly created composition controller to our visual and point it at
/// the page. Runs inside the WebView2 completion callback.
#[allow(clippy::too_many_arguments)]
fn attach(
    comp: &ICoreWebView2CompositionController,
    visual: &ContainerVisual,
    bounds: RECT,
    scale: f64,
    target: &str,
    sink: &Rc<RefCell<Pending>>,
    escape: Rc<Cell<bool>>,
    hooks: PageHooks,
) -> windows::core::Result<()> {
    let wake = hooks.wake.clone();
    unsafe { comp.SetRootVisualTarget(visual) }?;
    let controller: ICoreWebView2Controller = comp.cast()?;
    unsafe {
        // Raw pixels: the capture is the portal's on-screen physical size.
        // RasterizationScale is physical/CSS so Fit still lays out at
        // width_css and the bitmap matches the frame (not a 1× stretch).
        if let Ok(c3) = controller.cast::<ICoreWebView2Controller3>() {
            let _ = c3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS);
            let _ = c3.SetShouldDetectMonitorScaleChanges(false);
            let _ = c3.SetRasterizationScale(scale);
        }
        // Transparent, so a page that does not paint a background shows the
        // portal's own fill rather than white.
        if let Ok(c2) = controller.cast::<ICoreWebView2Controller2>() {
            let _ = c2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 0,
                R: 0,
                G: 0,
                B: 0,
            });
        }
        controller.SetBounds(bounds)?;
        controller.SetIsVisible(true)?;
    }
    let accelerator = AcceleratorKeyPressedEventHandler::create(Box::new(move |sender, args| {
        if let Some(args) = args {
            let mut key = 0;
            let mut kind = Default::default();
            unsafe {
                args.VirtualKey(&mut key)?;
                args.KeyEventKind(&mut kind)?;
            }
            if key == 0x1b {
                unsafe {
                    args.SetHandled(true)?;
                }
                use webview2_com::Microsoft::Web::WebView2::Win32::{
                    COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
                    COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
                };
                if kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    || kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    let mut status = Default::default();
                    unsafe {
                        args.PhysicalKeyStatus(&mut status)?;
                    }
                    if !status.WasKeyDown.as_bool() {
                        // Return native keyboard ownership too; subsequent keys
                        // must go through Slate's routing after Escape.
                        if let Some(controller) = sender {
                            let mut parent = HWND::default();
                            if unsafe { controller.ParentWindow(&mut parent) }.is_ok() {
                                let _ = unsafe {
                                    windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(
                                        parent,
                                    ))
                                };
                            }
                        }
                        escape.set(true);
                        wake.request_repaint();
                    }
                }
            }
        }
        Ok(())
    }));
    let mut accelerator_token = 0;
    unsafe {
        controller.add_AcceleratorKeyPressed(&accelerator, &mut accelerator_token)?;
    }
    let webview = unsafe { controller.CoreWebView2() }?;
    apply_page_settings(&webview);

    let errors = Rc::downgrade(sink);
    let nav = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
        let Some(errors) = errors.upgrade() else {
            return Ok(());
        };
        let mut succeeded = false;
        let mut link_script = String::new();
        if let Some(args) = args {
            let mut ok = windows::core::BOOL(0);
            let _ = unsafe { args.IsSuccess(&mut ok) };
            succeeded = ok.as_bool();
            let mut pending = errors.borrow_mut();
            pending.document_generation = pending.document_generation.wrapping_add(1);
            link_script = pending.link_script.clone();
            pending.error = if succeeded {
                None
            } else {
                let mut status = Default::default();
                let _ = unsafe { args.WebErrorStatus(&mut status) };
                Some(format!("the page did not load (error {})", status.0))
            };
        }
        // Following a link changes what the frame shows; saying so is what
        // keeps it honest about being a browser (Art. IV).
        if let Some(sender) = sender {
            let mut source = windows::core::PWSTR::null();
            if unsafe { sender.Source(&mut source) }.is_ok() && !source.is_null() {
                let text = unsafe { source.to_string() }.ok();
                unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(source.0 as *const _)) };
                errors.borrow_mut().url = text;
            }
            if succeeded && !link_script.is_empty() {
                let script = wide(&link_script);
                let _ = unsafe {
                    sender.ExecuteScript(
                        PCWSTR(script.as_ptr()),
                        None::<&ICoreWebView2ExecuteScriptCompletedHandler>,
                    )
                };
            }
        }
        Ok(())
    }));
    let mut token = 0i64;
    let _ = unsafe { webview.add_NavigationCompleted(&nav, &mut token) };

    install_popup_policy(&webview, hooks.clone());
    install_download_policy(&webview, &hooks);
    // `window.close()` in the portal's own page must not close the portal.
    let ignore_close = WindowCloseRequestedEventHandler::create(Box::new(|_, _| Ok(())));
    let mut close_token = 0i64;
    let _ = unsafe { webview.add_WindowCloseRequested(&ignore_close, &mut close_token) };

    let url = wide(&navigate_uri(target));
    unsafe { webview.Navigate(PCWSTR(url.as_ptr())) }?;

    let mut pending = sink.borrow_mut();
    pending.comp = Some(comp.clone());
    pending.controller = Some(controller);
    pending.webview = Some(webview);
    pending.attached = true;
    Ok(())
}

/// The same for every page Slate hosts, portal or pop-up.
fn apply_page_settings(webview: &ICoreWebView2) {
    if let Ok(settings) = unsafe { webview.Settings() } {
        unsafe {
            let _ = settings.SetAreDefaultContextMenusEnabled(false);
            let _ = settings.SetAreDefaultScriptDialogsEnabled(false);
            let _ = settings.SetIsStatusBarEnabled(false);
            // The page cannot post messages into Slate: there is no channel,
            // which is what makes Art. VII.4 structural rather than a promise.
            let _ = settings.SetIsWebMessageEnabled(false);
            let _ = settings.SetAreHostObjectsAllowed(false);
        }
    }
}

/// Ordinary downloads stay out of the board (D15). A `slate-canvas-*.png`
/// export lands under the portal, whether its page or one of its pop-ups
/// started it.
fn install_download_policy(webview: &ICoreWebView2, hooks: &PageHooks) {
    let Ok(wv4) = webview.cast::<ICoreWebView2_4>() else {
        return;
    };
    let portal = hooks.portal;
    let drops = hooks.drops.clone();
    let export_wake = hooks.wake.clone();
    let download = DownloadStartingEventHandler::create(Box::new(move |_sender, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut suggested = windows::core::PWSTR::null();
        let name = if unsafe { args.ResultFilePath(&mut suggested) }.is_ok() {
            take_pwstr(suggested)
        } else {
            None
        };
        let canvas = name.as_deref().and_then(canvas_export_file);
        let Some(dest) = canvas else {
            let _ = unsafe { args.SetCancel(true) };
            let _ = unsafe { args.SetHandled(true) };
            return Ok(());
        };
        let wide_dest = wide(&dest.to_string_lossy());
        if unsafe { args.SetResultFilePath(PCWSTR(wide_dest.as_ptr())) }.is_err() {
            let _ = unsafe { args.SetCancel(true) };
            let _ = unsafe { args.SetHandled(true) };
            return Ok(());
        }
        // No save dialog. The file lands under the portal when it finishes.
        let _ = unsafe { args.SetHandled(true) };
        if let Ok(op) = unsafe { args.DownloadOperation() } {
            let drops = drops.clone();
            let wake = export_wake.clone();
            let done = StateChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(op) = sender else {
                    return Ok(());
                };
                let mut state = Default::default();
                if unsafe { op.State(&mut state) }.is_ok()
                    && state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED
                    && dest.is_file()
                {
                    drops.borrow_mut().push((portal, dest.clone()));
                    wake.request_repaint();
                }
                Ok(())
            }));
            let mut token = 0i64;
            let _ = unsafe { op.add_StateChanged(&done, &mut token) };
        }
        Ok(())
    }));
    let mut download_token = 0i64;
    let _ = unsafe { wv4.add_DownloadStarting(&download, &mut download_token) };
}

// ---------------------------------------------------------------------------
// Sign-in pop-ups (D15 / D22, amended 25 September 2026)
// ---------------------------------------------------------------------------

/// What a page's handlers need beyond the view itself: where exports land, and
/// how to open a pop-up on the same environment and profile as the portal.
#[derive(Clone)]
struct PageHooks {
    portal: NodeId,
    wake: egui::Context,
    drops: Rc<RefCell<Vec<(NodeId, std::path::PathBuf)>>>,
    env: ICoreWebView2Environment3,
    /// The portal's WebView2 profile. A pop-up on another profile has no
    /// opener and would sign in to the wrong cookie jar.
    profile: String,
    /// Slate's board window, which owns every pop-up.
    owner: HWND,
    /// An environment that hosts composition controllers refuses windowed
    /// ones (`ERROR_INVALID_STATE`), so a pop-up is composition-hosted too.
    compositor: Compositor,
    popups: Rc<RefCell<PopupBook<isize>>>,
    anchor: PopupAnchor,
}

/// What a new pop-up centres on when the page gave no position.
#[derive(Clone)]
enum PopupAnchor {
    /// The portal on the board, in the owner's client coordinates.
    Portal(std::rc::Weak<RefCell<Pending>>),
    /// The pop-up that opened this one.
    Window(HWND),
}

/// A live pop-up window's browser, kept where its window procedure can reach
/// it. Removed on `WM_NCDESTROY`, which is also what marks the handle dead.
struct PopupSlot {
    comp: Option<ICoreWebView2CompositionController>,
    book: std::rc::Weak<RefCell<PopupBook<isize>>>,
    /// The window's visual tree; dropping the target blanks the window.
    target: windows::UI::Composition::Desktop::DesktopWindowTarget,
    root: ContainerVisual,
    /// A `WM_MOUSELEAVE` has been requested for the current hover.
    tracking: bool,
}

thread_local! {
    static POPUP_SLOTS: RefCell<HashMap<isize, PopupSlot>> = RefCell::new(HashMap::new());
}

const POPUP_CLASS: &str = "SlateWebPopup";
/// `WinUser.h`; the `windows` crate files it under a feature this app omits.
const WM_MOUSELEAVE: u32 = 0x02A3;

fn popup_key(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

fn popup_alive(window: isize) -> bool {
    POPUP_SLOTS.with(|slots| slots.borrow().contains_key(&window))
}

fn popup_comp(window: isize) -> Option<ICoreWebView2CompositionController> {
    POPUP_SLOTS.with(|slots| {
        slots
            .borrow()
            .get(&window)
            .and_then(|slot| slot.comp.clone())
    })
}

fn popup_controller(window: isize) -> Option<ICoreWebView2Controller> {
    popup_comp(window).and_then(|comp| comp.cast().ok())
}

/// Only a handle still in the slot table is ours; a recycled one is not.
fn destroy_popup(window: isize) {
    if popup_alive(window) {
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::DestroyWindow(HWND(
                window as *mut std::ffi::c_void,
            ))
        };
    }
}

/// The sizes and position `window.open` asked for, if any.
fn popup_features(args: &ICoreWebView2NewWindowRequestedEventArgs) -> PopupFeatures {
    let mut out = PopupFeatures::default();
    let Ok(features) = (unsafe { args.WindowFeatures() }) else {
        return out;
    };
    let mut has = windows::core::BOOL(0);
    if unsafe { features.HasPosition(&mut has) }.is_ok() && has.as_bool() {
        let (mut left, mut top) = (0u32, 0u32);
        if unsafe { features.Left(&mut left) }.is_ok() && unsafe { features.Top(&mut top) }.is_ok()
        {
            // Screens left of or above the primary report negative values.
            out.position = Some((left as i32, top as i32));
        }
    }
    let mut has = windows::core::BOOL(0);
    if unsafe { features.HasSize(&mut has) }.is_ok() && has.as_bool() {
        let (mut width, mut height) = (0u32, 0u32);
        if unsafe { features.Width(&mut width) }.is_ok()
            && unsafe { features.Height(&mut height) }.is_ok()
        {
            out.size = Some((width, height));
        }
    }
    out
}

/// One rule for the portal's page and for every pop-up it opens.
fn install_popup_policy(webview: &ICoreWebView2, hooks: PageHooks) {
    let handler = NewWindowRequestedEventHandler::create(Box::new(move |sender, args| {
        let Some(args) = args else {
            return Ok(());
        };
        let mut uri = windows::core::PWSTR::null();
        let target = if unsafe { args.Uri(&mut uri) }.is_ok() {
            take_pwstr(uri)
        } else {
            None
        };
        let features = popup_features(&args);
        if popup_disposition(&features) == PopupDisposition::OwnedWindow
            && open_popup(
                &hooks,
                &args,
                &features,
                target.as_deref().unwrap_or_default(),
            )
        {
            return Ok(());
        }
        // Handled with no NewWindow: the view that asked navigates instead, so
        // a search result opened in a new tab does not vanish.
        let _ = unsafe { args.SetHandled(true) };
        if let (Some(sender), Some(target)) = (sender, target) {
            let url = wide(&navigate_uri(&target));
            let _ = unsafe { sender.Navigate(PCWSTR(url.as_ptr())) };
        }
        Ok(())
    }));
    let mut token = 0i64;
    let _ = unsafe { webview.add_NewWindowRequested(&handler, &mut token) };
}

/// Where the pop-up goes, in physical screen pixels.
fn popup_placement(hooks: &PageHooks, features: &PopupFeatures) -> PxRect {
    use windows::Win32::Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::HiDpi::{
        AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    };

    let window_rect = |hwnd: HWND| {
        let mut r = RECT::default();
        let _ = unsafe { GetWindowRect(hwnd, &mut r) };
        PxRect::new(r.left, r.top, r.right, r.bottom)
    };
    let anchor = match &hooks.anchor {
        PopupAnchor::Portal(pending) => {
            pending
                .upgrade()
                .and_then(|p| p.borrow().anchor)
                .map(|client| {
                    let mut origin = POINT { x: 0, y: 0 };
                    let _ = unsafe { ClientToScreen(hooks.owner, &mut origin) };
                    client.offset(origin.x, origin.y)
                })
        }
        PopupAnchor::Window(hwnd) => Some(window_rect(*hwnd)),
    }
    .filter(|r| r.width() > 0 && r.height() > 0)
    .unwrap_or_else(|| window_rect(hooks.owner));

    let probe = RECT {
        left: anchor.left,
        top: anchor.top,
        right: anchor.right,
        bottom: anchor.bottom,
    };
    let monitor = unsafe { MonitorFromRect(&probe, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let work = if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        let w = info.rcWork;
        PxRect::new(w.left, w.top, w.right, w.bottom)
    } else {
        anchor
    };
    let (mut dpi, mut dpi_y) = (0u32, 0u32);
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) }.is_err()
        || dpi == 0
    {
        dpi = unsafe { GetDpiForWindow(hooks.owner) }.max(96);
    }
    let mut frame = RECT::default();
    let _ = unsafe {
        AdjustWindowRectExForDpi(
            &mut frame,
            WS_OVERLAPPEDWINDOW,
            false,
            WINDOW_EX_STYLE::default(),
            dpi,
        )
    };
    let frame = PxRect::new(-frame.left, -frame.top, frame.right, frame.bottom);
    popup_window_rect(features, dpi as f64 / 96.0, frame, anchor, work)
}

fn register_popup_class() {
    use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, WHITE_BRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{RegisterClassW, CS_DBLCLKS, WNDCLASSW};
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return;
        };
        let class = wide(POPUP_CLASS);
        let wc = WNDCLASSW {
            // The page gets real double-clicks, not two fast single clicks.
            style: CS_DBLCLKS,
            lpfnWndProc: Some(popup_proc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
}

/// An owned top-level window: above Slate, and not a taskbar button of its own.
fn create_popup_window(owner: HWND, rect: PxRect, title: &str) -> windows::core::Result<HWND> {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, SendMessageW, ShowWindow, ICON_BIG, ICON_SMALL, SW_SHOWNORMAL,
        WINDOW_EX_STYLE, WM_GETICON, WM_SETICON, WS_OVERLAPPEDWINDOW,
    };
    register_popup_class();
    let instance = unsafe { GetModuleHandleW(None) }?;
    let class = wide(POPUP_CLASS);
    let title = wide(title);
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            Some(owner),
            None,
            Some(instance.into()),
            None,
        )
    }?;
    for kind in [ICON_SMALL, ICON_BIG] {
        let icon = unsafe { SendMessageW(owner, WM_GETICON, Some(WPARAM(kind as usize)), None) };
        if icon.0 != 0 {
            unsafe {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(kind as usize)),
                    Some(LPARAM(icon.0)),
                )
            };
        }
    }
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNORMAL) };
    Ok(hwnd)
}

/// Opens a sign-in pop-up in its own window. `false` when no window could be
/// made, so the caller falls back to navigating in place.
fn open_popup(
    hooks: &PageHooks,
    args: &ICoreWebView2NewWindowRequestedEventArgs,
    features: &PopupFeatures,
    uri: &str,
) -> bool {
    let rect = popup_placement(hooks, features);
    let Ok(hwnd) = create_popup_window(hooks.owner, rect, &popup_title(uri)) else {
        return false;
    };
    let window = popup_key(hwnd);
    let Ok((target, root)) = popup_visuals(&hooks.compositor, hwnd) else {
        let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd) };
        return false;
    };
    POPUP_SLOTS.with(|slots| {
        slots.borrow_mut().insert(
            window,
            PopupSlot {
                comp: None,
                book: Rc::downgrade(&hooks.popups),
                target,
                root: root.clone(),
                tracking: false,
            },
        )
    });
    let Ok(deferral) = (unsafe { args.GetDeferral() }) else {
        destroy_popup(window);
        return false;
    };
    hooks.popups.borrow_mut().opened(hooks.portal, window);
    // Completed exactly once: by the controller callback, or below if the
    // controller could not even be requested.
    let deferral = Rc::new(RefCell::new(Some(deferral)));
    let complete = deferral.clone();

    let pending_args = args.clone();
    let nested = PageHooks {
        anchor: PopupAnchor::Window(hwnd),
        ..hooks.clone()
    };
    let handler = CreateCoreWebView2CompositionControllerCompletedHandler::create(Box::new(
        move |result: windows::core::Result<()>,
              comp: Option<ICoreWebView2CompositionController>| {
            let close = |comp: &ICoreWebView2CompositionController| {
                if let Ok(controller) = comp.cast::<ICoreWebView2Controller>() {
                    let _ = unsafe { controller.Close() };
                }
            };
            let webview = match (result, comp) {
                (Ok(()), Some(comp)) if popup_alive(window) => {
                    match wire_popup(&comp, hwnd, &root, &nested) {
                        Ok(webview) => {
                            POPUP_SLOTS.with(|slots| {
                                if let Some(slot) = slots.borrow_mut().get_mut(&window) {
                                    slot.comp = Some(comp.clone());
                                }
                            });
                            Some(webview)
                        }
                        Err(_) => {
                            close(&comp);
                            None
                        }
                    }
                }
                (_, Some(comp)) => {
                    close(&comp);
                    None
                }
                _ => None,
            };
            let attached = webview
                .as_ref()
                .is_some_and(|w| unsafe { pending_args.SetNewWindow(w) }.is_ok());
            let _ = unsafe { pending_args.SetHandled(true) };
            if let Some(deferral) = complete.borrow_mut().take() {
                let _ = unsafe { deferral.Complete() };
            }
            if !attached {
                destroy_popup(window);
            }
            Ok(())
        },
    ));
    let created = match profile_options(&hooks.env, &hooks.profile) {
        Some(options) => hooks
            .env
            .cast::<ICoreWebView2Environment10>()
            .and_then(|env10| unsafe {
                env10.CreateCoreWebView2CompositionControllerWithOptions(hwnd, &options, &handler)
            }),
        None => unsafe {
            hooks
                .env
                .CreateCoreWebView2CompositionController(hwnd, &handler)
        },
    };
    if created.is_err() {
        // Suppress rather than navigate the portal away from its own opener.
        let _ = unsafe { args.SetHandled(true) };
        if let Some(deferral) = deferral.borrow_mut().take() {
            let _ = unsafe { deferral.Complete() };
        }
        destroy_popup(window);
    }
    true
}

/// A visual tree drawn straight into the pop-up window, filling its client area.
fn popup_visuals(
    compositor: &Compositor,
    hwnd: HWND,
) -> windows::core::Result<(
    windows::UI::Composition::Desktop::DesktopWindowTarget,
    ContainerVisual,
)> {
    use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
    let interop: ICompositorDesktopInterop = compositor.cast()?;
    let target = unsafe { interop.CreateDesktopWindowTarget(hwnd, true) }?;
    let root = compositor.CreateContainerVisual()?;
    root.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
    target.SetRoot(&root)?;
    Ok((target, root))
}

/// Settings, handlers, and bounds for a pop-up's browser, before it is handed
/// to the page as its new window. It must not navigate on its own.
fn wire_popup(
    comp: &ICoreWebView2CompositionController,
    hwnd: HWND,
    root: &ContainerVisual,
    hooks: &PageHooks,
) -> windows::core::Result<ICoreWebView2> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, PostMessageW, SetWindowTextW, WM_CLOSE,
    };
    let window = popup_key(hwnd);
    unsafe { comp.SetRootVisualTarget(root) }?;
    let controller: ICoreWebView2Controller = comp.cast()?;
    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) }?;
    unsafe {
        // Client pixels in, and the page follows the window's monitor scale.
        if let Ok(c3) = controller.cast::<ICoreWebView2Controller3>() {
            let _ = c3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS);
            let _ = c3.SetShouldDetectMonitorScaleChanges(true);
        }
        controller.SetBounds(client)?;
        controller.SetIsVisible(true)?;
    }
    let webview = unsafe { controller.CoreWebView2() }?;
    apply_page_settings(&webview);
    install_popup_policy(&webview, hooks.clone());
    install_download_policy(&webview, hooks);

    // The sign-in page closes itself once it has reported to its opener.
    let close = WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
        if popup_alive(window) {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        Ok(())
    }));
    let mut token = 0i64;
    let _ = unsafe { webview.add_WindowCloseRequested(&close, &mut token) };

    // The title says where the page is, never what it calls itself (Art. IV).
    let title = SourceChangedEventHandler::create(Box::new(move |sender, _| {
        let Some(sender) = sender else {
            return Ok(());
        };
        let mut source = windows::core::PWSTR::null();
        if unsafe { sender.Source(&mut source) }.is_ok() && popup_alive(window) {
            if let Some(url) = take_pwstr(source) {
                let text = wide(&popup_title(&url));
                let _ = unsafe { SetWindowTextW(hwnd, PCWSTR(text.as_ptr())) };
            }
        }
        Ok(())
    }));
    let mut token = 0i64;
    let _ = unsafe { webview.add_SourceChanged(&title, &mut token) };

    let _ = unsafe { controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC) };
    Ok(webview)
}

/// A composition-hosted page hears only the mouse input its window hands it.
/// `COREWEBVIEW2_MOUSE_EVENT_KIND` values are the `WM_` message numbers, and
/// the virtual-key flags are the `MK_` bits of `wParam`.
fn forward_popup_mouse(
    hwnd: HWND,
    comp: &ICoreWebView2CompositionController,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) {
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
        WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDBLCLK, WM_XBUTTONDOWN,
        WM_XBUTTONUP,
    };
    const MK_BUTTONS: i32 = 0x01 | 0x02 | 0x10 | 0x20 | 0x40;
    let window = popup_key(hwnd);
    let keys = (wparam.0 & 0xffff) as i32;
    let high = ((wparam.0 >> 16) & 0xffff) as u16;
    let mut point = POINT {
        x: (lparam.0 & 0xffff) as i16 as i32,
        y: ((lparam.0 >> 16) & 0xffff) as i16 as i32,
    };
    let mut data = 0u32;
    let set_tracking = |on: bool| {
        POPUP_SLOTS.with(|slots| {
            slots
                .borrow_mut()
                .get_mut(&window)
                .map(|slot| std::mem::replace(&mut slot.tracking, on))
                .unwrap_or(on)
        })
    };
    match msg {
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            // Wheel messages carry screen coordinates.
            let _ = unsafe { ScreenToClient(hwnd, &mut point) };
            data = high as i16 as i32 as u32;
        }
        WM_XBUTTONDOWN | WM_XBUTTONUP | WM_XBUTTONDBLCLK => data = high as u32,
        WM_MOUSELEAVE => {
            set_tracking(false);
            point = POINT::default();
        }
        WM_MOUSEMOVE if !set_tracking(true) => {
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = unsafe { TrackMouseEvent(&mut track) };
        }
        _ => {}
    }
    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
            unsafe { SetCapture(hwnd) };
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP if keys & MK_BUTTONS == 0 => {
            let _ = unsafe { ReleaseCapture() };
        }
        _ => {}
    }
    let keys = if msg == WM_MOUSELEAVE { 0 } else { keys };
    let _ = unsafe {
        comp.SendMouseInput(
            COREWEBVIEW2_MOUSE_EVENT_KIND(msg as i32),
            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys),
            data,
            point,
        )
    };
}

unsafe extern "system" fn popup_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, GetClientRect, SetCursor, SetWindowPos, HTCLIENT, SWP_NOACTIVATE,
        SWP_NOZORDER, WM_DPICHANGED, WM_MOUSEFIRST, WM_MOUSELAST, WM_MOVE, WM_NCDESTROY,
        WM_SETCURSOR, WM_SETFOCUS, WM_SIZE,
    };
    let window = popup_key(hwnd);
    match msg {
        WM_MOUSEFIRST..=WM_MOUSELAST | WM_MOUSELEAVE => {
            if let Some(comp) = popup_comp(window) {
                forward_popup_mouse(hwnd, &comp, msg, wparam, lparam);
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SETCURSOR if (lparam.0 & 0xffff) as u32 == HTCLIENT => {
            let mut cursor = HCURSOR::default();
            match popup_comp(window) {
                Some(comp)
                    if unsafe { comp.Cursor(&mut cursor) }.is_ok() && !cursor.is_invalid() =>
                {
                    unsafe { SetCursor(Some(cursor)) };
                    LRESULT(1)
                }
                _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
        }
        WM_SIZE => {
            if let Some(controller) = popup_controller(window) {
                let mut client = RECT::default();
                if unsafe { GetClientRect(hwnd, &mut client) }.is_ok() {
                    let _ = unsafe { controller.SetBounds(client) };
                }
            }
            LRESULT(0)
        }
        WM_MOVE => {
            if let Some(controller) = popup_controller(window) {
                let _ = unsafe { controller.NotifyParentWindowPositionChanged() };
            }
            LRESULT(0)
        }
        WM_SETFOCUS => {
            if let Some(controller) = popup_controller(window) {
                let _ =
                    unsafe { controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC) };
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let suggested = unsafe { &*(lparam.0 as *const RECT) };
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let slot = POPUP_SLOTS.with(|slots| slots.borrow_mut().remove(&window));
            if let Some(slot) = slot {
                if let Some(controller) = slot
                    .comp
                    .and_then(|comp| comp.cast::<ICoreWebView2Controller>().ok())
                {
                    let _ = unsafe { controller.Close() };
                }
                let _ = slot.root.SetIsVisible(false);
                drop(slot.target);
                if let Some(book) = slot.book.upgrade() {
                    if let Ok(mut book) = book.try_borrow_mut() {
                        book.closed(window);
                    }
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn runtime_installed() -> bool {
    let mut version = windows::core::PWSTR::null();
    let ok = unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) }
        .is_ok()
        && !version.is_null();
    if !version.is_null() {
        unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(version.0 as *const _)) };
    }
    ok
}

fn create_d3d_device() -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    Ok((device.unwrap(), context.unwrap()))
}

fn create_staging(device: &ID3D11Device, w: u32, h: u32) -> windows::core::Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut tex = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut tex))? };
    Ok(tex.unwrap())
}

/// The captured surface is BGRA with a row pitch that is not the row width.
fn bgra_to_color_image(mapped: &D3D11_MAPPED_SUBRESOURCE, w: usize, h: usize) -> egui::ColorImage {
    let mut pixels = Vec::with_capacity(w * h);
    let pitch = mapped.RowPitch as usize;
    let base = mapped.pData as *const u8;
    for y in 0..h {
        let row = unsafe { std::slice::from_raw_parts(base.add(y * pitch), w * 4) };
        pixels.extend(
            row.as_chunks::<4>()
                .0
                .iter()
                .map(|p| egui::Color32::from_rgba_premultiplied(p[2], p[1], p[0], p[3])),
        );
    }
    egui::ColorImage {
        size: [w, h],
        pixels,
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// egui keys the page is allowed to hear. Printable characters arrive
/// separately as text, so this is the navigation and editing set.
fn virtual_key(key: egui::Key) -> Option<u32> {
    use egui::Key::*;
    Some(match key {
        ArrowDown => 0x28,
        ArrowLeft => 0x25,
        ArrowRight => 0x27,
        ArrowUp => 0x26,
        Backspace => 0x08,
        Delete => 0x2E,
        End => 0x23,
        Enter => 0x0D,
        Home => 0x24,
        Insert => 0x2D,
        PageDown => 0x22,
        PageUp => 0x21,
        Tab => 0x09,
        Space => 0x20,
        F5 => 0x74,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Test scaffolding
// ---------------------------------------------------------------------------
//
// The live tests need what the app normally supplies: a window to parent the
// browser to, and a message loop to deliver WebView2's async callbacks. Both
// live here rather than in a test module so the board-level integration test
// can use them too.

#[cfg(test)]
pub(crate) mod probe {
    use super::*;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW, RegisterClassW,
        TranslateMessage, CW_USEDEFAULT, MSG, PM_REMOVE, WINDOW_EX_STYLE, WNDCLASSW,
        WS_OVERLAPPEDWINDOW,
    };

    /// Drain the thread's message queue. WebView2 completions arrive here.
    pub(crate) fn pump() {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe extern "system" fn probe_proc(
        hwnd: HWND,
        msg: u32,
        wparam: windows::Win32::Foundation::WPARAM,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    pub(crate) fn window() -> HWND {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let instance = GetModuleHandleW(None).unwrap();
            let class = wide("SlateWebHostProbe");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(probe_proc),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                PCWSTR(class.as_ptr()),
                PCWSTR(wide("probe").as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                640,
                480,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap()
        }
    }

    /// A host parented to a throwaway window, with its own profile folder.
    pub(crate) fn host(tag: &str) -> Option<Webview2Host> {
        let dir = std::env::temp_dir().join(format!("slate-web-probe-{tag}"));
        std::fs::create_dir_all(&dir).ok()?;
        Webview2Host::new(window(), &dir.join("udf"), egui::Context::default())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pointer_tracks_physical_capture_resolution() {
        let point = super::capture_point(320.0, 180.0, 3.0);
        assert_eq!((point.x, point.y), (960, 540));
        let point = super::capture_point(320.0, 180.0, 6.0);
        assert_eq!((point.x, point.y), (1920, 1080));
    }
    use super::probe::{host, pump};
    use super::*;
    use slate_doc::scene::WebSourceKind;
    use std::time::{Duration, Instant};

    #[test]
    fn a_windows_path_becomes_a_file_uri_and_a_url_is_left_alone() {
        assert_eq!(
            navigate_uri(r"C:\dash boards\index.html"),
            "file:///C:/dash%20boards/index.html"
        );
        assert_eq!(
            navigate_uri("https://example.com/a?b=1"),
            "https://example.com/a?b=1"
        );
        assert_eq!(
            navigate_uri("file:///C:/x.html"),
            "file:///C:/x.html",
            "an already-formed file URI is not re-encoded"
        );
    }

    /// Pull frames until one satisfies `want`, pumping the message loop.
    fn run_until(
        host: &mut Webview2Host,
        id: NodeId,
        req: &WebRequest,
        secs: u64,
        want: impl Fn(&egui::ColorImage) -> bool,
    ) -> (usize, bool) {
        let deadline = Instant::now() + Duration::from_secs(secs);
        let mut frames = 0usize;
        let mut hit = false;
        while Instant::now() < deadline && !hit {
            pump();
            // `admit` is idempotent, and re-calling it is how the environment's
            // late arrival gets picked up — exactly what the pump does.
            host.admit(id, req);
            if let Some(img) = host.take_frame(id) {
                frames += 1;
                hit = want(&img);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        (frames, hit)
    }

    /// The one test that proves the pipeline end to end: a real browser, a real
    /// composition visual, a real capture session, real pixels. It needs a
    /// desktop session and the Evergreen runtime, so it is `#[ignore]`d and run
    /// deliberately:
    ///
    /// ```powershell
    /// cargo test -p slate --lib board_web_win -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn a_local_page_paints_into_a_texture() {
        let dir = std::env::temp_dir().join("slate-web-probe-local");
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("probe.html");
        std::fs::write(
            &page,
            "<html><body style=\"margin:0;background:#ff0000\"></body></html>",
        )
        .unwrap();

        let mut host =
            host("local").expect("no WebView2 runtime, GPU device, or compositor on this machine");
        let id = NodeId(1);
        let req = WebRequest {
            // Deliberately a bare Windows path, which is what the board hands
            // the host: turning it into a URI is the host's job.
            target: page.to_string_lossy().into_owned(),
            kind: WebSourceKind::LocalFile,
            width_css: 320,
            height_css: 200,
            raster_w: 320,
            raster_h: 200,
            link_script: String::new(),
            anchor_px: None,
            profile: "local".into(),
        };
        let (frames, red) = run_until(&mut host, id, &req, 45, |img| {
            img.pixels
                .iter()
                .any(|p| p.r() > 200 && p.g() < 80 && p.b() < 80)
        });
        println!(
            "frames: {frames}, red: {red}, error: {:?}",
            host.load_error(id)
        );
        assert!(frames > 0, "the capture session never produced a frame");
        assert!(red, "frames arrived but the page never painted");
    }

    /// Browser parity: an arbitrary public page loads and paints. Needs the
    /// network as well as a desktop session.
    #[test]
    #[ignore]
    fn an_arbitrary_public_page_loads_and_paints() {
        let mut host = host("remote").expect("no WebView2 runtime on this machine");
        let id = NodeId(2);
        let req = WebRequest {
            target: "https://example.com/".into(),
            kind: WebSourceKind::Remote,
            width_css: 1024,
            height_css: 700,
            raster_w: 1024,
            raster_h: 700,
            link_script: String::new(),
            anchor_px: None,
            profile: slate_doc::scene::web_profile_name("https://example.com/"),
        };
        // Any page that renders text puts dark pixels on a light background;
        // an unpainted capture is uniformly transparent.
        let (frames, painted) = run_until(&mut host, id, &req, 60, |img| {
            img.pixels.iter().any(|p| p.a() > 200 && p.r() < 120)
        });
        println!(
            "frames: {frames}, painted: {painted}, error: {:?}",
            host.load_error(id)
        );
        assert!(frames > 0, "the capture session never produced a frame");
        assert!(painted, "example.com never rendered any content");
    }

    /// Eviction releases the browser and the capture session, which is what
    /// keeps a board of a hundred pages to a bounded number of processes.
    #[test]
    #[ignore]
    fn eviction_releases_the_view() {
        let mut host = host("evict").expect("no WebView2 runtime on this machine");
        let id = NodeId(3);
        let req = WebRequest {
            target: "about:blank".into(),
            kind: WebSourceKind::Remote,
            width_css: 200,
            height_css: 120,
            raster_w: 200,
            raster_h: 120,
            link_script: String::new(),
            anchor_px: None,
            profile: "local".into(),
        };
        let (frames, _) = run_until(&mut host, id, &req, 30, |_| true);
        assert!(frames > 0);
        host.evict(id);
        pump();
        assert!(host.views.is_empty(), "the view outlived its slot");
        assert!(host.take_frame(id).is_none());
    }

    /// Run a fixed expression in the portal's page and wait for its JSON.
    fn eval(host: &Webview2Host, id: NodeId, script: &str, secs: u64) -> Option<String> {
        let webview = host.webview(id)?;
        let out: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let sink = out.clone();
        let handler = ExecuteScriptCompletedHandler::create(Box::new(move |_, json| {
            *sink.borrow_mut() = Some(json);
            Ok(())
        }));
        let script = wide(script);
        unsafe { webview.ExecuteScript(PCWSTR(script.as_ptr()), &handler) }.ok()?;
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline && out.borrow().is_none() {
            pump();
            std::thread::sleep(Duration::from_millis(20));
        }
        let json = out.borrow_mut().take();
        json
    }

    fn pump_until(
        host: &mut Webview2Host,
        id: NodeId,
        req: &WebRequest,
        secs: u64,
        done: impl Fn(&Webview2Host) -> bool,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            pump();
            host.admit(id, req);
            if done(host) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn local_request(page: &std::path::Path) -> WebRequest {
        WebRequest {
            target: page.to_string_lossy().into_owned(),
            kind: WebSourceKind::LocalFile,
            width_css: 400,
            height_css: 300,
            raster_w: 400,
            raster_h: 300,
            link_script: String::new(),
            anchor_px: None,
            profile: "local".into(),
        }
    }

    /// The sign-in shape end to end: a sized `window.open` gets its own owned
    /// window, the pop-up reaches its opener, and `window.close()` closes the
    /// window while the portal stays on its page.
    ///
    /// ```powershell
    /// cargo test -p slate --lib popup -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore]
    fn a_sign_in_popup_keeps_its_opener_and_closes_itself() {
        let dir = std::env::temp_dir().join("slate-web-probe-popup");
        std::fs::create_dir_all(&dir).unwrap();
        let opener = dir.join("opener.html");
        std::fs::write(
            &opener,
            "<html><body><script>\
             addEventListener('message', e => { document.title = e.data; });\
             window.open('signin.html', 'signin', 'width=420,height=360');\
             </script></body></html>",
        )
        .unwrap();
        std::fs::write(
            dir.join("signin.html"),
            "<html><body><script>\
             window.opener.postMessage('signed-in', '*');\
             setTimeout(() => window.close(), 300);\
             </script></body></html>",
        )
        .unwrap();

        let mut host = host("popup").expect("no WebView2 runtime on this machine");
        let id = NodeId(7);
        let req = local_request(&opener);
        let opened = pump_until(&mut host, id, &req, 45, |h| h.holds_popup(id));
        assert!(opened, "the sized window.open never became an owned window");
        let closed = pump_until(&mut host, id, &req, 30, |h| !h.holds_popup(id));
        assert!(
            closed,
            "window.close() in the pop-up did not close its window"
        );
        let title = eval(&host, id, "document.title", 10);
        println!("opener title: {title:?}, url: {:?}", host.current_url(id));
        assert_eq!(
            title.as_deref(),
            Some("\"signed-in\""),
            "the opener never heard back"
        );
        assert!(
            host.current_url(id)
                .is_some_and(|u| u.ends_with("opener.html")),
            "the portal was navigated away from the opener"
        );
    }

    /// A bare `window.open` (no size, no position) keeps the old behaviour: the
    /// portal navigates in place and no window appears.
    #[test]
    #[ignore]
    fn a_featureless_window_open_navigates_the_portal_in_place() {
        let dir = std::env::temp_dir().join("slate-web-probe-blank");
        std::fs::create_dir_all(&dir).unwrap();
        let start = dir.join("start.html");
        std::fs::write(
            &start,
            "<html><body><script>window.open('next.html');</script></body></html>",
        )
        .unwrap();
        std::fs::write(dir.join("next.html"), "<html><body>next</body></html>").unwrap();

        let mut host = host("blank").expect("no WebView2 runtime on this machine");
        let id = NodeId(8);
        let req = local_request(&start);
        let moved = pump_until(&mut host, id, &req, 45, |h| {
            h.current_url(id).is_some_and(|u| u.ends_with("next.html"))
        });
        println!("url: {:?}", host.current_url(id));
        assert!(moved, "the portal did not follow the link in place");
        assert!(!host.holds_popup(id), "a link must not open a window");
    }

    /// Evicting a portal closes the sign-in window its page opened.
    #[test]
    #[ignore]
    fn evicting_the_portal_closes_its_popup() {
        let dir = std::env::temp_dir().join("slate-web-probe-popup-evict");
        std::fs::create_dir_all(&dir).unwrap();
        let opener = dir.join("opener.html");
        std::fs::write(
            &opener,
            "<html><body><script>window.open('about:blank', 'x', 'width=300,height=200');</script></body></html>",
        )
        .unwrap();
        let mut host = host("popup-evict").expect("no WebView2 runtime on this machine");
        let id = NodeId(9);
        let req = local_request(&opener);
        assert!(pump_until(&mut host, id, &req, 45, |h| h.holds_popup(id)));
        host.evict(id);
        pump();
        assert!(!host.holds_popup(id));
        assert!(
            POPUP_SLOTS.with(|s| s.borrow().is_empty()),
            "the pop-up window outlived its portal"
        );
    }

    /// What Google's sign-in page says to WebView2, without entering anything.
    /// Prints the user agent and the page's visible text; asserts nothing
    /// about Google's policy, which can change.
    #[test]
    #[ignore]
    fn google_sign_in_page_in_webview2() {
        let mut host = host("google").expect("no WebView2 runtime on this machine");
        let id = NodeId(10);
        let req = WebRequest {
            target: "https://accounts.google.com/".into(),
            kind: WebSourceKind::Remote,
            width_css: 1024,
            height_css: 768,
            raster_w: 1024,
            raster_h: 768,
            link_script: String::new(),
            anchor_px: None,
            profile: slate_doc::scene::web_profile_name("https://accounts.google.com/"),
        };
        let loaded = pump_until(&mut host, id, &req, 60, |h| {
            h.current_url(id)
                .is_some_and(|u| u.contains("accounts.google.com/") && u.len() > 30)
        });
        // Let the sign-in form render after the redirect.
        let settle = Instant::now() + Duration::from_secs(5);
        while Instant::now() < settle {
            pump();
            host.admit(id, &req);
            std::thread::sleep(Duration::from_millis(50));
        }
        println!("loaded: {loaded}, url: {:?}", host.current_url(id));
        println!("error: {:?}", host.load_error(id));
        println!(
            "userAgent: {:?}",
            eval(&host, id, "navigator.userAgent", 10)
        );
        println!(
            "webdriver: {:?}",
            eval(&host, id, "String(navigator.webdriver)", 10)
        );
        println!(
            "text: {:?}",
            eval(
                &host,
                id,
                "document.body ? document.body.innerText.slice(0, 800) : ''",
                10
            )
        );
    }
}

fn map_cursor(cursor: HCURSOR) -> Option<egui::CursorIcon> {
    if cursor.is_invalid() {
        return None;
    }
    let known = [
        (IDC_ARROW, egui::CursorIcon::Default),
        (IDC_HAND, egui::CursorIcon::PointingHand),
        (IDC_IBEAM, egui::CursorIcon::Text),
        (IDC_WAIT, egui::CursorIcon::Wait),
        (IDC_SIZEALL, egui::CursorIcon::Move),
        (IDC_SIZENS, egui::CursorIcon::ResizeVertical),
        (IDC_SIZEWE, egui::CursorIcon::ResizeHorizontal),
    ];
    for (id, icon) in known {
        if let Ok(h) = unsafe { LoadCursorW(None, id) } {
            if h.0 == cursor.0 {
                return Some(icon);
            }
        }
    }
    None
}
