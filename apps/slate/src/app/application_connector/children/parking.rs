//! Keep the native guard until the source UI acknowledges its managed hide.
use super::*;
use crate::app::application_connector::native::BindingKey;

impl ChildSurface {
    pub(super) fn park_until_ack(
        &mut self,
        tx: &Sender<worker::Action>,
        key: &BindingKey,
        surface: &str,
    ) {
        let Some(native) = self.native.as_ref() else {
            return;
        };
        let _ = native.park();
        if self.worker.as_deref() != Some(key.worker.as_str()) {
            return;
        }
        if self.parking_key.as_ref() != Some(key) {
            self.parking_key = Some(key.clone());
            self.host.begin_release();
        }
        // This existing, advertised command hides the WinForms editor on its
        // own UI thread. A raw OS hide alone does not acknowledge managed state.
        let Some(command) = self.hide_command.clone() else {
            self.status =
                "Application window remains safely parked; managed hide unavailable".into();
            return;
        };
        self.host.retry_release(|request| {
            tx.try_send(worker::Action::FocusChild {
                key: key.clone(),
                surface: surface.into(),
                command: command.clone(),
                request,
            })
            .is_ok()
        });
    }

    pub(super) fn complete_focus(
        &mut self,
        key: &BindingKey,
        current: Option<&BindingKey>,
        request: hosting::FocusRequest,
        result: Result<(), String>,
        has_intent: bool,
    ) {
        if current != Some(key) || self.worker.as_deref() != Some(key.worker.as_str()) {
            return;
        }
        let Some(release) = self
            .host
            .complete_for_release(request, result.is_ok(), has_intent)
        else {
            return;
        };
        if let Err(error) = result {
            self.status = format!("Application window focus: {error}");
        } else if release && self.parking_key.as_ref() == Some(key) {
            self.release_hidden();
        } else if request.focused {
            self.status = "Native application window".into();
        }
    }

    fn release_hidden(&mut self) -> bool {
        if let Some(mut native) = self.native.take() {
            if matches!(
                native.handle(),
                Err(
                    atlas_connector_runtime::surface::SurfaceError::InvalidWindow
                        | atlas_connector_runtime::surface::SurfaceError::IdentityChanged
                )
            ) {
                // A destroyed/reused HWND cannot be shown by Drop; the shared
                // guard only releases its own stale host property in this case.
                self.parking_key = None;
                self.host.parked();
                return true;
            }
            if let Err(error) = native.detach_hidden() {
                let _ = native.park();
                self.native = Some(native);
                self.status = format!("Application window parking: {error}");
                return false;
            }
        }
        self.parking_key = None;
        self.host.begin_intent();
        self.host.parked();
        self.status = "Native application window parked · Double-click to enter".into();
        true
    }

    /// A destroyed/replaced source cannot acknowledge a previous scope. Restore
    /// hidden locally, invalidate its epochs, and retain a guard on any failure.
    pub(super) fn close_scope(&mut self) -> bool {
        if let Some(native) = &self.native {
            let _ = native.park();
        }
        self.host.begin_intent();
        self.host.parked();
        self.release_hidden()
    }
}

#[cfg(all(test, windows))]
#[path = "parking_tests.rs"]
mod tests;
