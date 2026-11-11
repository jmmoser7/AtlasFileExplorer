//! Explicit, anonymous delivery of the text portion of a local feedback report.
use super::bundle::{self, ReportJson};
use super::{FeedbackHub, FeedbackKind};
use atlas_curl::Request;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

const MAX_TITLE: usize = 160;
const MAX_BODY: usize = 6000;
const MAX_VERSION: usize = 64;
const MAX_REQUEST: usize = 32 * 1024;
const MAX_RECEIPT: usize = 1024;
const DELIVERY_ERROR: &str = "Couldn’t confirm delivery. Your local report is saved. Retry Send to use the same suggestion ID.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Text {
    source: &'static str,
    kind: &'static str,
    title: String,
    body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

impl Text {
    fn from_report(report: &ReportJson, kind: FeedbackKind) -> Result<Self, String> {
        let source = match report.app.as_str() {
            "slate" => "slate",
            "file-atlas" => "file-atlas",
            _ => return Err("Send is unavailable for this app.".into()),
        };
        let kind = match kind {
            FeedbackKind::Bug => "Bug",
            FeedbackKind::Feature => "Feature",
        };
        let first_line = report.description.lines().next().unwrap_or_default();
        let title = truncate_title(first_line);
        let title = if title.is_empty() { kind.into() } else { title };
        let mut body = report.description.clone();
        if !report.links.is_empty() {
            body.push_str("\n\nLinks\n");
            body.push_str(&report.links.join("\n"));
        }
        if body.encode_utf16().count() > MAX_BODY {
            return Err(format!(
                "Shorten the description and links to {MAX_BODY} characters to send."
            ));
        }
        if body.chars().any(
            |ch| matches!(ch, '\0'..='\u{8}' | '\u{b}'..='\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}'),
        ) {
            return Err("Remove control characters from the description or links to send.".into());
        }
        let version = report.version.trim();
        let version = (!version.is_empty()
            && version.encode_utf16().count() <= MAX_VERSION
            && !version.chars().any(|ch| ch.is_ascii_control()))
        .then(|| version.to_owned());
        Ok(Self {
            source,
            kind,
            title,
            body,
            version,
        })
    }
}

