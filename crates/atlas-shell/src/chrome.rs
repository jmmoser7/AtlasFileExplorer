//! Gear-menu panel registry, generic over each app's panel enums.
//!
//! Apps define their own `ToolPanel` / `ReadoutPanel` enums (the *set* of
//! panels is app-specific) and store visibility in a [`ChromeConfig`]
//! parameterized by the panel counts. The toggle/expand mechanics are shared
//! so the gear menus behave identically in every app.

/// Per-tab UI chrome configuration (nested inside the active tab's workspace).
///
/// `T` = number of tool panels, `R` = number of readout panels. Panel enums
/// index into the arrays via `as usize` (implement `From<Panel> for usize`).
#[derive(Clone, Debug)]
pub struct ChromeConfig<const T: usize, const R: usize> {
    pub tools: [bool; T],
    /// Within-section expand/collapse (gear menu still controls overall visibility).
    pub tools_expanded: [bool; T],
    pub readouts: [bool; R],
    /// Advanced tools — floating window, not a rail panel.
    pub advanced_open: bool,
    /// Full-screen canvas: suppress the tools rail and bottom readout bar so
    /// the canvas takes the whole space below the menu bar and tab strip.
    /// Toggled from the canvas mini menu (⛶), the View menu, or F11.
    pub canvas_fullscreen: bool,
}

impl<const T: usize, const R: usize> Default for ChromeConfig<T, R> {
    fn default() -> Self {
        Self {
            tools: [true; T],
            tools_expanded: [true; T],
            readouts: [true; R],
            advanced_open: false,
            canvas_fullscreen: false,
        }
    }
}

impl<const T: usize, const R: usize> ChromeConfig<T, R> {
    /// Construct with explicit per-panel visibility defaults.
    pub fn with_defaults(tools: [bool; T], readouts: [bool; R]) -> Self {
        Self {
            tools,
            tools_expanded: [true; T],
            readouts,
            advanced_open: false,
            canvas_fullscreen: false,
        }
    }

    pub fn tool(&self, panel: impl Into<usize>) -> bool {
        self.tools[panel.into()]
    }

    pub fn set_tool(&mut self, panel: impl Into<usize>, on: bool) {
        self.tools[panel.into()] = on;
    }

    pub fn tool_expanded(&self, panel: impl Into<usize>) -> bool {
        self.tools_expanded[panel.into()]
    }

    pub fn set_tool_expanded(&mut self, panel: impl Into<usize>, expanded: bool) {
        self.tools_expanded[panel.into()] = expanded;
    }

    pub fn readout(&self, panel: impl Into<usize>) -> bool {
        self.readouts[panel.into()]
    }

    pub fn set_readout(&mut self, panel: impl Into<usize>, on: bool) {
        self.readouts[panel.into()] = on;
    }
}
