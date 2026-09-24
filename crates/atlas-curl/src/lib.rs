//! The one curl transport for model adapters (Ollama, ComfyUI, OpenAI).
//!
//! An adapter describes a [`Request`]; this crate runs the operating system's
//! `curl` with its whole configuration written to stdin (`-K -`). Headers such
//! as an API key therefore never appear on a command line, and a connection can
//! be interrupted by killing the child even while a model loads. No renderer,
//! no provider knowledge, no logging of request contents.
use serde_json::Value;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

/// One multipart field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Text(String),
    /// A file sent under `filename`, or under its own name when `None`.
    File {
        path: PathBuf,
        filename: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    None,
    Json(String),
    Form(Vec<(String, Field)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    /// A loopback server: never route through a proxy.
    pub loopback: bool,
    /// Deliver the response as it arrives (streamed chat).
    pub stream: bool,
    pub headers: Vec<(String, String)>,
    pub body: Body,
    /// Write the response body to this file instead of stdout.
    pub output: Option<PathBuf>,
}

/// Why a request produced no usable body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// curl could not be started or the request could not be expressed.
    Transport(String),
    /// The server answered with an error; the body it sent, possibly empty.
    Status(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "{e}"),
            Self::Status(body) if body.trim().is_empty() => {
                write!(f, "The server refused the request.")
            }
            Self::Status(body) => write!(f, "{}", body.trim()),
        }
    }
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            timeout: Duration::from_secs(10),
            connect_timeout: Duration::from_secs(3),
            loopback: false,
            stream: false,
            headers: Vec::new(),
            body: Body::None,
            output: None,
        }
    }

    pub fn post_json(url: impl Into<String>, body: &Value) -> Self {
        Self {
            body: Body::Json(body.to_string()),
            ..Self::get(url)
        }
    }

    pub fn post_form(url: impl Into<String>, fields: Vec<(String, Field)>) -> Self {
        Self {
            body: Body::Form(fields),
            ..Self::get(url)
        }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn loopback(mut self) -> Self {
        self.loopback = true;
        self
    }

    pub fn stream(mut self) -> Self {
        self.stream = true;
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn output(mut self, path: &Path) -> Self {
        self.output = Some(path.to_path_buf());
        self
    }

    /// The curl configuration this request writes to stdin.
    pub fn config(&self) -> Result<String, Failure> {
        let mut lines = vec![
            "silent".to_string(),
            "show-error".to_string(),
            "fail-with-body".to_string(),
            format!(
                "connect-timeout = {}",
                self.connect_timeout.as_secs().max(1)
            ),
            format!("max-time = {}", self.timeout.as_secs().max(1)),
        ];
        if self.loopback {
            lines.push(format!("noproxy = {}", quote("*")));
        }
        if self.stream {
            lines.push("no-buffer".into());
        }
        for (name, value) in &self.headers {
            if name.contains([':', '\r', '\n']) || value.contains(['\r', '\n']) {
                return Err(Failure::Transport("A request header was malformed.".into()));
            }
            lines.push(format!("header = {}", quote(&format!("{name}: {value}"))));
        }
        match &self.body {
            Body::None => {}
            Body::Json(json) => {
                lines.push(format!(
                    "header = {}",
                    quote("Content-Type: application/json")
                ));
                lines.push(format!("data-binary = {}", quote(json)));
            }
            Body::Form(fields) => {
                for (name, field) in fields {
                    lines.push(format!("form = {}", quote(&form_value(name, field)?)));
                }
            }
        }
        if let Some(path) = &self.output {
            lines.push(format!("output = {}", quote(&path.to_string_lossy())));
        }
        lines.push(format!("url = {}", quote(&self.url)));
        let mut text = lines.join("\n");
        text.push('\n');
        Ok(text)
    }

    /// Start the request with stdout piped for the caller to read.
    pub fn spawn(&self) -> Result<Child, Failure> {
        let config = self.config()?;
        let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
        hidden(&mut command)
            .args(["-K", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command
            .spawn()
            .map_err(|e| Failure::Transport(format!("curl is unavailable: {e}")))?;
        let written = child
            .stdin
            .take()
            .map(|mut stdin| stdin.write_all(config.as_bytes()));
        if let Some(Err(e)) = written {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Failure::Transport(e.to_string()));
        }
        Ok(child)
    }

    /// Run to completion and return the response body.
    pub fn send(&self) -> Result<Vec<u8>, Failure> {
        let out = self
            .spawn()?
            .wait_with_output()
            .map_err(|e| Failure::Transport(e.to_string()))?;
        if !out.status.success() {
            return Err(Failure::Status(
                String::from_utf8_lossy(&out.stdout).into_owned(),
            ));
        }
        Ok(out.stdout)
    }

    /// Run to completion unless `cancel` turns true, which kills the transfer.
    /// The response is drained while it arrives: a large body (a picture as
    /// base64) would otherwise fill the pipe and leave curl blocked forever.
    /// `Ok(None)` means cancelled.
    pub fn send_until(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<Vec<u8>>, Failure> {
        use std::io::Read;
        use std::sync::atomic::Ordering;
        let mut child = self.spawn()?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| Failure::Transport("curl gave no output pipe.".into()))?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut body = Vec::new();
            let read = stdout.read_to_end(&mut body).map(|_| body);
            let _ = tx.send(read);
        });
        let body = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(read) => break read.map_err(|e| Failure::Transport(e.to_string()))?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break Vec::new(),
            }
        };
        let status = child
            .wait()
            .map_err(|e| Failure::Transport(e.to_string()))?;
        if !status.success() {
            return Err(Failure::Status(String::from_utf8_lossy(&body).into_owned()));
        }
        Ok(Some(body))
    }

    pub fn json(&self) -> Result<Value, Failure> {
        serde_json::from_slice(&self.send()?).map_err(|e| Failure::Transport(e.to_string()))
    }
}

