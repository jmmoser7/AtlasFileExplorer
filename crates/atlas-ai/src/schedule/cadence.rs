//! Recurrence arithmetic for live, explicitly armed board collaborators.
//! Time uses the same naive local wall clock as AgentSchedule and Task Scheduler.
use super::{AgentSchedule, Repeat};
use chrono::{Months, NaiveDateTime};

pub(super) fn months_after(t: NaiveDateTime, months: u32) -> NaiveDateTime {
    t.date()
        .checked_add_months(Months::new(months))
        .map(|d| d.and_time(t.time()))
        .unwrap_or(t)
}

/// First occurrence strictly after `after`, expressed as comparable naive-local
/// seconds (`now_local().and_utc().timestamp()`). This skips missed occurrences;
/// it never returns a backlog or extends the explicit end boundary.
pub fn next_local_seconds(schedule: &AgentSchedule, after: i64) -> Option<i64> {
    let start = schedule.start_time()?.and_utc().timestamp();
    let next = if start > after {
        start
    } else {
        let seconds = match schedule.repeat {
            Repeat::Once => return None,
            Repeat::Hourly => 3600,
            Repeat::Daily => 86400,
            Repeat::Weekly => 604800,
        };
        let steps = after
            .checked_sub(start)?
            .checked_div(seconds)?
            .checked_add(1)?;
        start.checked_add(steps.checked_mul(seconds)?)?
    };
    if schedule
        .end_time()
        .is_some_and(|end| next >= end.and_utc().timestamp())
    {
        None
    } else {
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schedule(repeat: Repeat) -> AgentSchedule {
        AgentSchedule {
            start: "2026-10-17T10:00".into(),
            repeat,
            prompt: "Check".into(),
            provider: "local".into(),
            cwd: String::new(),
            ai_workspace: String::new(),
            model: None,
            end: Some("2026-11-17T10:00".into()),
        }
    }
    #[test]
    fn once_has_no_catch_up_and_recurrence_skips_directly_to_the_next_future_time() {
        let once = schedule(Repeat::Once);
        let start = once.start_time().unwrap().and_utc().timestamp();
        assert_eq!(next_local_seconds(&once, start - 1), Some(start));
        assert_eq!(next_local_seconds(&once, start), None);
        let hourly = schedule(Repeat::Hourly);
        assert_eq!(
            next_local_seconds(&hourly, start + 3600 * 10 + 31),
            Some(start + 3600 * 11)
        );
        let weekly = schedule(Repeat::Weekly);
        assert_eq!(
            next_local_seconds(&weekly, start + 86400 * 8),
            Some(start + 86400 * 14)
        );
    }
    #[test]
    fn end_is_exclusive_and_malformed_start_never_runs() {
        let mut s = schedule(Repeat::Daily);
        let end = s.end_time().unwrap().and_utc().timestamp();
        assert_eq!(next_local_seconds(&s, end - 1), None);
        s.start = "broken".into();
        assert_eq!(next_local_seconds(&s, 0), None);
    }
}
