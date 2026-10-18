//! One cloud accounting and confirmation owner for copy and staging export.
//!
//! Listing source entries can take seconds on a share. Audit off-thread, bind
//! the answer to its requesting workspace, and never turn a lost reply into
//! permission to download.

use super::{AtlasApp, AtlasPick, EditMode, ExportUi, FsOp, PickPurpose};
use atlas_core::cloud::{self, CloudCost};
use atlas_core::export::{self, ExportItem};
use atlas_core::types::human_size;
use atlas_shell::file_picker::{self, PickRequest};
use crossbeam_channel::{unbounded, Receiver, TryRecvError};
use eframe::egui::{self, Pos2};
use std::path::PathBuf;

mod completion;
#[cfg(test)]
mod tests;

const BUSY: &str = "A file operation or export is already pending";
const STALE: &str = "Transfer cancelled — the folder or tab changed";
const AUDIT_FAILED: &str = "Cloud check failed — transfer cancelled; try again";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CloudContext {
    tab: u64,
    root: PathBuf,
    generation: u64,
}

enum Transfer {
    FsCopy {
        op: FsOp,
        label: String,
    },
    Export {
        dest: PathBuf,
        items: Vec<ExportItem>,
    },
}

impl Transfer {
    fn sources(&self) -> Vec<PathBuf> {
        match self {
            Self::FsCopy {
                op: FsOp::Copy { sources, .. },
                ..
            } => sources.clone(),
            Self::Export { items, .. } => items.iter().map(|item| item.source.clone()).collect(),
            Self::FsCopy { .. } => unreachable!("only copies require cloud accounting"),
        }
    }

    fn authorize(
        &self,
        origin: &CloudContext,
        current: Option<&CloudContext>,
        edit_mode: EditMode,
        busy: bool,
    ) -> Result<(), &'static str> {
        if current != Some(origin) {
            Err(STALE)
        } else if busy {
            Err(BUSY)
        } else if matches!(self, Self::FsCopy { .. }) && edit_mode != EditMode::Edit {
            Err("Switch to Edit mode before changing files")
        } else {
            Ok(())
        }
    }
}

pub(super) struct CloudAudit {
    rx: Receiver<CloudCost>,
    transfer: Transfer,
    context: CloudContext,
    pub(super) label: String,
    at: Pos2,
}

impl CloudAudit {
    fn poll(&self, current: Option<&CloudContext>) -> Option<Result<CloudCost, &'static str>> {
        if current != Some(&self.context) {
            return Some(Err(STALE));
        }
        match self.rx.try_recv() {
            Ok(cost) => Some(Ok(cost)),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(AUDIT_FAILED)),
        }
    }
}

pub(super) struct CloudCopyPlan {
    transfer: Transfer,
    context: CloudContext,
    cost: CloudCost,
    at: Pos2,
}

impl AtlasApp {
    pub(super) fn cloud_context(&self) -> Option<CloudContext> {
        if self.at_home {
            return None;
        }
        Some(CloudContext {
            tab: self.tabs.get(self.active_tab)?.id,
            root: self.root.clone()?,
            generation: self.generation,
        })
    }

    fn cloud_transfer_busy(&self) -> bool {
        self.cloud_audit.is_some()
            || self.cloud_copy_plan.is_some()
            || self.fs_op.is_some()
            || self.export_ui.is_some()
    }

    pub(super) fn dispatch_fs_op(&mut self, op: FsOp, label: String) {
        if self.edit_mode != EditMode::Edit {
            self.toast("Switch to Edit mode before changing files");
            return;
        }
        if self.cloud_transfer_busy() {
            self.toast(BUSY);
            return;
        }
        if matches!(op, FsOp::Copy { .. }) {
            self.audit_transfer(Transfer::FsCopy { op, label });
        } else {
            self.start_fs_op_unchecked(op, label);
        }
    }

