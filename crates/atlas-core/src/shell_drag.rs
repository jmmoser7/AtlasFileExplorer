//! Dragging files *out* of Atlas into other applications.
//!
//! The point is that a drag from the Atlas canvas is indistinguishable, to the
//! receiving application, from a drag out of File Explorer: PowerPoint inserts
//! the picture, Explorer copies the file, Slate places the item. That is not
//! something a toolkit can fake — it is Win32 OLE drag-and-drop, and the
//! receiver decides what to do based on the shell data object it is handed.
//!
//! ## Why this blocks
//!
//! [`drag_out`] does not return until the user releases the button (and, worse,
//! until the *target* finishes its drop handler). That is inherent: `DoDragDrop`
//! runs its own modal loop, must be called from an STA thread, and delivers its
//! notifications relative to windows owned by the calling thread — so it cannot
//! be moved to a worker. Calling it anywhere else looks like it works and then
//! silently drops nothing.
//!
//! This is a deliberate, narrow exception to Atlas's "never block the frame
//! loop" rule: the block lasts exactly as long as a gesture the user is
//! physically performing, during which they are looking at the cursor and the
//! target application, not at our canvas. It is not background work. See
//! invariant 7 in `apps/file-atlas/src/app/ARCHITECTURE.md`.
//!
//! ## Copy and link, never move
//!
//! The offered effects are `COPY | LINK`. Move is deliberately withheld: a move
//! accepted by a foreign drop target would relocate the user's files behind
//! Atlas's back, with no journal entry and no undo, which Article VI forbids.
//! Files leave Atlas by copy, or by a journaled export.

use std::path::{Path, PathBuf};

/// What the drop target did with the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    /// The target copied them (the common case: PowerPoint, Explorer, Slate).
    Copied,
    /// The target took a shortcut/link instead.
    Linked,
    /// Dropped on nothing, or cancelled with Escape.
    Cancelled,
    /// The data object could not be built — nothing was dragged.
    Failed,
    /// Not a Windows build.
    Unsupported,
}

impl DragOutcome {
    /// Did the files actually land somewhere?
    pub fn delivered(self) -> bool {
        matches!(self, DragOutcome::Copied | DragOutcome::Linked)
    }
}

/// Which mouse button started the drag, so the drop is committed when *that*
/// button comes up rather than whichever one OLE happens to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragButton {
    Left,
    Right,
}

/// Drag `paths` out of the application, blocking until the drop completes.
///
/// Must be called on the UI thread, with the mouse button already down.
///
/// Building the data object reads directory entries only — it never opens a
/// file, so dragging a cloud placeholder does not hydrate it here. Hydration, if
/// any, happens in the target application when it reads the bytes, which is the
/// user's explicit intent by then.
pub fn drag_out(paths: &[PathBuf], button: DragButton) -> DragOutcome {
    if paths.is_empty() {
        return DragOutcome::Failed;
    }
    imp::drag_out(paths, button)
}

/// Longest path list worth handing to a drop target in one gesture. Well past
/// any plausible selection, and keeps a stray "select all" on a 100k-file root
/// from building a megabyte-scale PIDL array inside a mouse-down handler.
pub const MAX_DRAG_PATHS: usize = 4096;

