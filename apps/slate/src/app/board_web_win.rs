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
//! Everything here is derived state. Nothing in this file touches the journal,
//! and the page has no channel back into Slate (Art. VII.4).

use std::cell::RefCell;
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
use windows::Win32::Foundation::{HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
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

use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, GetAvailableCoreWebView2BrowserVersionString,
    ICoreWebView2, ICoreWebView2CompositionController, ICoreWebView2Controller,
    ICoreWebView2Controller2, ICoreWebView2Controller3, ICoreWebView2Environment,
    ICoreWebView2Environment3, ICoreWebView2_4, COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS,
    COREWEBVIEW2_COLOR, COREWEBVIEW2_MOUSE_EVENT_KIND,
    COREWEBVIEW2_MOUSE_EVENT_KIND_HORIZONTAL_WHEEL, COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE,
    COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN, COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
    COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_DOWN,
    COREWEBVIEW2_MOUSE_EVENT_KIND_MIDDLE_BUTTON_UP, COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
    COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN, COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
    COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_LEFT_BUTTON,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_MIDDLE_BUTTON,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_RIGHT_BUTTON,
};
use webview2_com::{
    CallDevToolsProtocolMethodCompletedHandler,
    CreateCoreWebView2CompositionControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, DownloadStartingEventHandler,
    NavigationCompletedEventHandler, NewWindowRequestedEventHandler,
};

use super::board_web::{WebHost, WebInput, WebRequest};

/// Wide, NUL-terminated, kept alive for the duration of the call.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
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
}

struct View {
    /// Captured root. WebView2 draws into a child of this visual.
    root: ContainerVisual,
    child: ContainerVisual,
    item: Option<GraphicsCaptureItem>,
    pool: Option<Direct3D11CaptureFramePool>,
    session: Option<GraphicsCaptureSession>,
    staging: Option<ID3D11Texture2D>,
    size: (u32, u32),
    target: String,
    /// The most recent readback, kept so a demoted portal still has a poster.
    last: Option<egui::ColorImage>,
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
    deferred: Vec<(NodeId, WebRequest)>,
}

