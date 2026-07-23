//! Shared UI chrome for the Atlas ecosystem.
//!
//! Every app in the ecosystem (File Atlas, Slate, …) renders its window
//! chrome exclusively through this crate so the applications cannot drift
//! apart visually:
//!
//! - [`theme`] — the one [`theme::Palette`] + egui visuals both apps use.
//! - [`menubar`] — unified top bar: icon portal (File/View menus), inline
//!   browser tabs, caption drag, and window controls (see `TOPBAR.md`).
//! - [`tabs`] — tab strip painting used inside the unified top bar.
//! - [`dock`] — floating squircle docks and popover hosts (see `DOCK.md`).
//! - [`taper`] — soft AA tapered ribbons shared across chrome (see `PAINT.md`).
//! - [`tokens`] — values embedded from the human-editable `ui-tokens.toml`.
//! - [`tuning`] — optional live editor, compiled only with `ui-tuner`.
//! - [`sidebar`] — left tools-rail layout primitives (sections, rows).
//! - [`widgets`] — shared controls (chips, sliders, timeline, gear menu).
//! - [`chrome`] — gear-menu panel registry, generic over each app's panels.
//! - [`commands`] — command-reference table + shared canvas navigation.
//! - [`home`] — Cover Flow launch / home surface (recent folders & workbooks).
//! - [`recent`] — persisted MRU lists for the home surface.
//! - [`minimap`] — cached-texture canvas minimap overlay.
//! - [`palette`] — floating canvas command palette popup.
//! - [`history_ui`] — read-only journal-history overlay.
//!
//! **Rule:** apps may decide *which* panels and commands they expose, but the
//! rendering of chrome (colors, spacing, tab shapes, section cards) lives
//! here and only here. See `AGENTS.md` at the repo root.

pub mod chrome;
pub mod commands;
pub mod covers;
pub mod dock;
pub mod grid_fade;
pub mod history_ui;
pub mod home;
pub mod menubar;
pub mod minimap;
pub mod palette;
pub mod prefs;
pub mod recent;
pub mod sidebar;
pub mod tabs;
pub mod taper;
pub mod theme;
pub mod tokens;
pub mod tuning;
pub mod widgets;
