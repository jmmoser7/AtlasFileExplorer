//! Export results belong to the workspace that started them, even when parked.

use super::{AtlasApp, CloudContext};
use atlas_core::export::ExportMsg;
use atlas_core::index::DbCmd;
use atlas_core::journal::{Action, JournalEntry};
use atlas_core::scanner;
use crossbeam_channel::TryRecvError;
use std::path::PathBuf;

#[cfg(test)]
mod tests;

enum Owner {
    Live,
    Parked(usize),
    Loading,
    Missing,
}

impl AtlasApp {
    fn export_owner(&self, origin: &CloudContext) -> Owner {
        if self
            .tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.id == origin.tab)
            && self.root.as_ref() == Some(&origin.root)
            && !self.at_home
        {
            if self
                .pending_load
                .as_ref()
                .is_some_and(|(root, _)| *root == origin.root)
            {
                Owner::Loading
            } else {
                Owner::Live
            }
        } else if let Some(index) = self.tabs.iter().position(|tab| {
            tab.id == origin.tab
                && tab
                    .parked
                    .as_ref()
                    .is_some_and(|parked| parked.root.as_ref() == Some(&origin.root))
        }) {
            Owner::Parked(index)
        } else {
            Owner::Missing
        }
    }

    pub(in crate::app) fn poll_export(&mut self) {
        let Some(mut exp) = self.export_ui.take() else {
            return;
        };
        if exp.completed.is_none() {
            // Progress volume follows the assignment count, not the screen.
            for _ in 0..64 {
                match exp.rx.try_recv() {
                    Ok(ExportMsg::Progress {
                        done,
                        total,
                        current,
                    }) => {
                        exp.done = done;
                        exp.total = total;
                        exp.current = current;
                    }
                    Ok(done @ ExportMsg::Done { .. }) => {
                        exp.completed = Some(done);
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.toast("Export worker stopped before reporting completion; check the destination for partial copies");
                        return;
                    }
                }
            }
        }
        let owner = self.export_owner(&exp.origin);
        if exp.completed.is_none() || matches!(owner, Owner::Loading) {
            self.export_ui = Some(exp);
            return;
        }
        let Some(ExportMsg::Done {
            manifest_path,
            copied,
            created_dirs,
            errors,
        }) = exp.completed
        else {
            unreachable!("only export completion is retained");
        };
        let count = copied.len();
        let dest_root = PathBuf::from(&manifest_path)
            .parent()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let label = format!("Exported {count} files");
        let action = Action::Export {
            dest_root,
            manifest_path: manifest_path.clone(),
            copied,
            created_dirs,
        };
        match owner {
            Owner::Live => self.push_journal(label, action),
            Owner::Parked(index) => {
                let parked = self.tabs[index]
                    .parked
                    .as_mut()
                    .expect("resolved export owner");
                parked.journal.push(JournalEntry {
                    ts: scanner::now_unix(),
                    label,
                    action,
                });
                self.db.send(DbCmd::SaveJournal {
                    root: exp.origin.root.clone(),
                    json: parked.journal.to_json(),
                });
            }
            Owner::Missing => {
                // Keep published files and report the expected manifest path.
                // The legacy exporter does not report manifest-write errors,
                // so completion alone cannot prove that manifest was saved.
                // Appending to a freshly loaded database journal here would
                // race a reopened workspace; only an atomic append owner can
                // safely attach undo after the original workspace is gone.
                let detail = format!(
                    "Export completed from {}; its source workspace closed or changed, so undo was not attached. Expected manifest path: {manifest_path}",
                    exp.origin.root.display()
                );
                self.push_history("atlas.export", Some(detail.clone()));
                self.toast(detail);
                if !errors.is_empty() {
                    self.toast(format!(
                        "Export finished: {count} copied, {} errors",
                        errors.len()
                    ));
                }
                return;
            }
            Owner::Loading => unreachable!("loading source journal is deferred"),
        }
        if errors.is_empty() {
            self.toast(format!("Export complete: {count} files copied"));
        } else {
            self.toast(format!(
                "Export finished: {count} copied, {} errors (see manifest folder)",
                errors.len()
            ));
        }
    }
}
