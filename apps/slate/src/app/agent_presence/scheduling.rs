//! Bounded schedules run only inside an explicitly live, granted board session.
//! Configuration is durable; arming and due work are ephemeral and never restored.
use super::*;
use atlas_ai::schedule::{self as plan, AgentSchedule, Repeat};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScheduleDraft {
    pub id: String,
    pub date: String,
    pub time: String,
    pub end_date: String,
    pub end_time: String,
    pub repeat: Repeat,
    pub prompt: String,
    pub error: Option<String>,
    pub existing: bool,
    pub armed: bool,
}
#[derive(Default)]
pub(super) struct PresenceSchedules {
    pub(super) draft: Option<ScheduleDraft>,
    entries: HashMap<String, ArmedSchedule>,
    last_poll: Option<i64>,
    last_tick: Option<Instant>,
}
struct ArmedSchedule {
    plan: AgentSchedule,
    motion: PresenceId,
    grant: String,
    tab: u64,
    due: Option<i64>,
    waiting: bool,
}
#[derive(Clone, Copy)]
struct Eligibility {
    live: bool,
    visible: bool,
    paused: bool,
    busy: bool,
}
#[derive(Debug, PartialEq)]
enum Tick {
    None,
    Waiting,
    Run,
    Disarm,
}
impl ArmedSchedule {
    fn tick(&mut self, now: i64, eligible: Eligibility, gap: bool) -> Tick {
        if !eligible.live
            || self
                .plan
                .end_time()
                .is_none_or(|end| now >= end.and_utc().timestamp())
        {
            return Tick::Disarm;
        }
        let Some(due) = self.due else {
            return Tick::Disarm;
        };
        if due > now {
            return Tick::None;
        }
        if !eligible.visible || gap {
            self.due = plan::next_local_seconds(&self.plan, now);
            self.waiting = false;
            return if self.due.is_some() {
                Tick::None
            } else {
                Tick::Disarm
            };
        }
        if eligible.paused || eligible.busy {
            self.waiting = true;
            return Tick::Waiting;
        }
        // Advance before delivery: an error or a frame retry cannot resend this run.
        self.due = plan::next_local_seconds(&self.plan, now);
        self.waiting = false;
        Tick::Run
    }
}
impl PresenceSchedules {
    pub(super) fn disarm(&mut self, ids: &[String]) {
        self.entries.retain(|id, _| !ids.contains(id));
    }
}
impl SlateApp {
    pub(crate) fn presence_open_schedule(&mut self, who: &str) -> Result<(), String> {
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who && a.departure.is_none())
            .ok_or("Restore this collaborator on an open board before scheduling it")?;
        if a.parent.is_some() {
            return Err("Temporary helpers cannot schedule future tasks".into());
        }
        let thread = self.presence_conversation(who);
        let existing = thread.and_then(|c| c.schedule.as_ref());
        let start = existing
            .and_then(AgentSchedule::start_time)
            .unwrap_or_else(|| plan::hours_from(plan::now_local(), 1));
        let end = existing
            .and_then(AgentSchedule::end_time)
            .unwrap_or_else(|| plan::end_presets(start).0);
        let prompt = existing.map(|p| p.prompt.clone()).unwrap_or_else(|| {
            thread
                .map(|c| {
                    if !c.draft.trim().is_empty() {
                        c.draft.clone()
                    } else {
                        c.turns
                            .iter()
                            .rev()
                            .find(|t| t.role == "user")
                            .map(|t| t.text.clone())
                            .unwrap_or_default()
                    }
                })
                .unwrap_or_default()
        });
        self.presence.scheduling.draft = Some(ScheduleDraft {
            id: who.into(),
            date: start.format("%Y-%m-%d").to_string(),
            time: start.format("%H:%M").to_string(),
            end_date: end.format("%Y-%m-%d").to_string(),
            end_time: end.format("%H:%M").to_string(),
            repeat: existing.map_or(Repeat::Once, |s| s.repeat),
            prompt,
            error: None,
            existing: existing.is_some(),
            armed: self.presence.scheduling.entries.contains_key(who),
        });
        Ok(())
    }
    pub(crate) fn presence_apply_schedule(&mut self, draft: ScheduleDraft) -> Result<(), String> {
        let result = self.save_presence_schedule(&draft);
        if let Err(error) = &result {
            let mut draft = draft;
            draft.error = Some(error.clone());
            self.presence.scheduling.draft = Some(draft);
        } else {
            self.presence.scheduling.draft = None;
        }
        result
    }
    fn save_presence_schedule(&mut self, draft: &ScheduleDraft) -> Result<(), String> {
        if draft.prompt.trim().is_empty() || draft.prompt.len() > 32768 {
            return Err("Supply a message of at most 32768 bytes".into());
        }
        let now = plan::now_local();
        let start = plan::parse_when(&draft.date, &draft.time, now)?;
        let end = plan::parse_end(start, &draft.end_date, &draft.end_time)?;
        let ws = self
            .ai
            .config
            .valid_workspace()
            .ok_or("Set an AI workspace before scheduling")?
            .to_path_buf();
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == draft.id && a.departure.is_none())
            .ok_or("Collaborator is not present")?;
        if a.parent.is_some() {
            return Err("Temporary helpers cannot schedule future tasks".into());
        }
        let grant = a
            .grant
            .as_ref()
            .ok_or("Grant board access before scheduling")?;
        let choice = self.presence_model_choice(&draft.id, &a.profile.provider);
        let schedule = AgentSchedule {
            start: plan::format_start(start),
            repeat: draft.repeat,
            prompt: draft.prompt.trim().into(),
            provider: a.profile.provider.clone(),
            cwd: ws.to_string_lossy().into_owned(),
            ai_workspace: ws.to_string_lossy().into_owned(),
            model: choice.model,
            end: Some(plan::format_start(end)),
        };
        let entry = ArmedSchedule {
            due: plan::next_local_seconds(&schedule, now.and_utc().timestamp()),
            plan: schedule.clone(),
            motion: a.motion.clone(),
            grant: grant.id.clone(),
            tab: a.tab,
            waiting: false,
        };
        self.presence
            .conversations
            .edit(&draft.id)?
            .set_schedule(Some(schedule));
        self.presence.conversations.save(&draft.id);
        self.presence
            .scheduling
            .entries
            .insert(draft.id.clone(), entry);
        self.presence
            .scheduling
            .last_poll
            .get_or_insert(now.and_utc().timestamp());
        Ok(())
    }
    pub(crate) fn presence_stop_schedule(&mut self, who: &str) -> Result<(), String> {
        self.presence.scheduling.entries.remove(who);
        self.presence.conversations.edit(who)?.set_schedule(None);
        self.presence.conversations.save(who);
        self.presence.scheduling.draft = None;
        Ok(())
    }
    pub(crate) fn presence_schedule_command(
        &mut self,
        who: &str,
        value: &serde_json::Value,
    ) -> Result<(), String> {
        if value["stop"].as_bool() == Some(true) {
            return self.presence_stop_schedule(who);
        }
        if value["save"].as_bool() != Some(true) {
            return self.presence_open_schedule(who);
        }
        let repeat = serde_json::from_value(value["repeat"].clone())
            .map_err(|_| "Choose Once, Hourly, Daily or Weekly")?;
        let s = |key: &str| value[key].as_str().unwrap_or("").to_owned();
        self.presence_apply_schedule(ScheduleDraft {
            id: who.into(),
            date: s("date"),
            time: s("time"),
            end_date: s("end_date"),
            end_time: s("end_time"),
            repeat,
            prompt: s("prompt"),
            error: None,
            existing: false,
            armed: false,
        })
    }
    pub(crate) fn pump_presence_schedules(&mut self, ctx: &egui::Context) {
        let tick = Instant::now();
        if self
            .presence
            .scheduling
            .last_tick
            .is_some_and(|last| tick.duration_since(last).as_millis() < 1000)
        {
            return;
        }
        self.presence.scheduling.last_tick = Some(tick);
        let now = plan::now_local().and_utc().timestamp();
        let previous = self.presence.scheduling.last_poll.replace(now);
        let gap = previous.is_some_and(|previous| now.saturating_sub(previous) > 90);
        let visible = !self.at_home
            && self.presenting.is_none()
            && self.portal_chrome.maximized.is_none()
            && self.doc().view.active_view == slate_doc::ViewKind::Board
            && self.enscape_shown_node().is_none()
            && !ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let ids: Vec<_> = self.presence.scheduling.entries.keys().cloned().collect();
        for who in ids {
            let mut entry = self.presence.scheduling.entries.remove(&who).unwrap();
            let a = self.presence.agents.iter().find(|a| a.profile.id == who);
            let eligible = Eligibility {
                live: a.is_some_and(|a| {
                    a.departure.is_none()
                        && a.motion == entry.motion
                        && a.tab == entry.tab
                        && a.grant.as_ref().is_some_and(|g| g.id == entry.grant)
                }),
                visible: visible && a.is_some_and(|a| a.tab == self.tab().id),
                paused: a.is_some_and(|a| a.paused),
                busy: self.presence.task_busy(&who),
            };
            let tick = entry.tick(now, eligible, gap);
            if tick == Tick::Run {
                let provider_matches = a.is_some_and(|a| a.profile.provider == entry.plan.provider)
                    && self
                        .ai
                        .config
                        .valid_workspace()
                        .is_some_and(|path| path.to_string_lossy() == entry.plan.ai_workspace)
                    && self.presence_model_choice(&who, &entry.plan.provider).model
                        == entry.plan.model;
                let draft = self
                    .presence_conversation(&who)
                    .map(|c| c.draft.clone())
                    .unwrap_or_default();
                let result = if provider_matches {
                    self.send_presence_message(&who, &entry.plan.prompt)
                } else {
                    Err("Harness or model changed; open Schedule and Save to confirm the new choice".into())
                };
                if let Ok(thread) = self.presence.conversations.edit(&who) {
                    thread.set_draft(&draft);
                    if let Err(error) = &result {
                        thread.push(
                            "system",
                            &format!("Schedule stopped: {error}"),
                            "Slate",
                            None,
                            "",
                            "complete",
                        );
                    }
                    self.presence.conversations.save(&who);
                }
                if let Err(error) = result {
                    if let Some(a) = self
                        .presence
                        .agents
                        .iter_mut()
                        .find(|a| a.profile.id == who)
                    {
                        a.status = error;
                        a.activity = "error".into();
                    }
                    continue;
                }
            }
            if tick != Tick::Disarm && entry.due.is_some() {
                self.presence.scheduling.entries.insert(who, entry);
            }
        }
        if !self.presence.scheduling.entries.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
    }
}
#[cfg(test)]
mod tests;
