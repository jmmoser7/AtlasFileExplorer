use super::{protocol, Job, VideoEvent};
use std::io;

pub(super) fn stdio() -> io::Result<()> {
    // This first read is also the parent's JobObject assignment handshake.
    let job = protocol::read_job(&mut io::stdin().lock())?;
    #[cfg(debug_assertions)]
    diagnostic_fault(&job)?;
    let (id, token) = job.key();
    let (sender, receiver) = crossbeam_channel::bounded(2);
    let writer = std::thread::Builder::new()
        .name("video-worker-output".into())
        .spawn(move || {
            let mut output = io::stdout().lock();
            for event in receiver {
                protocol::write_event(&mut output, &event)?;
            }
            Ok::<(), io::Error>(())
        })?;
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(job, &sender))).is_err() {
        sender
            .send(VideoEvent::Failed { id, token })
            .map_err(|_| io::Error::other("worker output closed"))?;
    }
    drop(sender);
    writer
        .join()
        .map_err(|_| io::Error::other("worker output panicked"))?
}

#[cfg(debug_assertions)]
fn diagnostic_fault(_job: &Job) -> io::Result<()> {
    match std::env::args()
        .find_map(|arg| arg.strip_prefix("--diagnostic-fault=").map(str::to_owned))
        .as_deref()
    {
        Some("hang") => loop {
            std::thread::park();
        },
        Some("crash") => std::process::abort(),
        Some("malformed") => {
            use std::io::Write;
            io::stdout().write_all(b"invalid packet")?;
            std::process::exit(2);
        }
        Some("exit") => std::process::exit(2),
        _ => Ok(()),
    }
}

#[cfg(not(windows))]
fn run(job: Job, sender: &crossbeam_channel::Sender<VideoEvent>) {
    let (id, token) = job.key();
    let _ = sender.send(VideoEvent::Failed { id, token });
}

#[cfg(windows)]
fn run(job: Job, sender: &crossbeam_channel::Sender<VideoEvent>) {
    use super::mf;
    use std::sync::{atomic::AtomicU64, Arc};
    use windows::Win32::{Media::MediaFoundation::*, System::Com::*};
    let (id, token) = job.key();
    if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
        let _ = sender.send(VideoEvent::Failed { id, token });
        return;
    }
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    let _com = Com;
    if unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }.is_err() {
        let _ = sender.send(VideoEvent::Failed { id, token });
        return;
    }
    struct MediaFoundation;
    impl Drop for MediaFoundation {
        fn drop(&mut self) {
            unsafe {
                let _ = MFShutdown();
            }
        }
    }
    let _mf = MediaFoundation;
    let mut reader = None;
    let (_unused_sender, unused_receiver) = crossbeam_channel::bounded(1);
    let playing = Arc::new(AtomicU64::new(if matches!(job, Job::Play { .. }) {
        token
    } else {
        0
    }));
    match &job {
        Job::Strip { .. } => {
            mf::run_strip(&unused_receiver, sender, &playing, &mut reader, &job);
        }
        Job::Play { .. } => mf::run_play(&unused_receiver, sender, &playing, &mut reader, job),
    }
    // Reader/codec teardown may hang too. The supervisor kills/reaps the helper
    // as soon as its terminal event arrives, so teardown cannot poison a pool.
    drop(reader);
}
