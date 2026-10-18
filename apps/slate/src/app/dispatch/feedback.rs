use super::*;

impl SlateApp {
    pub(super) fn feedback_open_command(&mut self) -> bool {
        self.feedback.open_picker();
        true
    }

    pub(super) fn feedback_report_command(&mut self) -> bool {
        self.toast("Collaborator feedback reports are recorded through the scoped board link.");
        true
    }
}
