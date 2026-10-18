//! The launcher stays outside the GUI process so native faults cannot erase
//! the exit status and the last activity snapshot. No network or frame work.

use atlas_core::crash_report::{self, RunReport};
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

#[cfg(windows)]
mod native;
mod stderr;

const CHILD_REPORT: &str = "SLATE_CRASH_REPORT_CHILD";
const VERSION: &str = match option_env!("ATLAS_RELEASE_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

pub fn run() -> ExitCode {
    if let Some(dir) = std::env::var_os(CHILD_REPORT) {
        // A later Slate launch from this process must get its own monitor.
        std::env::remove_var(CHILD_REPORT);
        let dir = PathBuf::from(dir);
        crash_report::install_panic_hook(dir.clone());
        #[cfg(windows)]
        if let Err(error) = native::install(&dir) {
            eprintln!("Native exception details unavailable: {error}");
        }
        return run_window();
    }

    notify_previous_reports();
    let report = match RunReport::start("slate", VERSION) {
        Ok(report) => report,
        Err(error) => {
            message(
                "Crash reports unavailable",
                &format!("Slate could not create its local crash report folder.\n{error}\n\nSlate will open, but this run cannot save crash reports."),
            );
            return run_window();
        }
    };
    match monitor(report) {
        Ok(exit) => exit,
        Err(error) => {
            message("Slate could not start", &error.to_string());
            ExitCode::FAILURE
        }
    }
}

fn run_window() -> ExitCode {
    match slate::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Slate window failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn monitor(mut report: RunReport) -> io::Result<ExitCode> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(std::env::args_os().skip(1))
        .env(CHILD_REPORT, report.dir())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, GUI still opens.
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = report.finish_error(&format!("Could not launch Slate: {error}"));
            return Err(error);
        }
    };
    if let Err(error) = report.set_child_pid(child.id()) {
        eprintln!("Could not record Slate process identity: {error}");
    }
    let stderr = child.stderr.take().expect("piped child stderr");
    let output = report.stderr_path();
    // A bounded drain keeps diagnostic output from filling the disk or pipe.
    let (drained, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        stderr::capture(stderr, &output);
        let _ = drained.send(());
    });
    let exit = match child.wait() {
        Ok(exit) => exit,
        Err(error) => {
            let _ = report.finish_error(&format!("Could not observe Slate exit: {error}"));
            return Err(error);
        }
    };
    // Give the final diagnostic chunk time to land, without joining forever:
    // a spawned descendant may inherit the pipe and keep it open.
    let _ = finished.recv_timeout(std::time::Duration::from_millis(250));
    if let Err(error) = report.finish(exit) {
        message("Crash report could not be completed", &error.to_string());
    }
    Ok(if exit.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn notify_previous_reports() {
    let reports = match crash_report::unread_reports("slate") {
        Ok(reports) => reports,
        Err(error) => {
            eprintln!("Could not check previous crash reports: {error}");
            return;
        }
    };
    if reports.is_empty() {
        return;
    }
    let answer = rfd::MessageDialog::new()
        .set_title("Slate saved a crash report")
        .set_description(format!(
            "Slate saved {} new diagnostic report(s) after an unexpected exit or Rust panic.\n\nReports stay on this computer; nothing is uploaded. You can find them anytime in Advanced settings → Crash reports.\n\nOpen the report folder now?",
            reports.len()
        ))
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();
    if answer == rfd::MessageDialogResult::Yes {
        if let Err(error) = atlas_shell::crash_report::reveal("slate") {
            message("Could not open crash reports", &error.to_string());
        }
    }
    for report in reports {
        if let Err(error) = crash_report::acknowledge(&report.dir) {
            eprintln!("Could not acknowledge crash report: {error}");
        }
    }
}

fn message(title: &str, detail: &str) {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(detail)
        .show();
}
