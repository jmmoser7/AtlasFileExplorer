//! Prepare a reviewable crash summary without touching local report attachments.

use super::SlateApp;
use atlas_core::crash_report;
use eframe::egui;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Default)]
pub(super) struct Review {
    result: Option<Receiver<Result<String, String>>>,
}

impl Review {
    pub(super) fn pending(&self) -> bool {
        self.result.is_some()
    }
}

impl SlateApp {
    pub(super) fn review_crash_report(&mut self, ctx: &egui::Context) {
        if self.crash_feedback.pending() || !self.feedback.can_review_bug() {
            return;
        }
        if self.feedback.reopen_reviewed_bug() {
            return;
        }
        let repaint = ctx.clone();
        let (send, receive) = mpsc::channel();
        match std::thread::Builder::new()
            .name("slate-review-crash-report".into())
            .spawn(move || {
                let _ = send.send(latest_report_text());
                repaint.request_repaint();
            }) {
            Ok(_) => self.crash_feedback.result = Some(receive),
            Err(_) => self.toast("Couldn’t start the crash report review."),
        }
    }

    pub(super) fn poll_crash_report_review(&mut self) {
        let Some(pending) = &self.crash_feedback.result else {
            return;
        };
        let result = match pending.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Couldn’t prepare the crash report.".into()),
        };
        self.crash_feedback.result = None;
        match result {
            Ok(text) => {
                if !self.feedback.review_bug(text) {
                    self.toast(
                        "Finish your current suggestion, then review the crash report again.",
                    );
                }
            }
            Err(message) => self.toast(message),
        }
    }
}

fn latest_report_text() -> Result<String, String> {
    let reports = crash_report::recent_reports("slate")
        .map_err(|_| "Couldn’t find the local crash reports.")?;
    let latest = reports
        .first()
        .ok_or("There are no local crash reports to review.")?;
    let summary = crash_report::submission_summary(&latest.dir)
        .map_err(|_| "Couldn’t read a summary of the latest crash report.")?;
    let details = serde_json::to_string_pretty(&summary)
        .map_err(|_| "Couldn’t format the crash report summary.")?;
    Ok(format!("Slate crash report\n\n{details}"))
}
