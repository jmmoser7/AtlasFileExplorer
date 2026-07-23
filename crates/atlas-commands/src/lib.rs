//! # atlas-commands
//!
//! Pure command-surface contracts shared by File Atlas and Slate: command
//! specs, the registry, execution history + repeat rules, the cancel-stack
//! contract, and the palette query. See `DESIGN.md`.
//!
//! Constitution bindings:
//!
//! - **Article I** — this crate must not depend on `egui`, `eframe`, or any
//!   renderer. Apps map raw input to [`Chord`]s and interpret returned
//!   command IDs; this crate holds only data and pure functions.
//! - **Article VI** — every [`HistoryEntry`] carries its [`CmdAuthor`]
//!   (human or named agent), mirroring the journal's authorship rule. The
//!   history is the *intent* log (named commands); journals hold invertible
//!   deltas — distinct logs, both attributed.
//! - **Article VII** — commands are data. Keyboard, palette, radial menu,
//!   and the future agent surface all enumerate and dispatch the same
//!   [`Registry`]; none of them can drift from the others.

mod cancel;
mod history;
mod palette;
mod registry;
mod spec;

pub use cancel::{cancel_target, CancelLayer};
pub use history::{CmdAuthor, History, HistoryEntry, HISTORY_CAP};
pub use palette::{palette_query, PaletteItem};
pub use registry::Registry;
pub use spec::{Availability, Chord, CommandId, CommandSpec, Key, Repeat};
