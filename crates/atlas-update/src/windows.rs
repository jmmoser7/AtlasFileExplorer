use super::Event;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
use velopack::{
    sources::HttpSource, UpdateCheck, UpdateInfo, UpdateManager, UpdateOptions, VelopackApp,
};

const NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone)]
pub(super) struct Release {
    pub manager: UpdateManager,
    pub info: UpdateInfo,
}

#[derive(Deserialize)]
struct Distribution {
    channel: String,
}

pub(super) fn installed_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let current = exe.parent()?;
    let root = current.parent()?;
    (current.file_name()? == "current"
        && current.join("sq.version").is_file()
        && root.join("Update.exe").is_file())
    .then(|| root.to_owned())
}

pub(super) fn startup() -> Result<Option<File>, String> {
    VelopackApp::build()
        // Never let launching one app terminate another app with unsaved work.
        .set_auto_apply_on_startup(false)
        .on_after_install_fast_callback(|_| shortcuts(false))
        .on_after_update_fast_callback(|_| shortcuts(false))
        .on_before_uninstall_fast_callback(|_| shortcuts(true))
        .run();
    let Some(root) = installed_root() else {
        return Ok(None);
    };
    let start = Instant::now();
    loop {
        // Windows share modes, rather than advisory locks, also interoperate
        // with the small PowerShell coordinator outside the replaceable folder.
        match running_guard(&root) {
            Ok(file) => return Ok(Some(file)),
            Err(_) if start.elapsed() < Duration::from_secs(15) => std::thread::sleep(Duration::from_millis(200)),
            Err(error) => return Err(format!("An update is being installed, or the installation is not writable. Please reopen the app shortly.\n{error}")),
        }
    }
}

fn running_guard(root: &std::path::Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(3)
        .open(root.join("atlas-running.lock"))
}

fn shortcuts(remove: bool) {
    if let Some(root) = installed_root() {
        let _ = Command::new("powershell.exe")
            .creation_flags(NO_WINDOW)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(root.join("current/install-shortcuts.ps1"))
            .arg("-InstallRoot")
            .arg(&root)
            .arg("-Remove")
            .arg(if remove { "1" } else { "0" })
            .status();
    }
}

pub(super) fn check() -> Result<Event, String> {
    let root = installed_root().ok_or("This is not an installed copy of Atlas.")?;
    let previous_error = root.join("atlas-update-error.txt");
    if let Ok(error) = fs::read_to_string(&previous_error) {
        let _ = fs::remove_file(previous_error);
        return Ok(Event::ApplyFailed(error));
    }
    let config: Distribution = serde_json::from_slice(
        &fs::read(root.join("current/atlas-release.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Fixed, public HTTPS origins only. Never ship a personal GitHub token.
    let url = feed_url(&config.channel)?;
    let manager = UpdateManager::new(
        HttpSource::new(url),
        Some(UpdateOptions {
            ExplicitChannel: Some(config.channel.clone()),
            MaximumDeltasBeforeFallback: -1,
            ..Default::default()
        }),
        None,
    )
    .map_err(|e| e.to_string())?;
    let version = manager.get_current_version_as_string();
    let release = match manager.check_for_updates().map_err(|e| e.to_string())? {
        UpdateCheck::UpdateAvailable(info) => {
            validate_package(
                &info.TargetFullRelease.FileName,
                &info.TargetFullRelease.SHA256,
            )?;
            if info.TargetFullRelease.PackageId != manager.get_app_id() {
                return Err("The release feed belongs to a different application.".into());
            }
            Some(Release {
                manager,
                info: *info,
            })
        }
        _ => None,
    };
    Ok(Event::Checked(version, config.channel, release))
}

fn feed_url(channel: &str) -> Result<&'static str, String> {
    match channel {
        "stable" => Ok("https://github.com/jmmoser7/AtlasFileExplorer/releases/latest/download"),
        "preview" => Ok("https://github.com/jmmoser7/AtlasFileExplorer/releases/download/preview"),
        _ => Err("Unknown update channel; reinstall from the official Releases page.".into()),
    }
}

#[derive(Serialize)]
struct InstallRequest {
    package: String,
    sha256: String,
    size: u64,
    restart: String,
}

pub(super) fn schedule(release: &Release) -> Result<Event, String> {
    let root = installed_root().ok_or("This is not an installed copy of Atlas.")?;
    let asset = &release.info.TargetFullRelease;
    validate_package(&asset.FileName, &asset.SHA256)?;
    let restart = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .file_name()
        .ok_or("Missing app filename")?
        .to_string_lossy()
        .into_owned();
    let request = InstallRequest {
        package: asset.FileName.clone(),
        sha256: asset.SHA256.clone(),
        size: asset.Size,
        restart,
    };
    // These are local application-management files, outside both the replaced
    // current directory and the user's workbooks/settings/cache directory.
    let script = root.join("atlas-install-update.ps1");
    fs::write(&script, include_str!("install-update.ps1")).map_err(|e| e.to_string())?;
    let request_path = root.join(format!("atlas-update-{}.json", std::process::id()));
    fs::write(
        &request_path,
        serde_json::to_vec(&request).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let ready = request_path.with_extension("ready");
    let _ = fs::remove_file(&ready);
    let mut child = Command::new("powershell.exe")
        .creation_flags(NO_WINDOW)
        .current_dir(&root)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .arg("-InstallRoot")
        .arg(root)
        .arg("-RequestPath")
        .arg(request_path)
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if ready.is_file() {
            return Ok(Event::Scheduled);
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("Could not schedule the update. Another update may already be waiting; see atlas-update-error.txt in the installation folder.".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    Err("The update coordinator did not start. Please try again.".into())
}

fn validate_package(name: &str, hash: &str) -> Result<(), String> {
    if name.is_empty() || name.contains(['/', '\\', ':']) || !name.ends_with("-full.nupkg") {
        return Err("Invalid update package name.".into());
    }
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("The update is missing its SHA-256 checksum.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_running_apps_must_exit_before_installation_and_new_apps_are_excluded() {
        let root = std::env::temp_dir().join(format!(
            "atlas-update-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let exclusive = || {
            OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(root.join("atlas-running.lock"))
        };
        let slate = running_guard(&root).unwrap();
        let atlas = running_guard(&root).unwrap();
        assert!(exclusive().is_err());
        drop(slate);
        assert!(exclusive().is_err());
        drop(atlas);
        let installer = exclusive().unwrap();
        assert!(running_guard(&root).is_err());
        drop(installer);
        drop(running_guard(&root).unwrap());
        fs::remove_file(root.join("atlas-running.lock")).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn channels_cannot_redirect_to_an_untrusted_server() {
        assert!(feed_url("https://example.org").is_err());
        assert!(feed_url("stable").unwrap().contains("/latest/download"));
        assert!(feed_url("preview").unwrap().ends_with("/preview"));
    }
    #[test]
    fn rejects_traversal_and_missing_checksum() {
        let hash = "a".repeat(64);
        assert!(validate_package("AtlasSuite-0.1.0-full.nupkg", &hash).is_ok());
        for path in [
            "../evil-full.nupkg",
            "C:\\evil-full.nupkg",
            "sub/evil-full.nupkg",
        ] {
            assert!(validate_package(path, &hash).is_err());
        }
        assert!(validate_package("AtlasSuite-0.1.0-full.nupkg", "").is_err());
    }
}
