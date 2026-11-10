//! One owned native child; line decoding and cancellation never block the UI.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};
pub(crate) const MAX_EVENT_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const RESPONSE_TIMEOUT: &str =
    "The subscription CLI response timed out. Retry after checking the provider connection.";
const EVENT_QUEUE_DEPTH: usize = 8;

pub(crate) struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Receiver<Result<Value, String>>,
}
impl Process {
    pub(crate) fn spawn(mut command: Command) -> Result<Self, String> {
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
            .map_err(|error| format!("Could not start the subscription CLI: {error}. Check the native installation and Windows execution policy."))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        // Image tool results may legally include up to10MB encoded images.
        // Keep aggregate queued bytes below the previous4MiB×128 bound.
        let (tx, messages) = mpsc::sync_channel(EVENT_QUEUE_DEPTH);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                // A hard per-message cap protects against malformed/unbounded output.
                let mut bytes = Vec::new();
                match reader
                    .by_ref()
                    .take(MAX_EVENT_BYTES as u64 + 1)
                    .read_until(b'\n', &mut bytes)
                {
                    Ok(0) => break,
                    Ok(_) if bytes.len() > MAX_EVENT_BYTES => {
                        let _ = tx.send(Err(
                            "Subscription CLI output exceeded the message limit.".into()
                        ));
                        break;
                    }
                    Ok(_) => {
                        if bytes.iter().all(u8::is_ascii_whitespace) {
                            continue;
                        }
                        let parsed = serde_json::from_slice(&bytes).map_err(|_| "Subscription CLI returned an unsupported protocol message. Update the native CLI.".into());
                        if tx.send(parsed).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(Err("Could not read subscription CLI output.".into()));
                        break;
                    }
                }
            }
        });
        // Drain stderr without retaining account identity or possibly private diagnostics.
        std::thread::spawn(move || {
            let mut stderr = stderr;
            let mut bytes = [0; 4096];
            while stderr.read(&mut bytes).is_ok_and(|n| n != 0) {}
        });
        Ok(Self {
            child,
            stdin,
            messages,
        })
    }
    pub(crate) fn write(&mut self, text: &str) -> Result<(), String> {
        let mut stdin = self
            .stdin
            .take()
            .ok_or("The subscription CLI input is closed.")?;
        let bytes = text.as_bytes().to_vec();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = stdin.write_all(&bytes).and_then(|_| stdin.flush());
            let _ = tx.send((stdin, result));
        });
        match rx.recv_timeout(Duration::from_secs(12)) {
            Ok((stdin, result)) => {
                self.stdin = Some(stdin);
                result.map_err(|_| "Could not send the request to the subscription CLI.".into())
            }
            Err(_) => Err(
                "The subscription CLI did not accept its input within the startup limit.".into(),
            ),
        }
    }
    pub(crate) fn send(&mut self, value: &Value) -> Result<(), String> {
        self.write(&format!("{value}\n"))
    }
    pub(crate) fn close_input(&mut self) {
        self.stdin.take();
    }
    pub(crate) fn check_status(&mut self) -> Result<(), String> {
        match self.child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                Err("The subscription CLI exited with an error.".into())
            }
            Ok(_) => Ok(()),
            Err(error) => Err(format!("Could not check the subscription CLI: {error}")),
        }
    }
    pub(crate) fn receive(
        &mut self,
        stop: &AtomicBool,
        deadline: Instant,
    ) -> Result<Value, String> {
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("Response stopped.".into());
            }
            if Instant::now() >= deadline {
                return Err(RESPONSE_TIMEOUT.into());
            }
            match self.messages.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => return message,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err("The subscription CLI closed before completing its response. Check sign-in with the native CLI and update it if necessary.".into()),
            }
        }
    }
    pub(crate) fn finish(&mut self, stop: &AtomicBool, deadline: Instant) -> Result<(), String> {
        loop {
            if stop.load(Ordering::Relaxed) {
                return Err("Response stopped.".into());
            }
            match self.child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) => return Err("The subscription CLI failed. Check your sign-in, subscription limits and native CLI version.".into()),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
                Ok(None) => return Err("The subscription CLI did not exit after completing its response.".into()),
                Err(error) => return Err(format!("Could not check the subscription CLI process: {error}")),
            }
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        self.stdin.take();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill")
                .args(["/PID", &self.child.id().to_string(), "/T", "/F"])
                .creation_flags(0x0800_0000)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
