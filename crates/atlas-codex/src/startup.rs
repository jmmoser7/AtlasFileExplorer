//! Bounded diagnostics for the exact child used by the existing startup handshake.
use super::*;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{ChildStderr, ChildStdout};
use std::sync::{Arc, Mutex};

const TAIL_BYTES: usize = 8192;

#[derive(Default)]
struct Tail(VecDeque<u8>);
impl Tail {
    fn append(&mut self, bytes: &[u8]) {
        self.0.extend(bytes);
        let excess = self.0.len().saturating_sub(TAIL_BYTES);
        self.0.drain(..excess);
    }
    fn make_bytes(&self) -> Vec<u8> {
        self.0.iter().copied().collect()
    }
}

fn redact(text: &str) -> String {
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if [
                "api_key",
                "apikey",
                "api key",
                "token",
                "authorization",
                "bearer ",
                "secret",
                "password",
                "cookie",
                "sk-",
            ]
            .iter()
            .any(|marker| lower.contains(marker))
            {
                "[redacted sensitive stderr line]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .rev()
        .take(4096)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

pub(super) struct Diagnostics {
    executable: PathBuf,
    cwd: PathBuf,
    tail: Arc<Mutex<Tail>>,
    reader: Arc<Mutex<String>>,
    stderr_done: mpsc::Receiver<()>,
}
impl Diagnostics {
    pub(super) fn new(executable: &Path, cwd: &Path, stderr: ChildStderr) -> Self {
        let tail = Arc::new(Mutex::new(Tail::default()));
        let capture = tail.clone();
        let (done, stderr_done) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut stderr = stderr;
            let mut buffer = [0; 1024];
            loop {
                match stderr.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => capture.lock().unwrap().append(&buffer[..count]),
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        capture
                            .lock()
                            .unwrap()
                            .append(format!("\nstderr reader: {error}").as_bytes());
                        break;
                    }
                }
            }
            let _ = done.send(());
        });
        Self {
            executable: executable.into(),
            cwd: cwd.into(),
            tail,
            reader: Default::default(),
            stderr_done,
        }
    }

    pub(super) fn stdout_reader(
        &self,
        output: ChildStdout,
        tx: mpsc::SyncSender<Result<Value, String>>,
    ) {
        let state = self.reader.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let result = line
                    .map_err(|e| format!("stdout reader: {e}"))
                    .and_then(|s| {
                        serde_json::from_str(&s).map_err(|e| format!("stdout JSON reader: {e}"))
                    });
                if let Err(error) = &result {
                    *state.lock().unwrap() = redact(error);
                }
                if tx.send(result).is_err() {
                    return;
                }
            }
            let mut state = state.lock().unwrap();
            if state.is_empty() {
                *state = "stdout reached EOF".into();
            }
        });
    }

    fn failure(&self, child: &mut Child, error: String) -> String {
        // Only failed startup waits, and only briefly, for the child's final stderr.
        let _ = self.stderr_done.recv_timeout(Duration::from_millis(100));
        let exit = match child.try_wait() {
            Ok(Some(status)) => format!("{status}"),
            Ok(None) => "still running".into(),
            Err(error) => format!("unavailable: {error}"),
        };
        let bytes = self.tail.lock().unwrap().make_bytes();
        let tail = redact(&String::from_utf8_lossy(&bytes));
        format!(
            "{}\nCodex startup: executable={:?}; cwd={:?}; child={exit}; reader={}; stderr_tail={}",
            redact(&error),
            self.executable,
            self.cwd,
            self.reader.lock().unwrap(),
            if tail.is_empty() { "(empty)" } else { &tail }
        )
    }
}

impl Client {
    pub(super) fn initialize_connection(&mut self) -> Result<(), String> {
        let result: Result<(), String> = (|| {
            self.rpc("initialize", json!({"clientInfo":{"name":"slate","title":"Slate agent portal","version":"0.1.0"},"capabilities":{"experimentalApi":true}}))?;
            self.write(json!({"method":"initialized","params":{}}))?;
            let account = self.rpc("account/read", json!({"refreshToken":false}))?;
            if account.get("account").is_none_or(Value::is_null) {
                return Err(
                    "Sign in to Codex with ChatGPT using `codex login`, then send again.".into(),
                );
            }
            Ok(())
        })();
        result.map_err(|error| self.startup.failure(&mut self.child, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stderr_storage_is_bounded_and_keeps_the_tail() {
        let mut tail = Tail::default();
        tail.append(&vec![b'x'; TAIL_BYTES + 100]);
        tail.append(b"latest failure");
        assert_eq!(tail.0.len(), TAIL_BYTES);
        assert!(String::from_utf8(tail.make_bytes())
            .unwrap()
            .ends_with("latest failure"));
    }
    #[test]
    fn startup_stderr_redacts_credential_lines_and_bounds_public_text() {
        let text = redact("config parse failed\napi_key = private-value\nBearer private-value\nrefresh_token = private-value\nmissing file");
        assert!(!text.contains("private-value"));
        assert!(text.contains("config parse failed"));
        assert!(text.contains("missing file"));
        assert_eq!(redact(&"x".repeat(9000)).len(), 4096);
        assert!(redact(&format!("{}final failure", "x".repeat(9000))).ends_with("final failure"));
    }

    #[test]
    fn failed_child_reports_actual_executable_cwd_exit_and_reader_eof() {
        // The Rust test harness refuses the app-server arguments and exits before
        // initialization. This exercises the existing Client startup path itself.
        let executable = std::env::current_exe().unwrap();
        let cwd = std::env::temp_dir();
        let error = Client::probe(&executable, &cwd).unwrap_err();
        assert!(error.contains(&format!("executable={executable:?}")));
        assert!(error.contains(&format!("cwd={cwd:?}")));
        assert!(error.contains("stdout reached EOF"));
        assert!(
            error.contains("child=exit ") || error.contains("child=still running"),
            "{error}"
        );
        assert!(error.contains("stderr_tail="));
        assert!(error.contains("Unrecognized option"), "{error}");
    }

    #[test]
    #[ignore = "explicit installed-provider handshake only; set CODEX_BIN and ATLAS_CODEX_PROBE_CWD"]
    fn installed_client_probe_uses_explicit_executable_and_cwd() {
        let executable = PathBuf::from(std::env::var_os("CODEX_BIN").expect("CODEX_BIN"));
        let cwd = PathBuf::from(
            std::env::var_os("ATLAS_CODEX_PROBE_CWD").expect("ATLAS_CODEX_PROBE_CWD"),
        );
        Client::probe(&executable, &cwd).unwrap_or_else(|error| panic!("{error}"));
    }
}
