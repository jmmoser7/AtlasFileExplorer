//! `cargo xtask <command>` — workspace automation.

use std::path::PathBuf;
use std::process::ExitCode;

use xtask::{collect, verify_workspace_root, write_artifacts};

const USAGE: &str = "usage: cargo xtask metrics";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("metrics") => metrics(),
        Some(other) => Err(format!("unknown command `{other}`\n{USAGE}").into()),
        None => Err(USAGE.into()),
    }
}

fn metrics() -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = std::env::current_dir()?;
    verify_workspace_root(&root)?;

    let snapshot = collect(&root)?;
    write_artifacts(&root, &snapshot)?;

    println!(
        "docs/metrics/{}.json — {} crates, {} lines of code, pure ratio {:.3}",
        snapshot.date,
        snapshot.totals.crates,
        snapshot.totals.lines_code,
        snapshot.totals.pure_ratio
    );
    println!(
        "commands: slate {}, file-atlas {} · format_version {} · deviations open {}",
        snapshot.commands.slate,
        snapshot.commands.file_atlas,
        snapshot.model.format_version,
        snapshot.deviations.open
    );
    Ok(())
}
