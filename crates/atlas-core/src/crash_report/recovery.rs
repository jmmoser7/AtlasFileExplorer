//! Reconcile interrupted supervisors before a new app overwrites session logs.
//! A reused live PID or any failure to inspect a process is treated as active.

#[cfg(any(windows, test))]
use super::{capture, RunReport};
#[cfg(any(windows, test))]
use std::fs::{self, File, OpenOptions};
use std::path::Path;

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessState {
    Dead,
    LiveOrUnknown,
}

pub(super) fn recover(root: &Path, session_dir: &Path) {
    #[cfg(windows)]
    recover_at(root, session_dir, process_state);
    #[cfg(not(windows))]
    let _ = (root, session_dir);
}

#[cfg(any(windows, test))]
fn recover_at(root: &Path, session_dir: &Path, state: impl Fn(u32) -> ProcessState) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten().filter(capture::is_real_dir) {
        let dir = entry.path();
        if !dir.join(".active").is_file() {
            continue;
        }
        // Windows sharing mode gives simultaneous new launches an OS-owned
        // claim. A monitor killed during recovery releases it automatically.
        let Some(_claim) = claim(&dir) else {
            continue;
        };
        let Some(run) = capture::read_run(&dir.join("run.json")) else {
            continue;
        };
        let Some(child_pid) = run.child_pid else {
            continue;
        };
        if state(run.monitor_pid) != ProcessState::Dead || state(child_pid) != ProcessState::Dead {
            continue;
        }
        let report = RunReport {
            dir,
            session_dir: session_dir.to_owned(),
            run,
        };
        let _ = report.finish_error(
            "Previous run ended without a recorded exit status; it may have been terminated or interrupted",
        );
    }
}

#[cfg(any(windows, test))]
fn claim(dir: &Path) -> Option<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(dir.join(".recovery-lock")).ok()
}

#[cfg(windows)]
fn process_state(pid: u32) -> ProcessState {
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    // Open/wait/close are bounded kernel calls; no process names, command lines,
    // memory or credentials are inspected. Permission denied is inconclusive.
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => {
                let exited = WaitForSingleObject(handle, 0) == WAIT_OBJECT_0;
                let _ = CloseHandle(handle);
                if exited {
                    ProcessState::Dead
                } else {
                    ProcessState::LiveOrUnknown
                }
            }
            Err(error)
                if error.code()
                    == windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) =>
            {
                ProcessState::Dead
            }
            Err(_) => ProcessState::LiveOrUnknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crash_report::tests::TempDir;

    #[test]
    fn an_interrupted_run_is_reported_only_after_both_processes_are_known_dead() {
        let temp = TempDir::new();
        let mut run = temp.start();
        run.run.monitor_pid = 101;
        run.run.child_pid = Some(202);
        run.write_metadata().unwrap();
        let dir = run.dir().to_owned();
        let root = dir.parent().unwrap();
        let session = temp.0.join("session");
        for live in [101, 202] {
            recover_at(root, &session, |pid| {
                if pid == live {
                    ProcessState::LiveOrUnknown
                } else {
                    ProcessState::Dead
                }
            });
            assert!(dir.join(".active").exists());
            assert!(!dir.join("report.json").exists());
        }
        recover_at(root, &session, |_| ProcessState::Dead);
        assert!(dir.join("report.json").exists());
        assert!(!dir.join(".active").exists());
        assert_eq!(temp.unread().len(), 1);
        assert!(capture::read_tail(&dir.join("report.json"), 8192)
            .unwrap()
            .contains("terminated or interrupted"));
    }

    #[test]
    fn missing_child_pid_cannot_be_recovered_safely() {
        let temp = TempDir::new();
        let run = temp.start();
        let dir = run.dir().to_owned();
        recover_at(dir.parent().unwrap(), &temp.0.join("session"), |_| {
            ProcessState::Dead
        });
        assert!(dir.join(".active").exists());
        assert!(!dir.join("report.json").exists());
    }

    #[cfg(windows)]
    #[test]
    fn current_process_is_never_recovered_as_dead() {
        assert_eq!(
            process_state(std::process::id()),
            ProcessState::LiveOrUnknown
        );
    }
}