impl Webview2Host {
    /// `None` when there is no Evergreen runtime, no GPU device, or no
    /// composition support — the caller keeps the null host and every portal
    /// reports `NoRuntime` rather than stalling (D29).
    pub fn new(parent: HWND, user_data: &std::path::Path) -> Option<Self> {
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
            deferred: Vec::new(),
        })
    }

    /// Build the visual tree, the browser, and the capture session for one
    /// portal. Everything after the controller callback is asynchronous.
    fn create_view(&mut self, id: NodeId, req: &WebRequest) -> windows::core::Result<()> {
        let env = match self.env.borrow().clone() {
            Some(env) => env,
            None => {
                self.deferred.push((id, req.clone()));
                return Ok(());
            }
        };

        let (w, h) = (req.width_css.max(1), req.height_css.max(1));
        let root = self.compositor.CreateContainerVisual()?;
        root.SetSize(Vector2 {
            X: w as f32,
            Y: h as f32,
        })?;
        root.SetIsVisible(true)?;
        let child = self.compositor.CreateContainerVisual()?;
        child.SetRelativeSizeAdjustment(Vector2 { X: 1.0, Y: 1.0 })?;
        root.Children()?.InsertAtTop(&child)?;

        let shared: Rc<RefCell<Pending>> = Rc::new(RefCell::new(Pending::default()));
        let sink = shared.clone();
        let visual = child.clone();
        let target = req.target.clone();
        let bounds = RECT {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
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
                if let Err(e) = attach(&comp, &visual, bounds, &target, &sink) {
                    sink.borrow_mut().error = Some(format!("WebView2 could not start: {e}"));
                }
                Ok(())
            },
        ));
        unsafe { env.CreateCoreWebView2CompositionController(self.parent, &handler) }?;

        self.views.insert(
            id,
            View {
                root,
                child,
                item: None,
                pool: None,
                session: None,
                staging: None,
                size: (w, h),
                target: req.target.clone(),
                last: None,
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

    fn resize(&mut self, id: NodeId, w: u32, h: u32) {
        let Some(view) = self.views.get_mut(&id) else {
            return;
        };
        if view.size == (w, h) {
            return;
        }
        view.size = (w, h);
        view.staging = None;
        let _ = view.root.SetSize(Vector2 {
            X: w as f32,
            Y: h as f32,
        });
        if let Some(controller) = view.shared.borrow().controller.clone() {
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

    /// Copy the newest captured frame into CPU memory. This is the expensive
    /// step, which is why the pool is small and the idle rate is low.
    fn read_frame(&mut self, id: NodeId) -> Option<egui::ColorImage> {
        let (pool, size) = {
            let view = self.views.get(&id)?;
            (view.pool.clone()?, view.size)
        };
        let frame = pool.TryGetNextFrame().ok()?;
        let surface = frame.Surface().ok()?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast().ok()?;
        let source: ID3D11Texture2D = unsafe { access.GetInterface() }.ok()?;

        let staging = match self.views.get(&id).and_then(|v| v.staging.clone()) {
            Some(s) => s,
            None => {
                let s = create_staging(&self.device, size.0, size.1).ok()?;
                if let Some(v) = self.views.get_mut(&id) {
                    v.staging = Some(s.clone());
                }
                s
            }
        };

        let image = unsafe {
            self.context.CopyResource(&staging, &source);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .ok()?;
            let img = bgra_to_color_image(&mapped, size.0 as usize, size.1 as usize);
            self.context.Unmap(&staging, 0);
            img
        };
        let _ = frame.Close();
        if let Some(v) = self.views.get_mut(&id) {
            v.last = Some(image.clone());
        }
        Some(image)
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
}

impl WebHost for Webview2Host {
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
        match self.views.get(&id).map(|v| v.target.clone()) {
            Some(target) if target == req.target => {
                self.resize(id, req.width_css.max(1), req.height_css.max(1));
            }
            Some(_) => {
                self.evict(id);
                let _ = self.create_view(id, req);
            }
            None => {
                let _ = self.create_view(id, req);
            }
        }
        self.start_capture(id);
    }

    fn evict(&mut self, id: NodeId) {
        let Some(view) = self.views.remove(&id) else {
            return;
        };
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

    fn take_frame(&mut self, id: NodeId) -> Option<egui::ColorImage> {
        self.start_capture(id);
        self.read_frame(id)
    }

    fn capture_poster(&mut self, id: NodeId) -> Option<egui::ColorImage> {
        self.read_frame(id)
            .or_else(|| self.views.get(&id).and_then(|v| v.last.clone()))
    }

    fn send_input(&mut self, id: NodeId, input: WebInput) {
        let Some(comp) = self
            .views
            .get(&id)
            .and_then(|v| v.shared.borrow().comp.clone())
        else {
            return;
        };
        let point = |x: f32, y: f32| POINT {
            x: x.round() as i32,
            y: y.round() as i32,
        };
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
fn attach(
    comp: &ICoreWebView2CompositionController,
    visual: &ContainerVisual,
    bounds: RECT,
    target: &str,
    sink: &Rc<RefCell<Pending>>,
) -> windows::core::Result<()> {
    unsafe { comp.SetRootVisualTarget(visual) }?;
    let controller: ICoreWebView2Controller = comp.cast()?;
    unsafe {
        // Raw pixels and a fixed rasterization scale: the board's camera is
        // what scales a portal, so the page must not also react to monitor DPI.
        if let Ok(c3) = controller.cast::<ICoreWebView2Controller3>() {
            let _ = c3.SetBoundsMode(COREWEBVIEW2_BOUNDS_MODE_USE_RAW_PIXELS);
            let _ = c3.SetShouldDetectMonitorScaleChanges(false);
            let _ = c3.SetRasterizationScale(1.0);
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
    let webview = unsafe { controller.CoreWebView2() }?;
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

    let errors = sink.clone();
    let nav = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
        if let Some(args) = args {
            let mut ok = windows::core::BOOL(0);
            let _ = unsafe { args.IsSuccess(&mut ok) };
            let mut pending = errors.borrow_mut();
            pending.error = if ok.as_bool() {
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
        }
        Ok(())
    }));
    let mut token = 0i64;
    let _ = unsafe { webview.add_NavigationCompleted(&nav, &mut token) };

    // Popups and downloads are not a browser chrome feature we ship (D15, D32).
    // Handled=true with no NewWindow, and Cancel=true on download, so the page
    // cannot open a window or write a file through this host.
    let popup = NewWindowRequestedEventHandler::create(Box::new(move |_sender, args| {
        if let Some(args) = args {
            let _ = unsafe { args.SetHandled(true) };
        }
        Ok(())
    }));
    let mut popup_token = 0i64;
    let _ = unsafe { webview.add_NewWindowRequested(&popup, &mut popup_token) };
    if let Ok(wv4) = webview.cast::<ICoreWebView2_4>() {
        let download = DownloadStartingEventHandler::create(Box::new(move |_sender, args| {
            if let Some(args) = args {
                let _ = unsafe { args.SetCancel(true) };
                let _ = unsafe { args.SetHandled(true) };
            }
            Ok(())
        }));
        let mut download_token = 0i64;
        let _ = unsafe { wv4.add_DownloadStarting(&download, &mut download_token) };
    }

    let url = wide(&navigate_uri(target));
    unsafe { webview.Navigate(PCWSTR(url.as_ptr())) }?;

    let mut pending = sink.borrow_mut();
    pending.comp = Some(comp.clone());
    pending.controller = Some(controller);
    pending.webview = Some(webview);
    pending.attached = true;
    Ok(())
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
        for x in 0..w {
            let p = &row[x * 4..x * 4 + 4];
            pixels.push(egui::Color32::from_rgba_premultiplied(
                p[2], p[1], p[0], p[3],
            ));
        }
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
        Webview2Host::new(window(), &dir.join("udf"))
    }
}

#[cfg(test)]
mod tests {
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
        };
        let (frames, _) = run_until(&mut host, id, &req, 30, |_| true);
        assert!(frames > 0);
        host.evict(id);
        pump();
        assert!(host.views.is_empty(), "the view outlived its slot");
        assert!(host.take_frame(id).is_none());
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
