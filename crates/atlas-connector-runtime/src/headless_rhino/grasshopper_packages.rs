//! Restore missing Grasshopper packages from Rhino's configured repositories.
//! The host verifies plugin identity and distribution compatibility with Yak.
//! Only a verified package identity or an inspected owned archive goes to Yak.

mod local;
pub use local::{install_local_package, stage_local_package, LocalPackage, StagedLocalPackage};

use crate::transport::Cancellation;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::{Mutex, MutexGuard, TryLockError},
    thread,
    time::{Duration, Instant},
};

static YAK_INSTALL: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MissingPackage {
    pub library_id: Option<String>,
    pub name: String,
    pub version: Option<String>,
    pub component_ids: Vec<String>,
    pub already_loaded: bool,
}

/// Host response from Yak's GUID search and compatible-distribution check.
/// The host returns these in Yak version order, newest first.
#[derive(Debug, Clone, Deserialize)]
pub struct PackageCandidate {
    pub library_id: String,
    pub name: String,
    pub version: String,
    pub source: String,
    pub prerelease: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredPackage {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Default)]
pub struct RestoreReport {
    pub installed: Vec<RestoredPackage>,
    pub issues: Vec<String>,
}

impl RestoreReport {
    pub fn needs_restart(&self) -> bool {
        !self.installed.is_empty()
    }
}

/// Run on the Grasshopper worker's package-restore path, never on the UI thread.
/// Trusted sources come from the installed host's package settings response.
/// After an installation, start a fresh Slate-owned Grasshopper process and
/// run definition preflight again to verify the components actually load.
pub fn restore_missing_packages(
    rhino_system: &Path,
    trusted_sources: &[String],
    missing: &[MissingPackage],
    candidates: &[PackageCandidate],
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<RestoreReport, String> {
    let (plan, report) = plan_restoration(trusted_sources, missing, candidates);
    if plan.is_empty() {
        return Ok(report);
    }
    let yak = installed_yak(rhino_system)?;
    let _guard = lock_installer(cancel, deadline)?;
    apply_plan(plan, report, |args, interruptible| {
        run_yak(&yak, args, cancel, deadline, interruptible)
    })
}

fn installed_yak(rhino_system: &Path) -> Result<std::path::PathBuf, String> {
    let yak = rhino_system.join("Yak.exe");
    if !yak.is_absolute() || !yak.is_file() {
        return Err(
            "Rhino Package Manager (Yak.exe) was not found in the Rhino installation".into(),
        );
    }
    Ok(yak)
}

fn plan_restoration<'a>(
    trusted_sources: &[String],
    missing: &[MissingPackage],
    candidates: &'a [PackageCandidate],
) -> (BTreeMap<String, &'a PackageCandidate>, RestoreReport) {
    let mut plan: BTreeMap<String, &PackageCandidate> = BTreeMap::new();
    let mut conflicts = BTreeSet::new();
    let mut report = RestoreReport::default();
    for library in missing {
        let candidate = match choose_package(trusted_sources, library, candidates) {
            Ok(candidate) => candidate,
            Err(reason) => {
                let name = library.name.trim();
                let name = if name.is_empty() {
                    "Unknown library"
                } else {
                    name
                };
                let components = library.component_ids.join(", ");
                report
                    .issues
                    .push(format!("{name} ({components}): {reason}"));
                continue;
            }
        };
        let key = candidate.name.to_ascii_lowercase();
        if let Some(prior) = plan.get(&key) {
            if prior.version != candidate.version || prior.source != candidate.source {
                conflicts.insert(key);
            }
        } else {
            plan.insert(key, candidate);
        }
    }
    for name in conflicts {
        plan.remove(&name);
        report.issues.push(format!(
            "{name} has conflicting required versions or repositories. Resolve this in Rhino Package Manager"
        ));
    }
    (plan, report)
}

fn choose_package<'a>(
    trusted_sources: &[String],
    library: &MissingPackage,
    candidates: &'a [PackageCandidate],
) -> Result<&'a PackageCandidate, &'static str> {
    if library.already_loaded {
        return Err("The installed plug-in is missing components used by this definition");
    }
    let id = library
        .library_id
        .as_deref()
        .filter(|id| valid_guid(id))
        .ok_or(
            "This definition lacks a verified plug-in ID. Install the required library in Rhino",
        )?;
    let verified: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            candidate.library_id.eq_ignore_ascii_case(id)
                && valid_package_name(&candidate.name)
                && valid_version(&candidate.version)
                && trusted_sources.contains(&candidate.source)
        })
        .collect();
    let first = verified
        .first()
        .ok_or("No compatible package was found in Rhino's configured repositories")?;
    if verified.iter().any(|candidate| {
        !candidate.name.eq_ignore_ascii_case(&first.name) || candidate.source != first.source
    }) {
        return Err(
            "More than one package claims this plug-in ID. Choose it in Rhino Package Manager",
        );
    }
    if let Some(exact) = verified
        .iter()
        .find(|candidate| Some(candidate.version.as_str()) == library.version.as_deref())
    {
        return Ok(exact);
    }
    verified
        .into_iter()
        .find(|candidate| !candidate.prerelease)
        .ok_or("No matching version or compatible stable release was found")
}

