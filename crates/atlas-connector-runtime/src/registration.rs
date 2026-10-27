//! Explicit machine-local launch registrations. Read and launch on workers.
//! A workbook names a registration, never an executable or argument string.
use crate::transport::{PipeAddress, PipeListener};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub executable: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub approved_roots: Vec<PathBuf>,
}

pub struct RegisteredLaunch {
    pub process_id: u32,
    pub listener: PipeListener,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

impl Registration {
    pub fn directory() -> io::Result<PathBuf> {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("NativeFileAtlas").join("application-connectors"))
            .ok_or_else(|| invalid("LOCALAPPDATA is unavailable"))
    }
    pub fn load(id: &str) -> io::Result<Self> {
        Self::load_at(&Self::directory()?, id)
    }
    pub fn load_at(directory: &Path, id: &str) -> io::Result<Self> {
        if !atlas_connector::valid_identifier(id) {
            return Err(invalid("invalid registration identity"));
        }
        let path = directory.join(format!("{id}.json"));
        if fs::metadata(&path)?.len() > 65_536 {
            return Err(invalid("registration exceeds 64 KiB"));
        }
        let registration: Self = serde_json::from_slice(&fs::read(path)?)
            .map_err(|e| invalid(&format!("invalid registration: {e}")))?;
        registration.validate()?;
        Ok(registration)
    }
    pub fn validate(&self) -> io::Result<()> {
        if !self.executable.is_absolute() || !self.executable.is_file() {
            return Err(invalid(
                "registered executable must be an existing absolute file",
            ));
        }
        if self.arguments.len() > 32
            || self.arguments.iter().any(|a| {
                a.len() > 4096
                    || a.contains('\0')
                    || ["--slate-token", "--slate-pid", "--source-root"]
                        .iter()
                        .any(|r| a == r || a.starts_with(&format!("{r}=")))
            })
        {
            return Err(invalid("unbounded or reserved registration argument"));
        }
        if self.approved_roots.is_empty()
            || self.approved_roots.len() > 32
            || self
                .approved_roots
                .iter()
                .any(|p| !p.is_absolute() || !p.is_dir())
        {
            return Err(invalid(
                "registration requires existing absolute approved source roots",
            ));
        }
        Ok(())
    }
    pub fn approved_root(&self, source_path: &Path) -> io::Result<PathBuf> {
        let source = match source_path.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                // A new isolated document may not exist yet. Resolve its real
                // parent first so a symlink cannot broaden approved roots.
                let name = source_path
                    .file_name()
                    .ok_or_else(|| invalid("new source requires a file name"))?;
                let parent = source_path
                    .parent()
                    .ok_or_else(|| invalid("new source requires a parent"))?;
                parent.canonicalize()?.join(name)
            }
            Err(error) => return Err(error),
        };
        for root in &self.approved_roots {
            let root = root.canonicalize()?;
            if source.starts_with(&root) {
                return Ok(root);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "selected source is outside registered roots",
        ))
    }
    pub fn launch(
        &self,
        address: PipeAddress,
        host_pid: u32,
        source_path: &Path,
    ) -> io::Result<RegisteredLaunch> {
        self.validate()?;
        let root = self.approved_root(source_path)?;
        if host_pid != std::process::id() {
            return Err(invalid("launch host PID differs from current process"));
        }
        #[cfg(windows)]
        {
            launch_windows(self, address, host_pid, &root)
        }
        #[cfg(not(windows))]
        {
            let _ = (address, root);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "connector launch requires Windows",
            ))
        }
    }
}

/// Windows CommandLineToArgvW/CRT quoting, with no shell interpreter.
#[cfg(any(windows, test))]
fn quote(argument: &str) -> String {
    let mut out = String::from("\"");
    let mut slashes = 0;
    for ch in argument.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        if ch == '"' {
            out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat_n('\\', slashes * 2));
    out.push('"');
    out
}

#[cfg(windows)]
fn launch_windows(
    registration: &Registration,
    address: PipeAddress,
    host_pid: u32,
    root: &Path,
) -> io::Result<RegisteredLaunch> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        CreateProcessW, ResumeThread, TerminateProcess, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        PROCESS_INFORMATION, STARTUPINFOW,
    };
    let exe = registration.executable.to_string_lossy().into_owned();
    let mut args = vec![exe.clone()];
    args.extend(registration.arguments.clone());
    args.extend([
        "--slate-token".into(),
        address.token().into(),
        "--slate-pid".into(),
        host_pid.to_string(),
        "--source-root".into(),
        root.to_string_lossy().into_owned(),
    ]);
    let mut command: Vec<u16> = args
        .iter()
        .map(|a| quote(a))
        .collect::<Vec<_>>()
        .join(" ")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let wide_exe: Vec<u16> = exe.encode_utf16().chain(Some(0)).collect();
    let cwd: Vec<u16> = registration
        .executable
        .parent()
        .expect("absolute executable")
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR(wide_exe.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_SUSPENDED | CREATE_NO_WINDOW,
            None,
            PCWSTR(cwd.as_ptr()),
            &startup,
            &mut process,
        )
    }
    .map_err(io::Error::other)?;
    // Only this brand-new suspended helper may be terminated on setup failure.
    // Once resumed, handle release never closes a document or kills an app.
    let listener = match PipeListener::bind(address, process.dwProcessId) {
        Ok(listener) => listener,
        Err(error) => {
            unsafe {
                let _ = TerminateProcess(process.hProcess, 1);
                let _ = CloseHandle(process.hThread);
                let _ = CloseHandle(process.hProcess);
            }
            return Err(error);
        }
    };
    let resumed = unsafe { ResumeThread(process.hThread) };
    if resumed == u32::MAX {
        let error = io::Error::last_os_error();
        unsafe {
            let _ = TerminateProcess(process.hProcess, 1);
            let _ = CloseHandle(process.hThread);
            let _ = CloseHandle(process.hProcess);
        }
        return Err(error);
    }
    unsafe {
        let _ = CloseHandle(process.hThread);
        let _ = CloseHandle(process.hProcess);
    }
    Ok(RegisteredLaunch {
        process_id: process.dwProcessId,
        listener,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_preserve_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote("C:\\a b\\"), "\"C:\\a b\\\\\"");
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
    }
    #[test]
    fn prospective_draft_stays_within_approved_canonical_parent() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!(
            "slate-registration-{}-{unique}",
            std::process::id()
        ));
        let approved = temp.join("approved");
        let outside = temp.join("outside");
        fs::create_dir_all(&approved).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let registration = Registration {
            executable: std::env::current_exe().unwrap(),
            arguments: vec![],
            approved_roots: vec![approved.clone()],
        };
        registration.validate().unwrap();
        assert_eq!(
            registration
                .approved_root(&approved.join("new.3dm"))
                .unwrap(),
            approved.canonicalize().unwrap()
        );
        assert_eq!(
            registration
                .approved_root(&outside.join("new.3dm"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(registration
            .approved_root(&approved.join("missing-parent").join("new.3dm"))
            .is_err());
        let mut reserved = registration.clone();
        reserved.arguments.push("--source-root=outside".into());
        assert!(reserved.validate().is_err());
        fs::remove_dir(&approved).unwrap();
        fs::remove_dir(&outside).unwrap();
        fs::remove_dir(&temp).unwrap();
    }

    #[test]
    fn registration_id_cannot_escape_its_directory() {
        assert!(Registration::load_at(Path::new("."), "../evil").is_err());
        assert!(Registration::load_at(Path::new("."), "C:\\evil").is_err());
    }
}
