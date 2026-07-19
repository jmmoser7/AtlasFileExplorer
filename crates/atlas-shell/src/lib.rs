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
//! - [`dock`] — floating squircle docks and popover hosts.
//! - [`tokens`] — values embedded from the human-editable `ui-tokens.toml`.
//! - [`tuning`] — optional live editor, compiled only with `ui-tuner`.
//! - [`sidebar`] — left tools-rail layout primitives (sections, rows).
//! - [`widgets`] — shared controls (chips, sliders, timeline, gear menu).
//! - [`chrome`] — gear-menu panel registry, generic over each app's panels.
//! - [`commands`] — command-reference table + shared canvas navigation.
//!
//! **Rule:** apps may decide *which* panels and commands they expose, but the
//! rendering of chrome (colors, spacing, tab shapes, section cards) lives
//! here and only here. See `AGENTS.md` at the repo root.

pub mod chrome;
pub mod commands;
pub mod dock;
pub mod grid_fade;
pub mod menubar;
pub mod sidebar;
pub mod tabs;
pub mod theme;
pub mod tokens;
pub mod tuning;
pub mod widgets;