    fn audit_transfer(&mut self, transfer: Transfer) {
        if self.cloud_transfer_busy() {
            self.toast(BUSY);
            return;
        }
        let Some(context) = self.cloud_context() else {
            self.toast(STALE);
            return;
        };
        let sources = transfer.sources();
        let label = match &transfer {
            Transfer::FsCopy { label, .. } => label.clone(),
            Transfer::Export { .. } => "Export".into(),
        };
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let _ = tx.send(cloud::copy_cloud_cost(&sources));
        });
        self.cloud_audit = Some(CloudAudit {
            rx,
            transfer,
            context,
            label,
            at: self.dialog_anchor(),
        });
    }

    pub(super) fn poll_cloud_audit(&mut self) {
        let current = self.cloud_context();
        let Some(result) = self
            .cloud_audit
            .as_ref()
            .and_then(|a| a.poll(current.as_ref()))
        else {
            return;
        };
        let Some(audit) = self.cloud_audit.take() else {
            return;
        };
        match result {
            Err(error) => self.toast(error),
            Ok(cost) if cost.is_free() => self.start_cloud_transfer(audit.transfer, audit.context),
            Ok(cost) => {
                self.cloud_copy_plan = Some(CloudCopyPlan {
                    transfer: audit.transfer,
                    context: audit.context,
                    cost,
                    at: audit.at,
                });
            }
        }
    }

    fn start_cloud_transfer(&mut self, transfer: Transfer, context: CloudContext) {
        let current = self.cloud_context();
        if let Err(error) = transfer.authorize(
            &context,
            current.as_ref(),
            self.edit_mode,
            self.cloud_transfer_busy(),
        ) {
            self.toast(error);
            return;
        }
        match transfer {
            Transfer::FsCopy { op, label } => self.start_fs_op_unchecked(op, label),
            Transfer::Export { dest, items } => {
                let total = items.len();
                let rx = export::start_export(context.root.clone(), dest, items);
                self.export_ui = Some(ExportUi {
                    rx,
                    origin: context,
                    completed: None,
                    done: 0,
                    total,
                    current: String::new(),
                });
                self.push_history("atlas.export", Some(format!("{total} file(s)")));
            }
        }
    }

    pub(super) fn pick_export_dest(&mut self) {
        if self.cloud_transfer_busy() {
            self.toast(BUSY);
            return;
        }
        let Some(context) = self.cloud_context() else {
            return;
        };
        if self.picker.open(
            PickRequest::folder().title("Choose export destination"),
            move |picked| AtlasPick::Export {
                context,
                dest: file_picker::first(picked),
            },
        ) {
            self.picking = PickPurpose::Export;
        }
    }

    pub(super) fn begin_export(&mut self, dest: PathBuf) {
        if self.cloud_transfer_busy() {
            self.toast(BUSY);
            return;
        }
        let items = self.assigned_items();
        if items.is_empty() {
            self.toast("Nothing assigned to export");
            return;
        }
        self.audit_transfer(Transfer::Export { dest, items });
    }

    fn respond_cloud_copy(&mut self, accept: bool) {
        if let Some(plan) = self.cloud_copy_plan.take() {
            if accept {
                self.start_cloud_transfer(plan.transfer, plan.context);
            }
        }
    }

    pub(super) fn cloud_copy_confirm_window(&mut self, ctx: &egui::Context) {
        let Some(plan) = &self.cloud_copy_plan else {
            return;
        };
        if self.cloud_context().as_ref() != Some(&plan.context) {
            self.respond_cloud_copy(false);
            self.toast(STALE);
            return;
        }
        let (files, bytes, at) = (plan.cost.files, plan.cost.bytes, plan.at);
        let mut open = true;
        let mut accept = false;
        let mut cancel = false;
        egui::Window::new("Copy cloud-only files?")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_pos(at)
            .constrain(true)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} selected file(s) are stored online only.",
                    super::group_digits(files as u64)
                ));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Copying them will download about {} first.",
                        human_size(bytes)
                    ))
                    .strong(),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Atlas will not start this transfer unless you confirm.")
                        .small()
                        .color(self.palette().sub),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Copy and download").clicked() {
                        accept = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if accept || cancel || !open {
            self.respond_cloud_copy(accept);
        }
    }
}
