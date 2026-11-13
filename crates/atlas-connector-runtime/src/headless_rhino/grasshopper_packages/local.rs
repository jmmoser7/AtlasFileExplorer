use super::{
    installed_yak, lock_installer, read_installed, run_yak, valid_guid, valid_package_name,
    valid_version, MissingPackage, RestoreReport, RestoredPackage,
};
use crate::transport::Cancellation;
use serde::Deserialize;
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Metadata from the Grasshopper worker's read-only archive inspection.
#[derive(Debug, Deserialize)]
pub struct LocalPackage {
    pub name: String,
    pub version: String,
    pub library_ids: Vec<String>,
}

/// The same owned copy is inspected and handed to Yak. Preserve the original
/// filename because Yak encodes the target Rhino and OS in that name.
pub struct StagedLocalPackage {
    path: PathBuf,
    directory: PathBuf,
}

impl StagedLocalPackage {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagedLocalPackage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.directory);
    }
}

/// Called only for a human-dropped local file, on the Grasshopper worker.
pub fn stage_local_package(source: &Path, workspace: &Path) -> Result<StagedLocalPackage, String> {
    let filename = source
        .file_name()
        .filter(|_| {
            source
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("yak"))
        })
        .ok_or("Drop an original Rhino Package Manager .yak file")?;
    if !source.is_absolute() || !source.is_file() || !workspace.is_absolute() {
        return Err("The dropped Rhino package or staging directory is unavailable".into());
    }
    let mut original = File::open(source).map_err(|error| error.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let directory = workspace.join(format!("local-yak-{}-{nonce}", std::process::id()));
    fs::create_dir_all(workspace).map_err(|error| error.to_string())?;
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let staged = StagedLocalPackage {
        path: directory.join(filename),
        directory,
    };
    let mut copy = File::create_new(staged.path()).map_err(|error| error.to_string())?;
    std::io::copy(&mut original, &mut copy).map_err(|error| error.to_string())?;
    Ok(staged)
}

pub fn install_local_package(
    rhino_system: &Path,
    staged: &StagedLocalPackage,
    package: &LocalPackage,
    missing: &[MissingPackage],
    cancel: &Cancellation,
    deadline: Instant,
) -> Result<RestoreReport, String> {
    validate_local_package(package, missing)?;
    let yak = installed_yak(rhino_system)?;
    let path = staged
        .path()
        .to_str()
        .ok_or("Unsupported dropped package path")?;
    let _guard = lock_installer(cancel, deadline)?;
    install_local(path, package, |args, interruptible| {
        run_yak(&yak, args, cancel, deadline, interruptible)
    })
}

fn validate_local_package(
    package: &LocalPackage,
    missing: &[MissingPackage],
) -> Result<(), String> {
    if !valid_package_name(&package.name)
        || !valid_version(&package.version)
        || package.library_ids.iter().any(|id| !valid_guid(id))
    {
        return Err("The dropped package has invalid Rhino manifest metadata".into());
    }
    let mut required = false;
    for library in missing {
        let Some(id) = &library.library_id else {
            continue;
        };
        if package
            .library_ids
            .iter()
            .any(|package_id| package_id.eq_ignore_ascii_case(id))
        {
            if library.already_loaded {
                return Err(format!(
                    "Rhino already loaded {}. The installed library was preserved",
                    library.name
                ));
            }
            required = true;
        }
    }
    if !required {
        return Err(
            "The dropped package does not provide any missing plug-in required by this definition"
                .into(),
        );
    }
    Ok(())
}

fn install_local(
    path: &str,
    package: &LocalPackage,
    mut run: impl FnMut(&[&str], bool) -> Result<String, String>,
) -> Result<RestoreReport, String> {
    let key = package.name.to_ascii_lowercase();
    let installed = read_installed(&run(&["list"], true)?)?;
    if let Some(version) = installed.get(&key) {
        return Err(format!(
            "{} {version} is already installed. The installed package was preserved",
            package.name
        ));
    }
    run(&["install", path], false)?;
    let installed = read_installed(&run(&["list"], false)?)?;
    if installed.get(&key) != Some(&package.version) {
        return Err(format!(
            "{} {} did not appear in Rhino's installed packages",
            package.name, package.version
        ));
    }
    Ok(RestoreReport {
        installed: vec![RestoredPackage {
            name: package.name.clone(),
            version: package.version.clone(),
        }],
        issues: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-1111-1111-1111-111111111111";

    fn package() -> LocalPackage {
        LocalPackage {
            name: "Library".into(),
            version: "1.5.0".into(),
            library_ids: vec![ID.into()],
        }
    }

    #[test]
    fn rejects_unrelated_packages_and_loaded_libraries() {
        let package = package();
        let mut missing = MissingPackage {
            library_id: Some(ID.into()),
            name: "Library".into(),
            version: None,
            component_ids: Vec::new(),
            already_loaded: false,
        };
        assert!(validate_local_package(&package, &[]).is_err());
        assert!(validate_local_package(&package, std::slice::from_ref(&missing)).is_ok());
        missing.already_loaded = true;
        assert!(validate_local_package(&package, &[missing]).is_err());
    }

    #[test]
    fn preserves_installed_package_before_local_install() {
        let report = install_local("local.yak", &package(), |args, _| {
            assert_eq!(args, ["list"]);
            Ok("Library (1.0.0)".into())
        });
        assert!(report.unwrap_err().contains("1.0.0 is already installed"));
    }

    #[test]
    fn hands_the_inspected_path_to_yak_and_checks_the_installed_version() {
        let mut calls = 0;
        let path = r"C:\Owned Stage\Library-1.5.0-rh8-win.yak";
        let report = install_local(path, &package(), |args, interruptible| {
            calls += 1;
            match calls {
                1 => assert_eq!((args, interruptible), (&["list"][..], true)),
                2 => assert_eq!((args, interruptible), (&["install", path][..], false)),
                3 => return Ok("Library (1.5.0)".into()),
                _ => panic!("Unexpected invocation"),
            }
            Ok(String::new())
        })
        .unwrap();
        assert!(report.needs_restart());
    }
}
