//! Scheduled messages for Cursor and Codex conversations, run by Windows Task
//! Scheduler so they happen while Slate is closed.
//!
//! The schedule lives in `<link>/schedule.json` beside the conversation. The
//! task runs `slate.exe --scheduled-run <link>`, which sends the saved message
//! once and writes the reply into the same `session.json` the board reads, so
//! an opened board shows the new turns. Nothing here touches a workbook.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use atlas_agent::{AgentRequest, AgentSession, AgentStatus};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

/// A single run may take at most this long before it is stopped.
const RUN_LIMIT: Duration = Duration::from_secs(60 * 60);

/// Stored start format: local wall-clock time, no offset.
const START: &str = "%Y-%m-%dT%H:%M";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Repeat {
    #[default]
    Once,
    Hourly,
    Daily,
    Weekly,
}

impl Repeat {
    pub const ALL: [Repeat; 4] = [Repeat::Once, Repeat::Hourly, Repeat::Daily, Repeat::Weekly];

    pub fn label(self) -> &'static str {
        match self {
            Repeat::Once => "Once",
            Repeat::Hourly => "Every hour",
            Repeat::Daily => "Every day",
            Repeat::Weekly => "Every week",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSchedule {
    /// First run, local time (`2026-10-01T04:55`).
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub repeat: Repeat,
    pub prompt: String,
    pub provider: String,
    /// The conversation's project folder, else the AI workspace.
    pub cwd: String,
    pub ai_workspace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl AgentSchedule {
    pub fn start_time(&self) -> Option<NaiveDateTime> {
        NaiveDateTime::parse_from_str(&self.start, START).ok()
    }

    /// Whether a run is still ahead at `now`.
    pub fn pending(&self, now: NaiveDateTime) -> bool {
        self.repeat != Repeat::Once || self.start_time().is_some_and(|t| t > now)
    }

    /// One line for the clock's hover: "Once on Wed Oct 1 at 4:55 AM".
    pub fn describe(&self) -> String {
        let Some(t) = self.start_time() else {
            return "Scheduled".into();
        };
        let at = t.format("%-I:%M %p");
        let day = t.format("%a %b %-d");
        match self.repeat {
            Repeat::Once => format!("Once on {day} at {at}"),
            Repeat::Hourly => format!("Every hour from {day} at {at}"),
            Repeat::Daily => format!("Every day at {at}, from {day}"),
            Repeat::Weekly => format!("Every {} at {at}, from {day}", t.format("%A")),
        }
    }
}

pub fn now_local() -> NaiveDateTime {
    Local::now().naive_local()
}

/// `now` plus whole hours, for the dialog's "In 1 hour" preset.
pub fn hours_from(now: NaiveDateTime, hours: i64) -> NaiveDateTime {
    now + chrono::Duration::hours(hours)
}

pub fn format_start(t: NaiveDateTime) -> String {
    t.format(START).to_string()
}

/// Read what a person typed: a date ("tonight", "tomorrow", "Oct 1",
/// "October 1st", "10/1", "2026-10-01") and a time ("1am", "4:55 am",
/// "13:30"). A day word or a date without a year rolls forward to the next
/// time it happens; a full date that has passed is refused.
pub fn parse_when(date: &str, time: &str, now: NaiveDateTime) -> Result<NaiveDateTime, String> {
    let at = parse_time(time).ok_or("Type a time such as 1am, 4:55 am or 13:30.")?;
    let date = date.trim().to_ascii_lowercase();
    let today = now.date();
    let (day, rolls, yearless) = match date.as_str() {
        "" | "today" => (today, true, false),
        "tonight" => {
            // Small hours tonight are tomorrow's date.
            let d = if at.hour() < 6 {
                today.succ_opt().unwrap_or(today)
            } else {
                today
            };
            (d, false, false)
        }
        "tomorrow" => (today.succ_opt().unwrap_or(today), false, false),
        _ => {
            let (d, yearless) = parse_date(&date, today.year())
                .ok_or("Type a date such as tonight, tomorrow, Oct 1 or 2026-10-01.")?;
            (d, false, yearless)
        }
    };
    let mut when = day.and_time(at);
    if when <= now {
        if rolls {
            when += chrono::Duration::days(1);
        } else if yearless {
            when = day
                .with_year(day.year() + 1)
                .map(|d| d.and_time(at))
                .unwrap_or(when);
        } else {
            return Err("That time has already passed.".into());
        }
    }
    Ok(when)
}

fn parse_time(text: &str) -> Option<NaiveTime> {
    let t = text.trim().to_ascii_lowercase().replace(' ', "");
    let (body, pm) = if let Some(b) = t.strip_suffix("pm") {
        (b, Some(true))
    } else if let Some(b) = t.strip_suffix("am") {
        (b, Some(false))
    } else {
        (t.as_str(), None)
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (body.parse::<u32>().ok()?, 0),
    };
    let h = match pm {
        Some(true) if (1..=12).contains(&h) => h % 12 + 12,
        Some(false) if (1..=12).contains(&h) => h % 12,
        Some(_) => return None,
        None => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

fn parse_date(text: &str, year: i32) -> Option<(NaiveDate, bool)> {
    if let Ok(d) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some((d, false));
    }
    let slash: Vec<&str> = text.split('/').collect();
    if slash.len() == 2 || slash.len() == 3 {
        let m = slash[0].trim().parse().ok()?;
        let d = slash[1].trim().parse().ok()?;
        return match slash.get(2) {
            Some(y) => {
                let y: i32 = y.trim().parse().ok()?;
                let y = if y < 100 { 2000 + y } else { y };
                NaiveDate::from_ymd_opt(y, m, d).map(|d| (d, false))
            }
            None => NaiveDate::from_ymd_opt(year, m, d).map(|d| (d, true)),
        };
    }
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .collect();
    let month = words.first().and_then(|w| month_of(w))?;
    let day: u32 = words
        .get(1)?
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .parse()
        .ok()?;
    match words.get(2).and_then(|y| y.parse::<i32>().ok()) {
        Some(y) => NaiveDate::from_ymd_opt(y, month, day).map(|d| (d, false)),
        None => NaiveDate::from_ymd_opt(year, month, day).map(|d| (d, true)),
    }
}

fn month_of(word: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let w = word.get(..3)?;
    MONTHS.iter().position(|m| *m == w).map(|i| i as u32 + 1)
}

pub fn load(link_dir: &Path) -> Option<AgentSchedule> {
    std::fs::read(link_dir.join("schedule.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}

fn session_of(link_dir: &Path) -> Result<String, String> {
    crate::access::session_of(link_dir)
        .ok_or_else(|| "This card has no conversation folder.".into())
}

fn task_name(session: &str) -> String {
    format!("Slate agent {session}")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Task Scheduler XML. Start times are local ISO, so they never depend on the
/// Windows date format. A run missed while the computer slept runs on wake.
pub fn task_xml(schedule: &AgentSchedule, exe: &Path, link_dir: &Path) -> Result<String, String> {
    let start = schedule
        .start_time()
        .ok_or("Choose a date and time for the first run.")?;
    let boundary = format!("{}:00", format_start(start));
    let trigger = match schedule.repeat {
        Repeat::Once => format!("<TimeTrigger><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled></TimeTrigger>"),
        Repeat::Hourly => format!("<TimeTrigger><Repetition><Interval>PT1H</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled></TimeTrigger>"),
        Repeat::Daily => format!("<CalendarTrigger><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled><ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay></CalendarTrigger>"),
        Repeat::Weekly => format!(
            "<CalendarTrigger><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled><ScheduleByWeek><WeeksInterval>1</WeeksInterval><DaysOfWeek><{day} /></DaysOfWeek></ScheduleByWeek></CalendarTrigger>",
            day = start.format("%A")
        ),
    };
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Slate: a scheduled message for one agent conversation.</Description></RegistrationInfo>
  <Triggers>{trigger}</Triggers>
  <Principals><Principal id="Author"><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <ExecutionTimeLimit>PT1H</ExecutionTimeLimit>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>--scheduled-run "{link}"</Arguments></Exec></Actions>
</Task>
"#,
        exe = escape(&exe.to_string_lossy()),
        link = escape(&link_dir.to_string_lossy()),
    ))
}

/// Save the schedule and register the Windows task that runs it.
pub fn register(link_dir: &Path, schedule: &AgentSchedule, exe: &Path) -> Result<(), String> {
    let session = session_of(link_dir)?;
    if schedule.prompt.trim().is_empty() {
        return Err("Type the message to send first.".into());
    }
    let xml = task_xml(schedule, exe, link_dir)?;
    let file = std::env::temp_dir().join(format!("slate-task-{session}.xml"));
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&file, bytes).map_err(|e| format!("Could not prepare the task: {e}"))?;
    let result = schtasks(&[
        "/Create".into(),
        "/F".into(),
        "/TN".into(),
        task_name(&session),
        "/XML".into(),
        file.to_string_lossy().into_owned(),
    ]);
    let _ = std::fs::remove_file(&file);
    result?;
    crate::agent::atomic_write_json(&link_dir.join("schedule.json"), schedule)
        .map_err(|e| format!("Could not save the schedule: {e}"))
}

/// Remove the Windows task and the saved schedule.
pub fn unregister(link_dir: &Path) -> Result<(), String> {
    let session = session_of(link_dir)?;
    let _ = std::fs::remove_file(link_dir.join("schedule.json"));
    let args = ["/Delete", "/F", "/TN", &task_name(&session)].map(String::from);
    match schtasks(&args) {
        Ok(()) => Ok(()),
        Err(e) if e.contains("cannot find") || e.contains("does not exist") => Ok(()),
        Err(e) => Err(e),
    }
}

fn schtasks(args: &[String]) -> Result<(), String> {
    let mut cmd = Command::new("schtasks");
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("Task Scheduler is unavailable: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(text.chars().take(240).collect())
    }
}

fn request(schedule: &AgentSchedule, link_dir: &Path) -> Result<AgentRequest, String> {
    let output =
        crate::agent::recorded_output_dir(link_dir).map(|p| p.to_string_lossy().into_owned());
    serde_json::from_value(serde_json::json!({
        "id": crate::agent::request_id(),
        "prompt": schedule.prompt,
        "model": schedule.model,
        "at": crate::context::now_secs(),
        "output_dir": output,
    }))
    .map_err(|e| format!("Could not build the scheduled message: {e}"))
}

/// Send the saved message once and wait for the reply. Blocking; this is
/// the whole job of a `--scheduled-run` process.
pub fn run_once(link_dir: &Path) -> Result<(), String> {
    let schedule = load(link_dir).ok_or("This conversation has no schedule.")?;
    let req = request(&schedule, link_dir)?;
    let cwd = PathBuf::from(&schedule.cwd);
    let ws = PathBuf::from(&schedule.ai_workspace);
    match schedule.provider.as_str() {
        "cursor" => run_cursor(link_dir, &ws, &cwd, &req),
        "codex" => run_codex(link_dir, &cwd, &req),
        other => Err(format!("{other} cannot run on a schedule.")),
    }
}

fn run_cursor(link_dir: &Path, ws: &Path, cwd: &Path, req: &AgentRequest) -> Result<(), String> {
    let session = session_of(link_dir)?;
    crate::agent::atomic_write_json(&link_dir.join("request.json"), req)
        .map_err(|e| format!("Could not write the request: {e}"))?;
    // An open Slate's sidecar already watches this folder and answers it.
    if crate::sidecar::alive(link_dir) {
        return wait_for_reply(link_dir, &req.id);
    }
    std::env::set_var("ATLAS_AGENT_ONCE", "1");
    let mut child = crate::sidecar::spawn_cursor_sidecar_in(ws, &session, cwd, link_dir)?;
    let start = Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Ok(());
        }
        if start.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            return Err("The scheduled run took longer than an hour.".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn run_codex(link_dir: &Path, cwd: &Path, req: &AgentRequest) -> Result<(), String> {
    let link = crate::runtime::CodexLink::start_provider(
        link_dir.to_path_buf(),
        cwd.to_path_buf(),
        "codex".into(),
    );
    link.send(req.clone())?;
    wait_for_reply(link_dir, &req.id)
}

fn wait_for_reply(link_dir: &Path, id: &str) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let session = std::fs::read(link_dir.join("session.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<AgentSession>(&b).ok());
        if let Some(s) = session.filter(|s| s.request == id) {
            match s.status {
                AgentStatus::Idle => return Ok(()),
                AgentStatus::Error(e) => return Err(e),
                _ => {}
            }
        }
        if start.elapsed() > RUN_LIMIT {
            return Err("The scheduled run took longer than an hour.".into());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, START).unwrap()
    }

    #[test]
    fn people_can_type_dates_and_times_the_way_they_say_them() {
        let now = at("2026-09-24T17:02");
        assert_eq!(
            parse_when("tonight", "1am", now).unwrap(),
            at("2026-09-25T01:00")
        );
        assert_eq!(
            parse_when("tonight", "11:30 pm", now).unwrap(),
            at("2026-09-24T23:30")
        );
        assert_eq!(
            parse_when("October 1st", "4:55 am", now).unwrap(),
            at("2026-10-01T04:55")
        );
        assert_eq!(
            parse_when("Oct 1", "04:55", now).unwrap(),
            at("2026-10-01T04:55")
        );
        assert_eq!(
            parse_when("10/1", "4:55am", now).unwrap(),
            at("2026-10-01T04:55")
        );
        assert_eq!(
            parse_when("2026-10-01", "16:55", now).unwrap(),
            at("2026-10-01T16:55")
        );
        assert_eq!(
            parse_when("tomorrow", "9am", now).unwrap(),
            at("2026-09-25T09:00")
        );
        assert_eq!(
            parse_when("today", "9am", now).unwrap(),
            at("2026-09-25T09:00"),
            "past today rolls to tomorrow"
        );
        assert_eq!(
            parse_when("Jan 5", "9am", now).unwrap(),
            at("2027-01-05T09:00"),
            "a yearless past date is next year"
        );
        assert!(
            parse_when("2026-09-01", "9am", now).is_err(),
            "a full past date is refused"
        );
        assert!(parse_when("tonight", "25:00", now).is_err());
        assert!(parse_when("someday", "9am", now).is_err());
    }

    #[test]
    fn schedules_describe_themselves_and_know_when_they_are_done() {
        let s = AgentSchedule {
            start: "2026-10-01T04:55".into(),
            repeat: Repeat::Once,
            prompt: "x".into(),
            provider: "cursor".into(),
            cwd: String::new(),
            ai_workspace: String::new(),
            model: None,
        };
        assert_eq!(s.describe(), "Once on Thu Oct 1 at 4:55 AM");
        assert!(s.pending(at("2026-09-30T12:00")));
        assert!(
            !s.pending(at("2026-10-02T00:00")),
            "a finished one-time run shows no clock"
        );
        let daily = AgentSchedule {
            repeat: Repeat::Daily,
            ..s
        };
        assert!(daily.pending(at("2027-01-01T00:00")));
        assert_eq!(daily.describe(), "Every day at 4:55 AM, from Thu Oct 1");
    }

    /// Registers real tasks with Windows Task Scheduler, then removes them.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn task_scheduler_accepts_the_generated_tasks() {
        let root =
            std::env::temp_dir().join(format!("agent-sched-selftest-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for (i, repeat) in [Repeat::Once, Repeat::Weekly].into_iter().enumerate() {
            let link = root.join(format!("agent-selftest-{i}"));
            std::fs::create_dir_all(&link).unwrap();
            let s = AgentSchedule {
                start: format_start(hours_from(now_local(), 24 * 30)),
                repeat,
                prompt: "selftest".into(),
                provider: "cursor".into(),
                cwd: String::new(),
                ai_workspace: String::new(),
                model: None,
            };
            register(&link, &s, Path::new("C:/Windows/System32/cmd.exe")).unwrap();
            assert!(load(&link).is_some());
            let q = Command::new("schtasks")
                .args(["/Query", "/TN", &task_name(&format!("agent-selftest-{i}"))])
                .output()
                .unwrap();
            assert!(q.status.success(), "{}", String::from_utf8_lossy(&q.stderr));
            unregister(&link).unwrap();
            assert!(load(&link).is_none());
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn task_xml_uses_local_iso_times_and_the_right_trigger() {
        let s = AgentSchedule {
            start: "2026-10-01T04:55".into(),
            repeat: Repeat::Weekly,
            prompt: "x".into(),
            provider: "cursor".into(),
            cwd: String::new(),
            ai_workspace: String::new(),
            model: None,
        };
        let xml = task_xml(&s, Path::new("C:/Slate/slate.exe"), Path::new("C:/ws/a&b")).unwrap();
        assert!(xml.contains("<StartBoundary>2026-10-01T04:55:00</StartBoundary>"));
        assert!(xml.contains("<Thursday />"));
        assert!(xml.contains("a&amp;b"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
    }
}
