//! Shared UI chrome for the Atlas ecosystem.
//!
//! Every app in the ecosystem (File Atlas, Slate, …) renders its window
//! chrome exclusively through this crate so the applications cannot drift
//! apart visually:
//!
//! - [`theme`] — the one [`theme::Palette`] + egui visuals both apps use.
//! - [`menubar`] — Windows-style File/View menu bar (topmost chrome row).
//! - [`tabs`] — browser-style top bar and tab strip (identical painting).
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
pub mod menubar;
pub mod sidebar;
pub mod tabs;
pub mod theme;
pub mod widgets;
