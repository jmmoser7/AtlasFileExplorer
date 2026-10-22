//! Native picker request configuration; the parent owns threading and modality.
use super::DialogOwner;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    File,
    Files,
    Folder,
    Folders,
    Save,
}

/// What to ask the user for. Every mode yields `Option<Vec<PathBuf>>`
/// (`None` = cancelled); single-path modes yield at most one path — see
/// [`super::first`].
#[derive(Clone, Debug)]
pub struct PickRequest {
    mode: Mode,
    title: Option<String>,
    filters: Vec<(String, Vec<String>)>,
    directory: Option<PathBuf>,
    file_name: Option<String>,
    confirm_overwrite: bool,
}

impl PickRequest {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            title: None,
            filters: Vec::new(),
            directory: None,
            file_name: None,
            confirm_overwrite: true,
        }
    }
    pub fn file() -> Self {
        Self::new(Mode::File)
    }
    pub fn files() -> Self {
        Self::new(Mode::Files)
    }
    pub fn folder() -> Self {
        Self::new(Mode::Folder)
    }
    pub fn folders() -> Self {
        Self::new(Mode::Folders)
    }
    pub fn save() -> Self {
        Self::new(Mode::Save)
    }
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    pub fn filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
        let extensions = extensions.iter().map(ToString::to_string).collect();
        self.filters.push((name.into(), extensions));
        self
    }
    pub fn directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.directory = Some(dir.into());
        self
    }
    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// Windows only. Keep enabled unless the save owner atomically avoids
    /// existing files itself; other platform backends retain native policy.
    pub fn confirm_overwrite(mut self, confirm: bool) -> Self {
        self.confirm_overwrite = confirm;
        self
    }

    /// Blocks the calling (picker) thread until the dialog closes.
    pub(super) fn run(self, owner: Option<DialogOwner>) -> Option<Vec<PathBuf>> {
        let mut dialog = rfd::FileDialog::new().set_confirm_overwrite(self.confirm_overwrite);
        if let Some(title) = self.title {
            dialog = dialog.set_title(title);
        }
        for (name, extensions) in &self.filters {
            dialog = dialog.add_filter(name, extensions);
        }
        if let Some(dir) = self.directory {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = self.file_name {
            dialog = dialog.set_file_name(name);
        }
        #[cfg(windows)]
        if let Some(owner) = &owner {
            dialog = dialog.set_parent(owner);
        }
        #[cfg(not(windows))]
        let _ = owner;
        match self.mode {
            Mode::File => dialog.pick_file().map(|p| vec![p]),
            Mode::Files => dialog.pick_files(),
            Mode::Folder => dialog.pick_folder().map(|p| vec![p]),
            Mode::Folders => dialog.pick_folders(),
            Mode::Save => dialog.save_file().map(|p| vec![p]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PickRequest;

    #[test]
    fn ordinary_picker_requests_keep_overwrite_confirmation() {
        for request in [
            PickRequest::file(),
            PickRequest::files(),
            PickRequest::folder(),
            PickRequest::folders(),
            PickRequest::save(),
        ] {
            assert!(request.confirm_overwrite);
        }
        assert!(
            !PickRequest::save()
                .confirm_overwrite(false)
                .confirm_overwrite
        );
    }
}
