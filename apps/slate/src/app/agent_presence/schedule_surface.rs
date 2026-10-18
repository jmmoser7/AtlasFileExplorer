//! Thin view adapter; all schedule arithmetic and eligibility have existing owners.
use super::*;

impl SlateApp {
    pub(super) fn presence_schedule_surface(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.presence.scheduling.draft.as_ref() else {
            return;
        };
        let mut editor = chrome::ScheduleEditor {
            open: true,
            id: draft.id.clone(),
            date: draft.date.clone(),
            time: draft.time.clone(),
            end_date: draft.end_date.clone(),
            end_time: draft.end_time.clone(),
            repeat: serde_json::to_value(draft.repeat)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| "once".into()),
            prompt: draft.prompt.clone(),
            error: draft.error.clone().unwrap_or_default(),
            existing: draft.existing,
        };
        let action = chrome::schedule_editor_ui(ctx, &self.palette(), &mut editor);
        if !editor.open {
            self.presence.scheduling.draft = None;
            return;
        }
        if let Some(draft) = self.presence.scheduling.draft.as_mut() {
            draft.date = editor.date.clone();
            draft.time = editor.time.clone();
            draft.end_date = editor.end_date.clone();
            draft.end_time = editor.end_time.clone();
            draft.prompt = editor.prompt.clone();
            if let Ok(repeat) =
                serde_json::from_value(serde_json::Value::String(editor.repeat.clone()))
            {
                draft.repeat = repeat;
            }
        }
        let detail = match action {
            Some(chrome::ScheduleAction::Save) => {
                Some(serde_json::json!({"id":editor.id,"save":true,
                "date":editor.date,"time":editor.time,"end_date":editor.end_date,"end_time":editor.end_time,
                "repeat":editor.repeat,"prompt":editor.prompt}))
            }
            Some(chrome::ScheduleAction::Stop) => {
                Some(serde_json::json!({"id":editor.id,"stop":true}))
            }
            None => None,
        };
        if let Some(detail) = detail {
            self.dispatch(
                ctx,
                atlas_commands::CommandId("presence.schedule"),
                Some(detail.to_string()),
            );
        }
    }
}
