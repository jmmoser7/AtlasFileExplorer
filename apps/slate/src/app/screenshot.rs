//! One resolved configuration for the existing production screenshot harness.
//! Recovery evidence stays in a private OS-temp subtree, never with PNG artifacts.
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use eframe::egui;

pub(super) struct Capture {
    path: PathBuf,
    delay: u64,
    recovery_root: PathBuf,
    presence_workspace: Option<PathBuf>,
    requested: bool,
    fixture_status: Option<Result<(), &'static str>>,
}

impl Capture {
    pub(super) fn from_environment() -> Option<Self> {
        let spec = std::env::var_os("SLATE_SHOT")?;
        let resolved = std::env::current_dir().and_then(|cwd| {
            Self::resolve(
                Some(spec),
                std::env::var_os("SLATE_PRESENCE_REVIEW"),
                &cwd,
                &std::env::temp_dir(),
            )
        });
        let capture = resolved.unwrap_or_else(|error| fatal(&error.to_string()));
        if let Some(capture) = &capture {
            eprintln!(
                "SLATE_SHOT isolated recovery root: {}",
                capture.recovery_root.display()
            );
        }
        capture
    }

    /// Injected inputs keep tests independent of process-wide environment changes.
    fn resolve(
        spec: Option<OsString>,
        presence_workspace: Option<OsString>,
        cwd: &Path,
        machine_temp: &Path,
    ) -> io::Result<Option<Self>> {
        let Some(spec) = spec else { return Ok(None) };
        let spec = spec
            .to_str()
            .ok_or_else(|| invalid("non-Unicode output path"))?;
        let (path, delay) = match spec.split_once(';') {
            Some((path, delay)) => (path, delay.parse().unwrap_or(240)),
            None => (spec, 240),
        };
        if path.is_empty() || !cwd.is_absolute() || !machine_temp.is_absolute() {
            return Err(invalid("output path must resolve to an absolute file path"));
        }
        let path = cwd.join(path);
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(invalid("output path must name a file"));
        }
        let recovery_root = machine_temp
            .join("slate-shot-recovery")
            .join(atlas_agent::request_id());
        Ok(Some(Self {
            path,
            delay,
            recovery_root,
            presence_workspace: presence_workspace.map(PathBuf::from),
            requested: false,
            fixture_status: None,
        }))
    }

    pub(super) fn recovery_root(&self) -> &Path {
        &self.recovery_root
    }

    pub(super) fn presence_workspace(&self) -> Option<&Path> {
        self.presence_workspace.as_deref()
    }

    /// Optional application fixture readiness; ordinary captures impose none.
    pub(super) fn set_fixture_status(&mut self, status: Result<(), &'static str>) {
        self.fixture_status = Some(status);
    }

    fn request_due(&self, frame: u64, storage_ready: bool) -> Result<bool, &'static str> {
        if !storage_ready || frame < self.delay {
            return Ok(false);
        }
        if let Some(status) = self.fixture_status {
            status?;
        }
        Ok(!self.requested)
    }

    /// `SLATE_SHOT=<path>[;delay_frames]`: capture the real viewport and exit.
    pub(super) fn frame(&mut self, ctx: &egui::Context, frame: u64, storage: Result<bool, &str>) {
        let ready = storage.unwrap_or_else(|error| fatal(&format!("isolated recovery: {error}")));
        if self
            .request_due(frame, ready)
            .unwrap_or_else(|error| fatal(error))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            self.requested = true;
        }
        let shot: Option<std::sync::Arc<egui::ColorImage>> = ctx.input(|input| {
            input.raw.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = shot.filter(|_| self.requested && ready) {
            let [width, height] = image.size;
            let mut rgba = Vec::with_capacity(width * height * 4);
            for pixel in &image.pixels {
                rgba.extend_from_slice(&pixel.to_array());
            }
            let image = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
                .unwrap_or_else(|| fatal("invalid screenshot dimensions"));
            image
                .save(&self.path)
                .unwrap_or_else(|error| fatal(&error.to_string()));
            std::process::exit(0);
        }
        ctx.request_repaint();
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn fatal(message: &str) -> ! {
    eprintln!("SLATE_SHOT capture failed: {message}");
    std::process::exit(2)
}

#[cfg(test)]
mod tests;
