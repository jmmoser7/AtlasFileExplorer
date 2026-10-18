//! Discovery keeps each inactive run locked while validating its generations.
use super::*;
use std::collections::BTreeMap;

impl RecoveryStore {
    pub fn scan(&self) -> io::Result<RecoveryScan> {
        let mut scan = RecoveryScan::default();
        let directories = match fs::read_dir(&self.root) {
            Ok(directories) => directories,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(scan),
            Err(error) => return Err(error),
        };
        for directory in directories {
            let result = directory.and_then(|directory| {
                let session = directory.file_name().to_string_lossy().into_owned();
                let path = self.session_directory(&session)?;
                let Some(_guard) = inactive_guard(&path)? else {
                    return Ok(());
                };
                self.scan_session(&session, &path, &mut scan)
            });
            if let Err(error) = result {
                scan.issues
                    .push(format!("Recovery session could not be examined: {error}"));
            }
        }
        scan.entries
            .sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms));
        Ok(scan)
    }

    fn scan_session(&self, session: &str, path: &Path, scan: &mut RecoveryScan) -> io::Result<()> {
        let mut tabs = BTreeMap::<u64, Vec<PathBuf>>::new();
        for candidate in fs::read_dir(path)? {
            let candidate = match candidate {
                Ok(candidate) => candidate,
                Err(error) => {
                    scan.issues
                        .push(format!("Recovery entry could not be listed: {error}"));
                    continue;
                }
            };
            let name = candidate.file_name();
            let name = name.to_string_lossy();
            if name == GUARD || name.ends_with(".tmp") {
                continue;
            }
            if let Some(tab) = generation_tab(&name) {
                tabs.entry(tab).or_default().push(candidate.path());
            } else {
                scan.issues.push(format!(
                    "Unrecognized recovery entry retained: {session}/{name}"
                ));
            }
        }
        for (tab, mut paths) in tabs {
            paths.sort();
            let mut selected = None;
            let mut newer_failures = Vec::new();
            for path in paths.iter().rev() {
                match read_snapshot(path, session) {
                    Ok((metadata, _)) if selected.is_none() => {
                        let mut entry = RecoveryEntry::from(metadata);
                        if !newer_failures.is_empty() {
                            entry.warning = Some(format!(
                                "Using an earlier checkpoint; {} newer checkpoint(s) could not be read. Their files were retained.",
                                newer_failures.len()
                            ));
                        }
                        selected = Some(entry);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        if selected.is_none() {
                            newer_failures.push(error.to_string());
                        }
                        scan.issues.push(format!(
                            "Checkpoint retained for {session}, tab {tab} ({}): {error}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ));
                    }
                }
            }
            if let Some(entry) = selected {
                scan.entries.push(entry);
            }
        }
        Ok(())
    }
}