#[cfg(windows)]
fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
mod imp {
    use super::{wide, DragButton, DragOutcome};
    use std::path::PathBuf;
    use windows::core::{implement, BOOL, HRESULT, PCWSTR};
    use windows::Win32::Foundation::{
        DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, S_OK,
    };
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{
        DoDragDrop, IDropSource, IDropSource_Impl, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
    };
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MK_RBUTTON, MODIFIERKEYS_FLAGS};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{
        BHID_DataObject, IShellItemArray, SHCreateShellItemArrayFromIDLists, SHParseDisplayName,
    };

    /// Owns the PIDLs for as long as the shell item array needs them.
    struct Pidls(Vec<*const ITEMIDLIST>);

    impl Drop for Pidls {
        fn drop(&mut self) {
            for &p in &self.0 {
                unsafe { windows::Win32::UI::Shell::ILFree(Some(p)) };
            }
        }
    }

    fn parse(paths: &[PathBuf]) -> Pidls {
        let mut out = Pidls(Vec::with_capacity(paths.len()));
        for p in paths {
            let w = wide(p);
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            // Resolves a name to a shell id list. This is a namespace lookup,
            // not a read of the file's contents.
            let ok = unsafe { SHParseDisplayName(PCWSTR(w.as_ptr()), None, &mut pidl, 0, None) };
            if ok.is_ok() && !pidl.is_null() {
                out.0.push(pidl as *const ITEMIDLIST);
            }
        }
        out
    }

    /// The shell's own data object for a set of files — the same one Explorer
    /// hands out, so it carries `CF_HDROP`, `CFSTR_SHELLIDLIST` and the rest
    /// without us having to implement `IDataObject` and guess at formats.
    pub(super) fn data_object(paths: &[PathBuf]) -> Option<IDataObject> {
        let pidls = parse(paths);
        if pidls.0.is_empty() {
            return None;
        }
        unsafe {
            let items: IShellItemArray = SHCreateShellItemArrayFromIDLists(&pidls.0).ok()?;
            items.BindToHandler(None, &BHID_DataObject).ok()
        }
    }

    #[implement(IDropSource)]
    struct DropSource {
        /// The button held when the drag began; releasing it is the drop.
        button: MODIFIERKEYS_FLAGS,
    }

    impl IDropSource_Impl for DropSource_Impl {
        fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
            if escape.as_bool() {
                return DRAGDROP_S_CANCEL;
            }
            if keys.0 & self.button.0 == 0 {
                return DRAGDROP_S_DROP;
            }
            S_OK
        }

        fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
            // The shell draws the copy/link cursors and the drag image.
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }

    pub(super) fn drag_out(paths: &[PathBuf], button: DragButton) -> DragOutcome {
        let Some(data) = data_object(paths) else {
            return DragOutcome::Failed;
        };
        let source: IDropSource = DropSource {
            button: match button {
                DragButton::Left => MK_LBUTTON,
                DragButton::Right => MK_RBUTTON,
            },
        }
        .into();

        let mut effect = DROPEFFECT::default();
        // Move is withheld on purpose — see the module docs.
        let hr = unsafe {
            DoDragDrop(
                &data,
                &source,
                DROPEFFECT_COPY | DROPEFFECT_LINK,
                &mut effect,
            )
        };
        if hr != DRAGDROP_S_DROP {
            return DragOutcome::Cancelled;
        }
        if effect & DROPEFFECT_LINK != DROPEFFECT::default() {
            DragOutcome::Linked
        } else if effect & DROPEFFECT_COPY != DROPEFFECT::default() {
            DragOutcome::Copied
        } else {
            // Dropped on a target that took nothing.
            DragOutcome::Cancelled
        }
    }

    /// Read the file list back out of a data object as `CF_HDROP`, which is what
    /// a plain drop target (PowerPoint, Explorer) actually consumes. Only used
    /// by tests — the real path never inspects its own data object.
    #[cfg(test)]
    pub(super) fn hdrop_paths(data: &IDataObject) -> Vec<PathBuf> {
        use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
        use windows::Win32::System::Ole::ReleaseStgMedium;
        use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

        let fmt = FORMATETC {
            cfFormat: windows::Win32::System::Ole::CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        unsafe {
            let Ok(mut medium) = data.GetData(&fmt) else {
                return Vec::new();
            };
            let hdrop = HDROP(medium.u.hGlobal.0);
            let count = DragQueryFileW(hdrop, u32::MAX, None);
            let mut out = Vec::with_capacity(count as usize);
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                buf.truncate(len);
                out.push(PathBuf::from(String::from_utf16_lossy(&buf)));
            }
            ReleaseStgMedium(&mut medium);
            out
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{DragButton, DragOutcome};
    use std::path::PathBuf;

    pub(super) fn drag_out(_paths: &[PathBuf], _button: DragButton) -> DragOutcome {
        DragOutcome::Unsupported
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// The whole feature rests on the receiving application seeing the exact
    /// files the user selected, in `CF_HDROP`, the format a plain drop target
    /// reads. Everything after this is cursor plumbing.
    #[test]
    fn the_data_object_carries_the_selected_paths_as_cf_hdrop() {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        let dir = std::env::temp_dir().join(format!("atlas_drag_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<PathBuf> = ["a.png", "b with space.txt", "c.3dm"]
            .iter()
            .map(|n| {
                let p = dir.join(n);
                std::fs::write(&p, b"x").unwrap();
                p
            })
            .collect();

        let data = imp::data_object(&paths).expect("shell data object for real files");
        let got = imp::hdrop_paths(&data);

        assert_eq!(got, paths, "CF_HDROP must list exactly the dragged files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_that_does_not_exist_yields_no_data_object() {
        let missing = vec![PathBuf::from(r"C:\atlas-nonexistent\nope.png")];
        assert!(
            imp::data_object(&missing).is_none(),
            "nothing to drag should fail cleanly rather than drag an empty set"
        );
    }
}
