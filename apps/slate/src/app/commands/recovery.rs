//! Recovery commands are data in the same application registry.
use super::{spec, Repeat, GLOBAL};

pub(super) const OPEN: atlas_commands::CommandSpec = spec(
    "app.recovery.open",
    "Application",
    "Recover workbooks",
    "Advanced → Autosave & recovery → Recover workbooks",
    None,
    Repeat::Never,
    GLOBAL,
    &["autosave", "unsaved", "restore"],
);

pub(super) const FOLDER: atlas_commands::CommandSpec = spec(
    "app.recovery.folder",
    "Application",
    "Open recovery folder",
    "Advanced → Autosave & recovery → Open recovery folder",
    None,
    Repeat::Never,
    GLOBAL,
    &["backup", "autosave"],
);

pub(super) const RESTORE: atlas_commands::CommandSpec = spec(
    "app.recovery.restore",
    "Application",
    "Recover workbook copy",
    "Recovery window → Recover a copy; preserves the original",
    None,
    Repeat::Never,
    GLOBAL,
    &["recover", "restore"],
);

pub(super) const DISCARD: atlas_commands::CommandSpec = spec(
    "app.recovery.discard",
    "Application",
    "Discard recovery copies",
    "Recovery window → Discard, then confirm",
    None,
    Repeat::Never,
    GLOBAL,
    &["backup", "discard"],
);

pub(super) const CONFIGURE: atlas_commands::CommandSpec = spec(
    "app.recovery.configure",
    "Application",
    "Configure autosave and recovery",
    "Advanced → Autosave & recovery: enable, interval, retained copies",
    None,
    Repeat::Never,
    GLOBAL,
    &["autosave", "preferences"],
);