fn truncate_title(line: &str) -> String {
    let mut units = 0;
    line.chars()
        .map(|ch| if ch.is_ascii_control() { ' ' } else { ch })
        .take_while(|ch| {
            units += ch.len_utf16();
            units <= MAX_TITLE
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[derive(Clone, Serialize)]
struct Suggestion {
    id: String,
    #[serde(flatten)]
    text: Text,
}

impl Suggestion {
    fn new(text: Text) -> Self {
        Self {
            id: format!(
                "{}-{}",
                chrono::Utc::now().format("%Y%m"),
                uuid::Uuid::new_v4()
            ),
            text,
        }
    }
}

#[derive(Deserialize)]
struct Receipt {
    id: String,
}

fn deliver(endpoint: &str, suggestion: &Suggestion) -> Result<(), ()> {
    let body = serde_json::to_value(suggestion).map_err(|_| ())?;
    if body.to_string().len() > MAX_REQUEST {
        return Err(());
    }
    let request = Request::post_json(endpoint, &body);
    #[cfg(test)]
    let request = if endpoint.starts_with("http://127.0.0.1:") {
        request.loopback()
    } else {
        request
    };
    let (status, bytes) = request
        .send_bounded_with_status(MAX_RECEIPT)
        .map_err(|_| ())?;
    if !matches!(status, 200 | 201) {
        return Err(());
    }
    let receipt = serde_json::from_slice::<Receipt>(&bytes).map_err(|_| ())?;
    if receipt.id != suggestion.id {
        return Err(());
    }
    Ok(())
}

pub(super) fn endpoint(app: &str) -> Option<&'static str> {
    configured_endpoint(app, option_env!("SLATE_SUGGESTION_ENDPOINT"))
}

fn configured_endpoint(app: &str, configured: Option<&'static str>) -> Option<&'static str> {
    if app != "slate" {
        return None;
    }
    let url = configured?.trim();
    let (host, path) = url.strip_prefix("https://")?.split_once('/')?;
    (!host.is_empty()
        && !host.contains(['@', '?', '#', '\\'])
        && !host.chars().any(char::is_whitespace)
        && path == "suggestions")
        .then_some(url)
}

type Attachments = Vec<(PathBuf, Vec<u8>)>;

#[derive(Clone)]
struct LocalReport {
    dir: PathBuf,
    evidence: u64,
}

struct Outcome {
    suggestion: Suggestion,
    attachments: Attachments,
    saved: Result<LocalReport, String>,
    delivered: bool,
}

#[derive(Default)]
pub(super) struct State {
    worker: Option<Receiver<Outcome>>,
    retry: Option<(Suggestion, LocalReport)>,
    pub(super) error: Option<String>,
}

impl State {
    pub(super) fn pending(&self) -> bool {
        self.worker.is_some()
    }
}

pub(super) fn start(
    ctx: &egui::Context,
    hub: &mut FeedbackHub,
    app: &str,
    version: &str,
) -> Result<(), String> {
    let endpoint =
        endpoint(app).ok_or("Send is unavailable. Save or email your report instead.")?;
    if hub.send.pending() || hub.screenshot_pending || hub.capture_rx.is_some() {
        return Ok(());
    }
    let mut report = super::ui::prepare_report(hub, app, version)?;
    let text = Text::from_report(&report, hub.kind.ok_or("Missing report kind.")?)?;
    let (suggestion, previous_report) = match hub.send.retry.as_ref() {
        Some((previous, local)) if previous.text == text => (previous.clone(), Some(local.clone())),
        _ => (Suggestion::new(text), None),
    };
    let attachments = std::mem::take(&mut hub.attachments);
    let reports_dir = hub.prefs.reports_dir.clone();
    let repaint = ctx.clone();
    let (tx, rx) = mpsc::channel();
    hub.send.error = None;
    hub.send.worker = Some(rx);
    std::thread::spawn(move || {
        let mut hash = DefaultHasher::new();
        reports_dir.hash(&mut hash);
        attachments.hash(&mut hash);
        report.reproduce.hash(&mut hash);
        for step in &report.steps {
            step.at_ms.hash(&mut hash);
            step.label.hash(&mut hash);
        }
        let evidence = hash.finish();
        let saved = match previous_report {
            Some(local) if local.evidence == evidence && local.dir.is_dir() => Ok(local),
            _ => {
                report.git_hash = bundle::git_hash_short();
                let reports_dir = reports_dir.unwrap_or_else(bundle::default_reports_dir);
                bundle::write_bundle(&reports_dir, &report, &attachments)
                    .map(|dir| LocalReport { dir, evidence })
            }
        };
        let delivered = saved.is_ok() && deliver(endpoint, &suggestion).is_ok();
        let _ = tx.send(Outcome {
            suggestion,
            attachments,
            saved,
            delivered,
        });
        repaint.request_repaint();
    });
    Ok(())
}

pub(super) fn poll(hub: &mut FeedbackHub) -> Option<String> {
    let outcome = match hub.send.worker.as_ref()?.try_recv() {
        Ok(outcome) => outcome,
        Err(TryRecvError::Empty) => return None,
        Err(TryRecvError::Disconnected) => {
            hub.send.worker = None;
            hub.send.error = Some("Couldn’t confirm delivery.".into());
            return None;
        }
    };
    hub.send.worker = None;
    if outcome.delivered {
        let local = outcome.saved.ok()?;
        hub.set_last_bundle(local.dir);
        hub.reset_after_submit();
        return Some(format!("Suggestion sent ({})", outcome.suggestion.id));
    }
    hub.attachments.splice(..0, outcome.attachments);
    match outcome.saved {
        Ok(local) => {
            hub.set_last_bundle(local.dir.clone());
            hub.send.retry = Some((outcome.suggestion, local));
            hub.send.error = Some(DELIVERY_ERROR.into());
        }
        Err(error) => hub.send.error = Some(format!("Couldn’t save the local report. {error}")),
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    #[test]
    fn configured_send_is_available_only_in_slate() {
        let url = "https://suggestions.example.test/suggestions";
        assert_eq!(configured_endpoint("slate", Some(url)), Some(url));
        assert_eq!(configured_endpoint("slate", None), None);
        assert_eq!(configured_endpoint("file-atlas", Some(url)), None);
        assert_eq!(configured_endpoint("other", Some(url)), None);
    }

    #[test]
    fn sends_only_explicit_text_and_checks_the_http_receipt() {
        let mut hub = FeedbackHub::default();
        hub.begin_form(FeedbackKind::Feature);
        hub.description = "Request title\nRequest details".into();
        hub.links_text = "https://example.test/link".into();
        hub.attachments = vec![(PathBuf::from("private-image.png"), vec![1, 2, 3])];
        hub.recorded_steps.push(super::super::RecordedStep {
            at_ms: 123,
            label: "private-step".into(),
        });
        let mut report = super::super::ui::prepare_report(&hub, "slate", "1.0.7").unwrap();
        report.git_hash = Some("private-hash".into());
        report.bundle_dir = "private-directory".into();
        let suggestion =
            Suggestion::new(Text::from_report(&report, FeedbackKind::Feature).unwrap());
        let (month, id) = suggestion.id.split_at(6);
        assert_eq!(month, chrono::Utc::now().format("%Y%m").to_string());
        let uuid = uuid::Uuid::parse_str(&id[1..]).unwrap();
        assert_eq!(id, format!("-{uuid}"));
        assert_eq!(uuid.get_version_num(), 4);
        let expected = serde_json::json!({
            "id": suggestion.id,
            "source": "slate",
            "kind": "Feature",
            "title": "Request title",
            "body": "Request title\nRequest details\n\nLinks\nhttps://example.test/link",
            "version": "1.0.7"
        });
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/suggestions", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (status, wrong_id) in [
                ("302 Found", false),
                ("201 Created", true),
                ("200 OK", false),
                ("201 Created", false),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line, "POST /suggestions HTTP/1.1\r\n");
                let mut length = None;
                loop {
                    line.clear();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    let (name, value) = line.trim().split_once(':').unwrap();
                    assert!(!name.eq_ignore_ascii_case("authorization"));
                    if name.eq_ignore_ascii_case("content-length") {
                        length = Some(value.trim().parse().unwrap());
                    }
                }
                let mut request = vec![0; length.unwrap()];
                reader.read_exact(&mut request).unwrap();
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&request).unwrap(),
                    expected
                );
                let receipt = serde_json::json!({
                    "id": if wrong_id { "different" } else { expected["id"].as_str().unwrap() }
                })
                .to_string();
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{receipt}", receipt.len()).unwrap();
            }
        });
        assert!(deliver(&endpoint, &suggestion).is_err());
        assert!(deliver(&endpoint, &suggestion).is_err());
        assert!(deliver(&endpoint, &suggestion).is_ok());
        assert!(deliver(&endpoint, &suggestion).is_ok());
        server.join().unwrap();
    }
}
