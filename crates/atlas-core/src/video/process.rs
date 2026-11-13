use super::protocol;
use super::{Job, VideoEvent};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;

#[cfg(windows)]
mod job_object;

/// One isolated decode request. All destruction happens on the supervisor,
/// never on the caller's frame loop. Killing also unblocks both pipe threads.
pub(super) struct Decoder {
    child: Child,
    events: Option<crossbeam_channel::Receiver<io::Result<VideoEvent>>>,
    pipes: Vec<JoinHandle<()>>,
    #[cfg(windows)]
    _job: Option<job_object::JobObject>,
}
impl Decoder {
    pub fn launch(job: Job, fault: Option<&str>) -> io::Result<Self> {
        let mut command = Command::new(worker_path()?);
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(
            if std::env::var_os("ATLAS_VIDEO_DIAGNOSTICS").is_some_and(|value| value == "1") {
                Stdio::inherit()
            } else {
                Stdio::null()
            },
        );
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW.
        }
        #[cfg(debug_assertions)]
        if let Some(fault) = fault {
            command.arg(format!("--diagnostic-fault={fault}"));
        }
        #[cfg(not(debug_assertions))]
        let _ = fault;
        Self::with_command(command, job)
    }
    fn with_command(mut command: Command, job: Job) -> io::Result<Self> {
        #[cfg(windows)]
        let guard = job_object::JobObject::new()?;
        let child = command.spawn()?;
        // The worker blocks on its first stdin packet. Do not send that packet
        // until assignment succeeds; no codec or MF initialization can run early.
        #[cfg(windows)]
        if let Err(error) = guard.assign(&child) {
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let (sender, events) = crossbeam_channel::bounded(2);
        let mut decoder = Self {
            child,
            events: Some(events),
            pipes: Vec::new(),
            #[cfg(windows)]
            _job: Some(guard),
        };
        let mut input = decoder
            .child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing worker input"))?;
        let mut output = decoder
            .child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing worker output"))?;
        let failures = sender.clone();
        decoder.pipes.push(
            std::thread::Builder::new()
                .name("video-input".into())
                .spawn(move || {
                    if let Err(error) = protocol::write_job(&mut input, &job) {
                        let _ = failures.send(Err(error));
                    }
                })?,
        );
        decoder.pipes.push(
            std::thread::Builder::new()
                .name("video-output".into())
                .spawn(move || loop {
                    let event = protocol::read_event(&mut output);
                    let failed = event.is_err();
                    if sender.send(event).is_err() || failed {
                        break;
                    }
                })?,
        );
        Ok(decoder)
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn receive(&self) -> Result<io::Result<VideoEvent>, crossbeam_channel::TryRecvError> {
        self.events
            .as_ref()
            .expect("live decoder event receiver")
            .try_recv()
    }
}
impl Drop for Decoder {
    fn drop(&mut self) {
        // kill/wait remains bounded by process termination even if a codec's
        // synchronous ReadSample or COM teardown never returns.
        let _ = self.child.kill();
        #[cfg(windows)]
        drop(self._job.take());
        let _ = self.child.wait();
        drop(self.events.take());
        for pipe in self.pipes.drain(..) {
            let _ = pipe.join();
        }
    }
}

fn worker_path() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("ATLAS_VIDEO_WORKER") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(io::Error::other("video worker path must be absolute"));
        }
        return Ok(path);
    }
    let current = std::env::current_exe()?;
    let directory = current
        .parent()
        .ok_or_else(|| io::Error::other("missing executable directory"))?;
    Ok(adjacent_worker(directory))
}

fn adjacent_worker(directory: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "slate-video-worker.exe"
    } else {
        "slate-video-worker"
    };
    let adjacent = directory.join(name);
    if adjacent.is_file() {
        return adjacent;
    }
    let profile = match directory.file_name().and_then(|name| name.to_str()) {
        Some("examples" | "deps") => directory.parent(),
        // Cargo 1.100 runs test binaries from build/<crate>/<hash>/out.
        Some("out") => directory
            .ancestors()
            .nth(3)
            .filter(|parent| parent.file_name().is_some_and(|name| name == "build"))
            .and_then(Path::parent)
            .filter(|profile| {
                profile.ancestors().skip(1).take(2).any(|parent| {
                    parent.file_name().is_some_and(|name| name == "target")
                        || parent.join(".rustc_info.json").is_file()
                })
            }),
        _ => None,
    };
    profile.map_or(adjacent, |profile| profile.join(name))
}

#[cfg(test)]
mod tests;
