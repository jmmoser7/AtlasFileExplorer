//! Human-directed filesystem edits for File Atlas.
//!
//! These operations deliberately run off the frame loop. The app owns the UI
//! confirmations and journal labels; this module owns the blocking disk work
//! and reports exactly which paths changed.

use crossbeam_channel::{unbounded, Receiver};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum FsOp {
    Rename {
        path: PathBuf,
        new_name: String,
    },
    Move {
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
    },
    Copy {
        sources: Vec<PathBuf>,
        dest_dir: PathBuf,
    },
    NewDir {
        parent: PathBuf,
        name: String,
    },
    Delete {
        paths: Vec<PathBuf>,
        permanent: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FsOutcome {
    Renamed { from: PathBuf, to: PathBuf },
    Moved { from: PathBuf, to: PathBuf },
    Copied { from: PathBuf, to: PathBuf },
    CreatedDir { path: PathBuf },
    Deleted { path: PathBuf, recycled: bool },
    Skipped { path: PathBuf, reason: String },
}

#[derive(Clone, Debug, Default)]
pub struct FsOpResult {
    pub outcomes: Vec<FsOutcome>,
}

impl FsOpResult {
    pub fn changed_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| !matches!(o, FsOutcome::Skipped { .. }))
            .count()
    }
}

#[derive(Clone, Debug)]
pub enum FsOpMsg {
    Progress { done: usize, total: usize },
    Done(FsOpResult),
    Failed(String),
}

pub struct FsOpHandle {
    pub rx: Receiver<FsOpMsg>,
}

pub fn start(op: FsOp) -> FsOpHandle {
    let (tx, rx) = unbounded();
    std::thread::spawn(move || {
        let total = op_len(&op);
        match run(op, |done| {
            let _ = tx.send(FsOpMsg::Progress { done, total });
        }) {
            Ok(result) => {
                let _ = tx.send(FsOpMsg::Done(result));
            }
            Err(err) => {
                let _ = tx.send(FsOpMsg::Failed(err));
            }
        }
    });
    FsOpHandle { rx }
}

fn op_len(op: &FsOp) -> usize {
    match op {
        FsOp::Rename { .. } | FsOp::NewDir { .. } => 1,
        FsOp::Move { sources, .. }
        | FsOp::Copy { sources, .. }
        | FsOp::Delete { paths: sources, .. } => sources.len(),
    }
}

fn run(op: FsOp, mut progress: impl FnMut(usize)) -> Result<FsOpResult, String> {
    let mut result = FsOpResult::default();
    match op {
        FsOp::Rename { path, new_name } => {
            let to = path
                .parent()
                .ok_or_else(|| format!("{} has no parent directory", path.display()))?
                .join(new_name);
            if to.exists() {
                result.outcomes.push(FsOutcome::Skipped {
                    path,
                    reason: "destination already exists".into(),
                });
            } else {
                std::fs::rename(&path, &to).map_err(|e| format!("rename failed: {e}"))?;
                result.outcomes.push(FsOutcome::Renamed { from: path, to });
            }
            progress(1);
        }
        FsOp::Move { sources, dest_dir } => {
            std::fs::create_dir_all(&dest_dir)
                .map_err(|e| format!("could not create {}: {e}", dest_dir.display()))?;
            for (i, src) in sources.into_iter().enumerate() {
                let to = dest_dir.join(src.file_name().unwrap_or_default());
                if to.exists() {
                    result.outcomes.push(FsOutcome::Skipped {
                        path: src,
                        reason: "destination already exists".into(),
                    });
                } else {
                    match std::fs::rename(&src, &to) {
                        Ok(()) => result.outcomes.push(FsOutcome::Moved { from: src, to }),
                        Err(e) => result.outcomes.push(FsOutcome::Skipped {
                            path: src,
                            reason: e.to_string(),
                        }),
                    }
                }
                progress(i + 1);
            }
        }
        FsOp::Copy { sources, dest_dir } => {
            std::fs::create_dir_all(&dest_dir)
                .map_err(|e| format!("could not create {}: {e}", dest_dir.display()))?;
            for (i, src) in sources.into_iter().enumerate() {
                let to = dest_dir.join(src.file_name().unwrap_or_default());
                if to.exists() {
                    result.outcomes.push(FsOutcome::Skipped {
                        path: src,
                        reason: "destination already exists".into(),
                    });
                } else {
                    match copy_any(&src, &to) {
                        Ok(()) => result.outcomes.push(FsOutcome::Copied { from: src, to }),
                        Err(e) => result.outcomes.push(FsOutcome::Skipped {
                            path: src,
                            reason: e.to_string(),
                        }),
                    }
                }
                progress(i + 1);
            }
        }
        FsOp::NewDir { parent, name } => {
            let path = parent.join(name);
            if path.exists() {
                result.outcomes.push(FsOutcome::Skipped {
                    path,
                    reason: "folder already exists".into(),
                });
            } else {
                std::fs::create_dir(&path)
                    .map_err(|e| format!("could not create {}: {e}", path.display()))?;
                result.outcomes.push(FsOutcome::CreatedDir { path });
            }
            progress(1);
        }
        FsOp::Delete { paths, permanent } if permanent => {
            for (i, path) in paths.into_iter().enumerate() {
                match delete_permanent(&path) {
                    Ok(()) => result.outcomes.push(FsOutcome::Deleted {
                        path,
                        recycled: false,
                    }),
                    Err(e) => result.outcomes.push(FsOutcome::Skipped {
                        path,
                        reason: e.to_string(),
                    }),
                }
                progress(i + 1);
            }
        }
        FsOp::Delete { paths, .. } => {
            // The Recycle Bin is a shell concept, so the whole batch goes to
            // the shell in one call. Whether each path actually went is then
            // read back from the filesystem rather than assumed.
            let total = paths.len();
            let failure = recycle(&paths).err().map(|e| e.to_string());
            for path in paths {
                if path.exists() {
                    result.outcomes.push(FsOutcome::Skipped {
                        path,
                        reason: failure
                            .clone()
                            .unwrap_or_else(|| "the shell left it in place".into()),
                    });
                } else {
                    result.outcomes.push(FsOutcome::Deleted {
                        path,
                        recycled: true,
                    });
                }
            }
            progress(total);
        }
    }
    Ok(result)
}

