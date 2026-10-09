use std::time::{Duration, Instant};

pub const RECORD_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecordedStep {
    pub at_ms: u64,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Recorder {
    started: Instant,
    origin_ms: u64,
    pub steps: Vec<RecordedStep>,
    last_stall_count: usize,
}

impl Recorder {
    pub fn start(origin_ms: u64) -> Self {
        Self {
            started: Instant::now(),
            origin_ms,
            steps: Vec::new(),
            last_stall_count: 0,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn timed_out(&self) -> bool {
        self.elapsed() >= RECORD_TIMEOUT
    }

    pub fn record_command(&mut self, id: &str, detail: Option<&str>) {
        if id.starts_with("app.feedback.") {
            return;
        }
        let detail = detail.filter(|_| should_record_detail(id));
        let label = match detail {
            Some(d) => format!("{id} ({d})"),
            None => id.to_string(),
        };
        self.push(label);
    }

    pub fn record_ui(&mut self, area: &str) {
        self.push(format!("click: {area}"));
    }

    pub fn ingest_stalls(&mut self, stalls: &[atlas_core::session_log::Stall]) {
        while self.last_stall_count < stalls.len() {
            let s = &stalls[self.last_stall_count];
            self.last_stall_count += 1;
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

fn should_record_detail(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    !(id.contains("secret")
        || id.contains("paste")
        || id.contains("password")
        || id.contains("api_key")
        || id.contains("clipboard")
        || id.contains("search"))
}

/// Redact absolute paths to file names; leave other text intact.
pub fn redact_paths_in_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if looks_like_path_start(bytes, i) {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let token = &s[start..i];
            out.push_str(&path_token_to_name(token));
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn looks_like_path_start(bytes: &[u8], i: usize) -> bool {
    if i + 1 < bytes.len() && bytes[i].is_ascii_alphabetic() && bytes[i + 1] == b':' {
        return true;
    }
    bytes[i] == b'/' || bytes[i] == b'\\'
}

fn path_token_to_name(token: &str) -> String {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    std::path::Path::new(trimmed)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "<path>".into())
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
    fn recorder_times_out_at_sixty_seconds() {
        let mut r = Recorder {
            started: Instant::now() - RECORD_TIMEOUT - Duration::from_millis(1),
            origin_ms: 0,
            steps: Vec::new(),
            last_stall_count: 0,
        };
        assert!(r.timed_out());
    }

    #[test]
    fn secrets_and_search_skip_detail() {
        let mut r = Recorder::start(0);
        r.record_command("canvas.search", Some("my query"));
        r.record_command("app.open", Some("C:\\x\\y.doc"));
        assert_eq!(r.steps.len(), 2);
        assert_eq!(r.steps[0].label, "canvas.search");
        assert_eq!(r.steps[1].label, "app.open (y.doc)");
    }
}
