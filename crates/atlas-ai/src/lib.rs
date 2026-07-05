//! AI / Cursor integration shared by File Atlas and Slate.
//!
//! This crate is the plumbing layer for the ecosystem's AI features. Today it
//! provides:
//!
//! - [`config`] — the shared **AI workspace** folder. The user establishes it
//!   on first launch (required) and it persists across both apps and all
//!   future Slate instances; it becomes Cursor's default working directory
//!   whenever Cursor is launched from either program.
//! - [`launch`] — locating and launching Cursor (assumed installed).
//! - [`context`] — the **live link**: each app maintains a machine-readable
//!   context file inside the AI workspace (`.atlas-ai/<app>-context.json`)
//!   describing what is currently being previewed. Future MCP servers read
//!   these to give Cursor full view of Atlas/Slate state (auto-tagging,
//!   classification, presentation generation, …).
//! - [`ui`] — the sidebar panel body both apps render, so the AI toolbar is
//!   pixel-identical in Atlas and Slate (see the shared-chrome rule).
//!
//! Both apps hold one [`AiPanel`] and call [`AiPanel::poll`] every frame.

pub mod config;
pub mod context;
pub mod launch;
pub mod ui;

pub use config::AiConfig;
pub use context::AiAppContext;
pub use ui::AiPanel;
