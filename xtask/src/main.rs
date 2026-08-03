//! `cargo xtask <command>` — workspace automation.

use std::path::PathBuf;
use std::process::ExitCode;

use xtask::{
    audit_contracts, audit_kits, collect, render_contract_audit, render_kit_audit,
    verify_workspace_root, write_artifacts,
};

const USAGE: &str = "usage: cargo xtask <metrics | contracts | kits [dir]>";

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
        Some("contracts") => contracts(),
        Some("kits") => kits(std::env::args().nth(2)),
        Some(other) => Err(format!("unknown command `{other}`\n{USAGE}").into()),
        None => Err(USAGE.into()),
    }
}

/// Checks every `.slatekit` file: it parses, its grammars exist in this build,
/// and each recipe is something its grammar can actually produce. With no
/// argument it audits the committed tree; with one it audits that folder, which
/// is how a person checks a kit they just wrote.
fn kits(dir: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = match dir {
        Some(d) => PathBuf::from(d),
        None => {
            let root = std::env::current_dir()?;
            verify_workspace_root(&root)?;
            root
        }
    };

    let audit = audit_kits(&root)?;
    print!("{}", render_kit_audit(&audit));
    if audit.findings.is_empty() {
        Ok(())
    } else {
        Err(format!("{} kit finding(s)", audit.findings.len()).into())
    }
}

/// Checks `docs/keymap/contracts/` against itself: every in-scope dimension
/// answered, every matrix row mirrored in `decisions.json`, and no contract
/// claiming to be settled while a row is still proposed.
fn contracts() -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = std::env::current_dir()?;
    verify_workspace_root(&root)?;

    let audit = audit_contracts(&root)?;
    print!("{}", render_contract_audit(&audit));
    if audit.findings.is_empty() {
        Ok(())
    } else {
        Err(format!("{} contract finding(s)", audit.findings.len()).into())
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