/// Keep a spawned console program from flashing a window.
pub fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

/// Percent-encoding for one URL query component.
pub fn encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn form_value(name: &str, field: &Field) -> Result<String, Failure> {
    if name.contains(['=', ';', '"']) {
        return Err(Failure::Transport(format!(
            "Form field name {name:?} is not allowed."
        )));
    }
    Ok(match field {
        Field::Text(text) => {
            if text.starts_with(['@', '<']) {
                return Err(Failure::Transport(
                    "A form value may not start with @ or <.".into(),
                ));
            }
            format!("{name}={text}")
        }
        Field::File { path, filename } => {
            let shown = path.to_string_lossy();
            if shown.contains([';', ',', '"']) {
                return Err(Failure::Transport(format!(
                    "Rename the file so its path has no ; , or \" characters: {}",
                    path.display()
                )));
            }
            match filename {
                Some(file) if !file.contains([';', ',', '"']) => {
                    format!("{name}=@{shown};filename={file}")
                }
                _ => format!("{name}=@{shown}"),
            }
        }
    })
}

/// A curl config string: double-quoted with backslash escapes.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_response_is_drained_while_it_arrives_and_cancel_kills_the_transfer() {
        let dir = std::env::temp_dir().join(format!("atlas-curl-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.json");
        // Several megabytes, like a picture returned as base64: far beyond a pipe buffer.
        let body = vec![b'A'; 6 * 1024 * 1024];
        std::fs::write(&path, &body).unwrap();
        let url = format!("file:///{}", path.to_string_lossy().replace('\\', "/"));
        let request = Request::get(url).timeout(Duration::from_secs(30));
        let started = std::time::Instant::now();
        let got = request
            .send_until(&std::sync::atomic::AtomicBool::new(false))
            .unwrap()
            .unwrap();
        assert_eq!(got.len(), body.len());
        assert!(started.elapsed() < Duration::from_secs(20));
        let cancelled = request
            .send_until(&std::sync::atomic::AtomicBool::new(true))
            .unwrap();
        assert!(cancelled.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_json_post_quotes_its_body_and_keeps_headers_off_the_command_line() {
        let request = Request::post_json(
            "https://api.example.com/v1/images",
            &serde_json::json!({"prompt": "say \"hi\"\nnow", "path": "C:\\a"}),
        )
        .header("Authorization", "Bearer sk-test")
        .timeout(Duration::from_secs(90));
        let config = request.config().unwrap();
        assert!(config.contains("header = \"Authorization: Bearer sk-test\""));
        assert!(config.contains(r#"data-binary = "{\""#));
        assert!(config.contains(r#"\"prompt\":\"say \\\"hi\\\"\\nnow\""#));
        assert!(config.contains(r#"\"path\":\"C:\\\\a\""#));
        assert!(config.contains("max-time = 90"));
        assert!(!config.contains("noproxy"));
        assert!(config.ends_with("url = \"https://api.example.com/v1/images\"\n"));
    }

    #[test]
    fn loopback_streams_forms_and_outputs_are_expressed_and_unsafe_values_refused() {
        let config = Request::get("http://127.0.0.1:8188/view")
            .loopback()
            .stream()
            .output(Path::new("C:/out/a.png"))
            .config()
            .unwrap();
        assert!(config.contains("noproxy = \"*\""));
        assert!(config.contains("no-buffer"));
        assert!(config.contains("output = \"C:/out/a.png\""));
        let form = Request::post_form(
            "http://127.0.0.1:8188/upload/image",
            vec![
                (
                    "image".into(),
                    Field::File {
                        path: "C:/in/view.png".into(),
                        filename: Some("slate-view.png".into()),
                    },
                ),
                ("overwrite".into(), Field::Text("true".into())),
            ],
        )
        .config()
        .unwrap();
        assert!(form.contains("form = \"image=@C:/in/view.png;filename=slate-view.png\""));
        assert!(form.contains("form = \"overwrite=true\""));
        let bad = Request::post_form(
            "http://x",
            vec![(
                "image".into(),
                Field::File {
                    path: "C:/a;b.png".into(),
                    filename: None,
                },
            )],
        );
        assert!(bad.config().is_err());
        assert!(Request::get("http://x")
            .header("X", "a\r\nInjected: yes")
            .config()
            .is_err());
        assert_eq!(encode_component("a b/c"), "a%20b%2Fc");
    }
}