fn valid_guid(id: &str) -> bool {
    id.len() == 36
        && id
            .bytes()
            .any(|byte| byte.is_ascii_hexdigit() && byte != b'0')
        && id.bytes().enumerate().all(|(position, byte)| {
            if matches!(position, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_version(version: &str) -> bool {
    version.len() <= 128
        && version.starts_with(|character: char| character.is_ascii_digit())
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}

fn apply_plan(
    plan: BTreeMap<String, &PackageCandidate>,
    mut report: RestoreReport,
    mut run: impl FnMut(&[&str], bool) -> Result<String, String>,
) -> Result<RestoreReport, String> {
    let mut installed = read_installed(&run(&["list"], true)?)?;
    for (key, candidate) in plan {
        if let Some(version) = installed.get(&key) {
            report.issues.push(format!(
                "{} {version} is already installed. Rhino could not load all required components",
                candidate.name
            ));
            continue;
        }
        let source = format!("--source={}", candidate.source);
        let args = ["install", &source, &candidate.name, &candidate.version];
        match run(&args, false) {
            Err(error) => {
                report.issues.push(format!("{}: {error}", candidate.name));
                break;
            }
            Ok(_) => match run(&["list"], false).and_then(|output| read_installed(&output)) {
                Ok(current) => {
                    installed = current;
                    if installed.get(&key) == Some(&candidate.version) {
                        report.installed.push(RestoredPackage {
                            name: candidate.name.clone(),
                            version: candidate.version.clone(),
                        });
                    } else {
                        report.issues.push(format!(
                            "{} {} did not appear in Rhino's installed packages",
                            candidate.name, candidate.version
                        ));
                    }
                }
                Err(error) => {
                    report
                        .issues
                        .push(format!("Cannot verify {}: {error}", candidate.name));
                    break;
                }
            },
        }
    }
    Ok(report)
}

fn read_installed(output: &str) -> Result<BTreeMap<String, String>, String> {
    let mut installed = BTreeMap::new();
    for line in output.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("Using package folder ") {
            continue;
        }
        let (name, version) = line
            .split_once(" (")
            .and_then(|(name, version)| version.strip_suffix(')').map(|version| (name, version)))
            .filter(|(name, version)| valid_package_name(name) && valid_version(version))
            .ok_or("Cannot read Rhino's installed packages. Existing packages were preserved")?;
        installed.insert(name.to_ascii_lowercase(), version.to_owned());
    }
    Ok(installed)
}

fn lock_installer(
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<MutexGuard<'static, ()>, String> {
    loop {
        check_deadline(cancel, deadline)?;
        match YAK_INSTALL.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::WouldBlock) => thread::sleep(Duration::from_millis(50)),
            Err(TryLockError::Poisoned(_)) => {
                return Err("Rhino package installer unavailable".into());
            }
        }
    }
}

fn check_deadline(cancel: &Cancellation, deadline: Instant) -> Result<(), String> {
    if cancel.is_cancelled() {
        return Err("Grasshopper package restoration was cancelled".into());
    }
    if Instant::now() >= deadline {
        return Err("Grasshopper package restoration timed out".into());
    }
    Ok(())
}

fn run_yak(
    yak: &Path,
    args: &[&str],
    cancel: &Cancellation,
    deadline: Instant,
    interruptible: bool,
) -> Result<String, String> {
    const OUTPUT_LIMIT: u64 = 256 * 1024;
    if interruptible || args[0] == "install" {
        check_deadline(cancel, deadline)?;
    } else if Instant::now() >= deadline {
        return Err("Grasshopper package restoration timed out".into());
    }
    let mut command = Command::new(yak);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Cannot launch Rhino Package Manager: {error}"))?;
    let stdout = child.stdout.take().expect("Yak stdout is piped");
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.take(OUTPUT_LIMIT + 1).read_to_end(&mut output)?;
        Ok::<_, std::io::Error>(output)
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline && !(interruptible && cancel.is_cancelled()) => {
                thread::sleep(Duration::from_millis(50));
            }
            status => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(match status {
                    Err(error) => format!("Cannot monitor Rhino Package Manager: {error}"),
                    Ok(_) if interruptible && cancel.is_cancelled() => {
                        "Grasshopper package restoration was cancelled".into()
                    }
                    Ok(_) => {
                        "Rhino Package Manager timed out. Check installed packages before retrying"
                            .into()
                    }
                });
            }
        }
    };
    let status = status?;
    let output = reader
        .join()
        .map_err(|_| "Cannot read Rhino Package Manager output")?
        .map_err(|error| format!("Cannot read Rhino Package Manager output: {error}"))?;
    if !status.success() {
        return Err(format!(
            "Rhino Package Manager {} failed ({status})",
            args[0]
        ));
    }
    if output.len() as u64 > OUTPUT_LIMIT {
        return Err("Rhino Package Manager output exceeded its limit".into());
    }
    // The folder heading can contain characters from the Windows console
    // code page. Package names and versions in the listing are ASCII.
    Ok(String::from_utf8_lossy(&output).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-1111-1111-1111-111111111111";
    const SOURCE: &str = "https://yak.rhino3d.com/";

    fn missing(version: &str) -> MissingPackage {
        MissingPackage {
            library_id: Some(ID.into()),
            name: "Library".into(),
            version: Some(version.into()),
            component_ids: vec!["component-guid".into()],
            already_loaded: false,
        }
    }

    fn candidate(version: &str) -> PackageCandidate {
        PackageCandidate {
            library_id: ID.into(),
            name: "Library-Yak".into(),
            version: version.into(),
            source: SOURCE.into(),
            prerelease: version.contains('-'),
        }
    }

    #[test]
    fn installs_exact_verified_version_and_requires_restart() {
        let candidates = [candidate("2.0.0"), candidate("1.5.0")];
        let (plan, report) = plan_restoration(&[SOURCE.into()], &[missing("1.5.0")], &candidates);
        let mut calls = 0;
        let report = apply_plan(plan, report, |args, _| {
            calls += 1;
            match calls {
                1 => assert_eq!(args, ["list"]),
                2 => assert_eq!(
                    args,
                    [
                        "install",
                        "--source=https://yak.rhino3d.com/",
                        "Library-Yak",
                        "1.5.0"
                    ]
                ),
                3 => {
                    return Ok(
                        "Using package folder C:\\Rhino\\packages\nLibrary-Yak (1.5.0)\n".into(),
                    );
                }
                _ => panic!("Unexpected invocation"),
            }
            Ok(String::new())
        })
        .unwrap();
        assert!(report.needs_restart());
        assert_eq!(report.installed[0].version, "1.5.0");
    }

    #[test]
    fn preserves_yak_installed_and_manually_loaded_versions() {
        let candidates = [candidate("2.0.0")];
        let (plan, report) = plan_restoration(&[SOURCE.into()], &[missing("2.0.0")], &candidates);
        let report = apply_plan(plan, report, |args, _| {
            assert_eq!(args, ["list"]);
            Ok("library-yak (1.0.0)\n".into())
        })
        .unwrap();
        assert!(!report.needs_restart());
        assert!(report.issues[0].contains("1.0.0 is already installed"));
        let mut library = missing("2.0.0");
        library.already_loaded = true;
        assert!(plan_restoration(&[SOURCE.into()], &[library], &candidates)
            .0
            .is_empty());
    }

    #[test]
    fn selects_stable_fallback_and_rejects_untrusted_and_ambiguous_matches() {
        let mut untrusted = candidate("2.0.0");
        untrusted.source = "https://unconfigured.example/".into();
        let candidates = [candidate("3.0.0-beta"), untrusted, candidate("1.5.0")];
        let (plan, _) = plan_restoration(&[SOURCE.into()], &[missing("2.0.0")], &candidates);
        assert_eq!(plan["library-yak"].version, "1.5.0");
        let mut duplicate = candidate("1.5.0");
        duplicate.name = "Another".into();
        let candidates = [duplicate, candidate("1.5.0")];
        assert!(
            plan_restoration(&[SOURCE.into()], &[missing("1.5.0")], &candidates)
                .0
                .is_empty()
        );
    }

    #[test]
    fn missing_guid_or_conflicting_package_versions_cannot_install() {
        let candidates = [candidate("2.0.0"), candidate("1.5.0")];
        let libraries = [missing("1.5.0"), missing("2.0.0")];
        let (plan, report) = plan_restoration(&[SOURCE.into()], &libraries, &candidates);
        assert!(plan.is_empty());
        assert_eq!(report.issues.len(), 1);
        let mut library = missing("2.0.0");
        library.library_id = None;
        assert!(plan_restoration(&[SOURCE.into()], &[library], &candidates)
            .0
            .is_empty());
    }
}
