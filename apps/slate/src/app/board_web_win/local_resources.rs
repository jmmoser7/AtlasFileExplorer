//! Chromium cannot open long Windows file URLs. Supply only the bound document
//! and its local display resources, keeping their original file URLs and origin.
use super::*;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::WebResourceRequestedEventHandler;
use windows::core::{w, BOOL, PWSTR};

mod files;
#[cfg(test)]
mod tests;

const REQUEST_LIMIT: usize = 128;
type Opened = (File, u64);
struct Job {
    id: u64,
    path: PathBuf,
}

struct Request {
    id: u64,
    epoch: u64,
    mime: &'static str,
    args: ICoreWebView2WebResourceRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
}
impl Drop for Request {
    fn drop(&mut self) {
        let _ = unsafe { self.deferral.Complete() };
    }
}

pub(super) struct LocalResources {
    page: url::Url,
    active: Cell<bool>,
    epoch: Cell<u64>,
    next: Cell<u64>,
    requests: RefCell<Vec<Request>>,
    open: SyncSender<Job>,
    ready: Receiver<(u64, Option<Opened>)>,
    env: ICoreWebView2Environment,
}

impl LocalResources {
    pub(super) fn install(
        web: &ICoreWebView2,
        env: &ICoreWebView2Environment3,
        authored: &str,
        wake: &egui::Context,
    ) -> Option<Rc<Self>> {
        // Web requests carry a separate authored locator when resuming an
        // in-page navigation. A visited or remote URL cannot establish scope.
        let page = url::Url::from_file_path(authored).ok()?;
        let file = page.to_file_path().ok()?;
        if !files::long_disk_path(&file) {
            return None;
        }
        let directory = file.parent()?.to_owned();
        let (open, jobs) = mpsc::sync_channel::<Job>(REQUEST_LIMIT);
        let (completed, ready) = mpsc::channel();
        let wake = wake.clone();
        std::thread::Builder::new()
            .name("slate-local-web".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = files::open(&directory, &job.path);
                    if completed.send((job.id, result)).is_err() {
                        break;
                    }
                    wake.request_repaint();
                }
            })
            .ok()?;
        let this = Rc::new(Self {
            page,
            active: Cell::new(false),
            epoch: Cell::new(0),
            next: Cell::new(0),
            requests: RefCell::default(),
            open,
            ready,
            env: env.cast().ok()?,
        });
        let weak = Rc::downgrade(&this);
        let started = NavigationStartingEventHandler::create(Box::new(move |_, args| {
            if let Some(scope) = weak.upgrade() {
                let mut uri = PWSTR::null();
                let url = args
                    .and_then(|args| unsafe { args.Uri(&mut uri) }.ok())
                    .and_then(|()| take_pwstr(uri))
                    .and_then(|u| resource_url(&u));
                scope.active.set(url.as_ref() == Some(&scope.page));
                scope.epoch.set(scope.epoch.get().wrapping_add(1));
            }
            Ok(())
        }));
        let weak = Rc::downgrade(&this);
        let changed = SourceChangedEventHandler::create(Box::new(move |sender, _| {
            if let (Some(scope), Some(sender)) = (weak.upgrade(), sender) {
                let mut uri = PWSTR::null();
                let url = unsafe { sender.Source(&mut uri) }
                    .ok()
                    .and_then(|()| take_pwstr(uri))
                    .and_then(|u| resource_url(&u));
                if url.as_ref() != Some(&scope.page) && scope.active.replace(false) {
                    scope.epoch.set(scope.epoch.get().wrapping_add(1));
                }
            }
            Ok(())
        }));
        let mut token = 0;
        let weak = Rc::downgrade(&this);
        let requested = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            if let (Some(scope), Some(args)) = (weak.upgrade(), args) {
                scope.request(args)?;
            }
            Ok(())
        }));
        unsafe {
            web.add_NavigationStarting(&started, &mut token).ok()?;
            web.add_SourceChanged(&changed, &mut token).ok()?;
            web.AddWebResourceRequestedFilter(
                w!("file:///*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
            )
            .ok()?;
            web.add_WebResourceRequested(&requested, &mut token).ok()?;
        }
        Some(this)
    }

    #[cfg(test)]
    pub(super) fn is_active(&self) -> bool {
        self.active.get()
    }

    fn request(
        &self,
        args: ICoreWebView2WebResourceRequestedEventArgs,
    ) -> windows::core::Result<()> {
        if !self.active.get() || self.requests.borrow().len() >= REQUEST_LIMIT {
            return Ok(());
        }
        let request = unsafe { args.Request()? };
        let mut method = PWSTR::null();
        unsafe { request.Method(&mut method)? };
        if take_pwstr(method).as_deref() != Some("GET") {
            return Ok(());
        }
        let mut ranged = BOOL::default();
        unsafe { request.Headers()?.Contains(w!("Range"), &mut ranged)? };
        if ranged.as_bool() {
            return Ok(());
        }
        let mut uri = PWSTR::null();
        let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
        unsafe {
            request.Uri(&mut uri)?;
            args.ResourceContext(&mut context)?;
        }
        let Some(url) = take_pwstr(uri).and_then(|uri| resource_url(&uri)) else {
            return Ok(());
        };
        let Some((path, mime)) = eligible(&self.page, &url, context) else {
            return Ok(());
        };
        let id = self.next.get().wrapping_add(1);
        let deferral = unsafe { args.GetDeferral()? };
        let pending = Request {
            id,
            epoch: self.epoch.get(),
            mime,
            args,
            deferral,
        };
        if self.open.try_send(Job { id, path }).is_ok() {
            self.next.set(id);
            self.requests.borrow_mut().push(pending);
        }
        Ok(())
    }

    pub(super) fn drain(&self) {
        while let Ok((id, opened)) = self.ready.try_recv() {
            let req = {
                let mut requests = self.requests.borrow_mut();
                requests
                    .iter()
                    .position(|r| r.id == id)
                    .map(|i| requests.swap_remove(i))
            };
            if let (Some(req), Some((file, size))) = (req, opened) {
                if self.active.get() && req.epoch == self.epoch.get() {
                    let stream = super::local_stream::from_file(file, size);
                    let headers = wide(&format!("Content-Type: {}\r\nContent-Length: {size}\r\nX-Content-Type-Options: nosniff",req.mime));
                    unsafe {
                        if let Ok(response) = self.env.CreateWebResourceResponse(
                            &stream,
                            200,
                            w!("OK"),
                            PCWSTR(headers.as_ptr()),
                        ) {
                            let _ = req.args.SetResponse(&response);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn cancel(&self) {
        self.active.set(false);
        self.epoch.set(self.epoch.get().wrapping_add(1));
        let requests = std::mem::take(&mut *self.requests.borrow_mut());
        drop(requests);
    }
}

// Keep a fragment in the authored document, and its asset cache-busting queries. Exclude encoded
// path delimiters. File URLs served here may only address a local disk drive.
fn resource_url(raw: &str) -> Option<url::Url> {
    let mut url = url::Url::parse(raw).ok()?;
    if url.scheme() != "file" || url.host_str().is_some() {
        return None;
    }
    let path = url.path().to_ascii_lowercase();
    if ["%2f", "%5c", "%00", "%3a"]
        .iter()
        .any(|escape| path.contains(escape))
    {
        return None;
    }
    url.set_fragment(None);
    Some(url)
}

fn eligible(
    page: &url::Url,
    url: &url::Url,
    context: COREWEBVIEW2_WEB_RESOURCE_CONTEXT,
) -> Option<(PathBuf, &'static str)> {
    let path = url.to_file_path().ok()?;
    let authored = page.to_file_path().ok()?;
    let relative = path.strip_prefix(authored.parent()?).ok()?;
    if relative
        .components()
        .any(|p| !matches!(p,Component::Normal(name) if !name.to_string_lossy().contains(':')))
    {
        return None;
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match (context, ext.as_str()) {
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT, "html" | "htm") if url == page => "text/html",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET, "css") => "text/css",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT, "js" | "mjs") => "text/javascript",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE, "png") => "image/png",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE, "jpg" | "jpeg") => "image/jpeg",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE, "gif") => "image/gif",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE, "webp") => "image/webp",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE, "avif") => "image/avif",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE, "svg") => "image/svg+xml",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE, "ico") => "image/x-icon",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT, "woff") => "font/woff",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT, "woff2") => "font/woff2",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT, "ttf") => "font/ttf",
        (COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT, "otf") => "font/otf",
        _ => return None,
    };
    Some((path, mime))
}
