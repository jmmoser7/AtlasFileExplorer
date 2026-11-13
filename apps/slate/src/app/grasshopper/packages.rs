//! Package Manager restore and user-dropped local Grasshopper plug-ins.

use super::{output, response_status, start_worker, Cancellation, Output, Request, Worker};
use atlas_connector_runtime::headless_rhino::{
    grasshopper_packages::{install_local_package, restore_missing_packages, stage_local_package},
    HeadlessRhino,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) fn is_local_plugin(path: &Path) -> bool {
    path.is_absolute()
        && (is_raw_plugin(path)
            || path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("yak")))
}

pub(super) fn is_raw_plugin(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gha"))
}

pub(super) struct Restorer<'a> {
    pub worker: &'a mut Option<Worker>,
    pub config: &'a HeadlessRhino,
    pub startup_failure: &'a mut Option<String>,
    pub stop: &'a Cancellation,
    pub stage: &'a dyn Fn(&'static str),
}

impl Restorer<'_> {
    pub(super) fn resolve(&mut self, output: &mut Output, request: &Request, dropped: &[PathBuf]) {
        let result = if dropped.is_empty() {
            self.replay_approved(output, request).and_then(|()| {
                if needs_packages(output) {
                    self.restore_from_manager(output, request)?;
                }
                Ok(())
            })
        } else {
            self.load_dropped(output, request, dropped)
        };
        if let Err(error) = result {
            output.response.diagnostics.insert(0, error);
        }
    }

    fn restore_from_manager(
        &mut self,
        output: &mut Output,
        request: &Request,
    ) -> Result<(), String> {
        (self.stage)("Checking Rhino Package Manager for missing components…");
        let library_ids: BTreeSet<_> = output
            .response
            .missing_packages
            .iter()
            .filter(|package| !package.already_loaded)
            .filter_map(|package| package.library_id.as_deref())
            .collect();
        let catalog = if library_ids.is_empty() {
            None
        } else {
            Some(self.exchange(
                &serde_json::json!({ "action": "resolve_packages", "library_ids": library_ids }),
            )?)
        };
        (self.stage)("Restoring compatible Grasshopper packages…");
        let report = restore_missing_packages(
            &self.config.rhino_system,
            catalog
                .as_ref()
                .map_or(&[], |entry| entry.response.trusted_sources.as_slice()),
            &output.response.missing_packages,
            catalog
                .as_ref()
                .map_or(&[], |entry| entry.response.candidates.as_slice()),
            self.stop,
            package_deadline(),
        )?;
        if report.needs_restart() {
            self.reload(output, request)?;
            self.replay_approved(output, request)?;
        }
        if needs_packages(output) {
            let errors = catalog
                .into_iter()
                .flat_map(|entry| entry.response.errors.clone());
            output
                .response
                .diagnostics
                .splice(0..0, report.issues.into_iter().chain(errors));
        }
        Ok(())
    }

    fn load_dropped(
        &mut self,
        output: &mut Output,
        request: &Request,
        paths: &[PathBuf],
    ) -> Result<(), String> {
        for path in paths {
            if !needs_packages(output) {
                break;
            }
            if is_raw_plugin(path) {
                (self.stage)("Loading the dropped Grasshopper plug-in…");
                let binding = self.load_raw(path, None, &missing_component_ids(output))?;
                *output = self.exchange(request)?;
                if let Err(error) = self.remember(&binding) {
                    output.response.diagnostics.push(error);
                }
            } else {
                (self.stage)("Checking the dropped Yak package…");
                let staged = stage_local_package(path, &self.config.workspace)?;
                let inspected = self.exchange(&serde_json::json!({
                    "action": "inspect_local_package", "package_path": staged.path()
                }))?;
                let package = inspected
                    .response
                    .local_package
                    .as_ref()
                    .ok_or_else(|| response_status(&inspected))?;
                (self.stage)("Installing the dropped Yak package…");
                let report = install_local_package(
                    &self.config.rhino_system,
                    &staged,
                    package,
                    &output.response.missing_packages,
                    self.stop,
                    package_deadline(),
                )?;
                if report.needs_restart() {
                    self.reload(output, request)?;
                }
                if needs_packages(output) {
                    output.response.diagnostics.splice(0..0, report.issues);
                }
            }
        }
        self.replay_approved(output, request)
    }

    fn replay_approved(&mut self, output: &mut Output, request: &Request) -> Result<(), String> {
        if !needs_packages(output) {
            return Ok(());
        }
        let Ok(entries) = fs::read_dir(self.approval_directory()) else {
            return Ok(());
        };
        let mut approvals: Vec<_> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                (path
                    .extension()
                    .is_some_and(|extension| extension == "json"))
                .then(|| (entry.metadata().and_then(|meta| meta.modified()).ok(), path))
            })
            .collect();
        approvals.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, path) in approvals {
            if !needs_packages(output) {
                break;
            }
            let Some(binding) = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Binding>(&bytes).ok())
            else {
                continue;
            };
            if !binding.path.is_absolute()
                || !is_raw_plugin(&binding.path)
                || !valid_hash(&binding.sha256)
            {
                continue;
            }
            let missing = missing_component_ids(output);
            let required: BTreeSet<_> = binding
                .component_ids
                .into_iter()
                .filter(|id| missing.contains(id))
                .collect();
            if !required.is_empty() && binding.path.is_file() {
                (self.stage)("Loading a previously supplied Grasshopper plug-in…");
                self.load_raw(&binding.path, Some(&binding.sha256), &required)?;
                *output = self.exchange(request)?;
            }
        }
        Ok(())
    }

    fn load_raw(
        &mut self,
        path: &Path,
        expected: Option<&str>,
        missing: &BTreeSet<String>,
    ) -> Result<Binding, String> {
        let reply = self.exchange(&serde_json::json!({
            "action": "load_local_plugin", "source_path": path,
            "component_ids": missing, "expected_sha256": expected
        }))?;
        let hash = reply
            .response
            .sha256
            .as_ref()
            .filter(|hash| valid_hash(hash));
        let component_ids: Vec<_> = reply
            .response
            .loaded_component_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .filter(|id| missing.contains(id))
            .collect();
        if let Some(sha256) =
            hash.filter(|_| reply.response.status == "ready" && !component_ids.is_empty())
        {
            return Ok(Binding {
                path: path.to_path_buf(),
                sha256: sha256.to_ascii_lowercase(),
                component_ids,
            });
        }
        // A CLR plug-in can leave registrations behind even when identity verification fails.
        *self.worker = None;
        Err(response_status(&reply))
    }

    fn reload(&mut self, output: &mut Output, request: &Request) -> Result<(), String> {
        (self.stage)("Reloading Grasshopper with restored packages…");
        *self.worker = None;
        *self.worker = Some(start_worker(self.config, self.stop, self.startup_failure)?);
        *output = self.exchange(request)?;
        Ok(())
    }

    fn exchange(&mut self, request: &impl Serialize) -> Result<Output, String> {
        let result = self
            .worker
            .as_mut()
            .ok_or("Grasshopper worker is unavailable")?
            .request(request, self.stop);
        if result.is_err() {
            *self.worker = None;
        }
        result
    }

    fn approval_directory(&self) -> PathBuf {
        atlas_core::index::data_dir().join("headless-rhino/local-plugins")
    }

    fn remember(&self, binding: &Binding) -> Result<(), String> {
        let key = model_preview::view_meta::hash_bytes(binding.path.to_string_lossy().as_bytes());
        let destination = self.approval_directory().join(format!("{key}.json"));
        let draft = output::Draft::reserve(destination)?;
        let bytes = serde_json::to_vec(binding).map_err(|error| error.to_string())?;
        fs::write(draft.path(), bytes).map_err(|error| error.to_string())?;
        draft.commit().map(|_| ())
    }
}

/// A per-user binding can only be written following a verified local drop.
/// Workbook contents never supply executable plug-in paths.
#[derive(Serialize, Deserialize)]
struct Binding {
    path: PathBuf,
    sha256: String,
    component_ids: Vec<String>,
}

fn needs_packages(output: &Output) -> bool {
    output.response.status == "missing_packages"
}

fn missing_component_ids(output: &Output) -> BTreeSet<String> {
    output
        .response
        .missing_packages
        .iter()
        .flat_map(|package| {
            package
                .component_ids
                .iter()
                .map(|id| id.to_ascii_lowercase())
        })
        .collect()
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn package_deadline() -> Instant {
    Instant::now() + Duration::from_secs(120)
}
