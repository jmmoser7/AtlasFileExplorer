use std::time::{Duration, Instant};

pub const RECORD_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordedStep {
    pub at_ms: u64,
    pub label: String,
}

/// Step capture for a Reproduce report. Steps are command ids and stall
/// summaries only: a command's detail can carry typed text, a search, a
/// path, or a secret, so it is never recorded.
#[derive(Clone, Debug)]
pub struct Recorder {
    started: Instant,
    origin_ms: u64,
    pub steps: Vec<RecordedStep>,
    /// Newest stall already recorded (`Stall::unix_ms`).
    last_stall_ms: u64,
    last_stall_poll: Option<Instant>,
}

/// How often recording reads the session log's stall tail.
const STALL_POLL: Duration = Duration::from_millis(250);

impl Recorder {
    pub fn start(origin_ms: u64) -> Self {
        Self {
            started: Instant::now(),
            origin_ms,
            steps: Vec::new(),
            last_stall_ms: origin_ms,
            last_stall_poll: None,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn timed_out(&self) -> bool {
        self.elapsed() >= RECORD_TIMEOUT
    }

    pub fn remaining(&self) -> Duration {
        RECORD_TIMEOUT.saturating_sub(self.elapsed())
    }

    pub fn record_command(&mut self, id: &str) {
        if id.starts_with("app.feedback.") {
            return;
        }
        self.push(id.to_string());
    }

    /// True at most every [`STALL_POLL`], so the stall tail is not copied
    /// out of the session log every frame.
    pub fn stall_poll_due(&mut self) -> bool {
        let due = self
            .last_stall_poll
            .is_none_or(|at| at.elapsed() >= STALL_POLL);
        if due {
            self.last_stall_poll = Some(Instant::now());
        }
        due
    }

    /// Stalls newer than the last one recorded. The log keeps a bounded
    /// tail, so position is by timestamp, not index.
    pub fn ingest_stalls(&mut self, stalls: &[atlas_core::session_log::Stall]) {
        for s in stalls {
            if s.unix_ms <= self.last_stall_ms {
                continue;
            }
            self.last_stall_ms = s.unix_ms;
            self.push(format!(
                "stall {:.0} ms app / {:.0} ms delivered — {}",
                s.app_ms, s.delivered_ms, s.spans
            ));
        }
    }

    fn push(&mut self, label: String) {
        let rel = self.started.elapsed().as_millis() as u64;
        self.steps.push(RecordedStep {
            at_ms: self.origin_ms.saturating_add(rel),
            label: redact_paths_in_text(&label),
        });
    }
}

/// Replace each absolute path token with its file name. A token starts at
/// a word boundary with a drive (`C:\`, `C:/`), a UNC prefix (`\\`), or a
/// rooted `/name`, and runs to the next whitespace.
pub fn redact_paths_in_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut at_boundary = true;
    while let Some(ch) = rest.chars().next() {
        if at_boundary && looks_like_path_start(rest) {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let (token, tail) = rest.split_at(end);
            let closing: String = token
                .chars()
                .rev()
                .take_while(|c| matches!(c, ')' | ']' | '"' | '\'' | ',' | ';'))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            out.push_str(&path_token_to_name(&token[..token.len() - closing.len()]));
            out.push_str(&closing);
            rest = tail;
            at_boundary = false;
            continue;
        }
        out.push(ch);
        at_boundary = ch.is_whitespace() || matches!(ch, '(' | '[' | '"' | '\'' | '=' | ',');
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn looks_like_path_start(s: &str) -> bool {
    let b = s.as_bytes();
    let drive = b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'\\' || b[2] == b'/');
    let unc = s.starts_with("\\\\");
    let rooted = b.len() >= 2 && b[0] == b'/' && !b[1].is_ascii_whitespace();
    drive || unc || rooted
}

fn path_token_to_name(token: &str) -> String {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let name = trimmed
        .rsplit(['\\', '/'])
        .find(|part| !part.is_empty())
        .unwrap_or("");
    if name.is_empty() || name.ends_with(':') {
        "<path>".into()
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_windows_paths_to_file_names() {
        let s = "app.open (C:\\Users\\me\\Photos\\vacation.jpg)";
        assert_eq!(redact_paths_in_text(s), "app.open (vacation.jpg)");
    }

    #[test]
    fn redacts_unc_paths() {
        let s = r"atlas.root \\server\share\project\file.txt";
        assert_eq!(redact_paths_in_text(s), "atlas.root file.txt");
    }

    #[test]
    fn leaves_ratios_slashes_and_unicode_alone() {
        let s = "stall 40 ms app / 52 ms delivered — slate.board.paint";
        assert_eq!(redact_paths_in_text(s), s);
        assert_eq!(redact_paths_in_text("Delta: café"), "Delta: café");
        assert_eq!(redact_paths_in_text("open /home/me/a.txt"), "open a.txt");
        assert_eq!(redact_paths_in_text("root C:\\"), "root <path>");
    }

    #[test]
    fn recorder_times_out_at_sixty_seconds() {
        let r = Recorder {
            started: Instant::now() - RECORD_TIMEOUT - Duration::from_millis(1),
            origin_ms: 0,
            steps: Vec::new(),
            last_stall_ms: 0,
            last_stall_poll: None,
        };
        assert!(r.timed_out());
        assert_eq!(r.remaining(), Duration::ZERO);
    }

    #[test]
    fn records_command_ids_only_and_never_its_own() {
        let mut r = Recorder::start(0);
        r.record_command("canvas.search");
        r.record_command("app.feedback.open");
        r.record_command("board.undo");
        let labels: Vec<_> = r.steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["canvas.search", "board.undo"]);
    }
}