fn copy_any(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        copy_dir(from, to)
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        copy_any(&src, &dst)?;
    }
    Ok(())
}

fn delete_permanent(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Send a batch of paths to the Recycle Bin through the shell.
#[cfg(windows)]
fn recycle(paths: &[PathBuf]) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_NOERRORUI,
        FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
    };

    if paths.is_empty() {
        return Ok(());
    }
    // `pFrom` is a double-null-terminated list of full paths.
    let mut from: Vec<u16> = Vec::new();
    for path in paths {
        if !path.is_absolute() {
            return Err(std::io::Error::other(format!(
                "{} is not an absolute path",
                path.display()
            )));
        }
        from.extend(path.as_os_str().encode_wide());
        from.push(0);
    }
    from.push(0);

    unsafe {
        // Shell file operations expect an initialized apartment; this runs on
        // the op's own worker thread, which has none yet.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let mut op = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: windows::core::PCWSTR(from.as_ptr()),
            fFlags: (FOF_ALLOWUNDO.0
                | FOF_NOCONFIRMATION.0
                | FOF_NOCONFIRMMKDIR.0
                | FOF_NOERRORUI.0
                | FOF_SILENT.0) as u16,
            ..Default::default()
        };
        let code = SHFileOperationW(&mut op);
        if code != 0 {
            return Err(std::io::Error::other(format!(
                "the shell refused the delete (0x{code:x})"
            )));
        }
        if op.fAnyOperationsAborted.as_bool() {
            return Err(std::io::Error::other("the delete was aborted"));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn recycle(_paths: &[PathBuf]) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the Recycle Bin is only available on Windows",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "atlas_fsops_{name}_{}_{}",
            std::process::id(),
            stamp
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run_to_done(op: FsOp) -> FsOpResult {
        let handle = start(op);
        loop {
            match handle.rx.recv().unwrap() {
                FsOpMsg::Done(result) => return result,
                FsOpMsg::Failed(err) => panic!("{err}"),
                FsOpMsg::Progress { .. } => {}
            }
        }
    }

    #[test]
    fn rename_reports_old_and_new_paths() {
        let dir = temp_dir("rename");
        let from = dir.join("old.txt");
        std::fs::write(&from, "hello").unwrap();

        let result = run_to_done(FsOp::Rename {
            path: from.clone(),
            new_name: "new.txt".into(),
        });

        let to = dir.join("new.txt");
        assert_eq!(
            result.outcomes,
            vec![FsOutcome::Renamed {
                from: from.clone(),
                to: to.clone()
            }]
        );
        assert!(!from.exists());
        assert!(to.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The Recycle Bin is the default delete, so the thing worth proving is
    /// that the file is actually gone afterwards — an earlier shell-out
    /// reported success while leaving every file in place.
    #[cfg(windows)]
    #[test]
    fn recycle_bin_delete_removes_the_file() {
        let dir = temp_dir("recycle");
        let file = dir.join("bin-me.txt");
        std::fs::write(&file, "hello").unwrap();

        let result = run_to_done(FsOp::Delete {
            paths: vec![file.clone()],
            permanent: false,
        });

        assert_eq!(
            result.outcomes,
            vec![FsOutcome::Deleted {
                path: file.clone(),
                recycled: true
            }]
        );
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn copy_move_newdir_and_permanent_delete_report_changes() {
        let dir = temp_dir("ops");
        let src = dir.join("src.txt");
        let copy_dest = dir.join("copies");
        let move_dest = dir.join("moved");
        std::fs::write(&src, "hello").unwrap();

        let copy = run_to_done(FsOp::Copy {
            sources: vec![src.clone()],
            dest_dir: copy_dest.clone(),
        });
        assert_eq!(copy.changed_count(), 1);
        assert!(copy_dest.join("src.txt").exists());

        let mv = run_to_done(FsOp::Move {
            sources: vec![src.clone()],
            dest_dir: move_dest.clone(),
        });
        assert_eq!(mv.changed_count(), 1);
        assert!(!src.exists());
        assert!(move_dest.join("src.txt").exists());

        let mk = run_to_done(FsOp::NewDir {
            parent: dir.clone(),
            name: "child".into(),
        });
        assert_eq!(mk.changed_count(), 1);
        assert!(dir.join("child").is_dir());

        let del = run_to_done(FsOp::Delete {
            paths: vec![copy_dest.join("src.txt")],
            permanent: true,
        });
        assert_eq!(del.changed_count(), 1);
        assert!(!copy_dest.join("src.txt").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
