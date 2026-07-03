//! Which optional sub-panels are visible in the left tools rail and bottom
//! readout bar. Toggled from the gear menus — the registry is the extension
//! point for future agent-added panels.

/// Left tools rail: panels that act on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ToolPanel {
    BasicFilters = 0,
    DisplaySettings = 1,
    Workflow = 2,
    Tags = 3,
}

impl ToolPanel {
    pub const ALL: [ToolPanel; 4] = [
        ToolPanel::BasicFilters,
        ToolPanel::DisplaySettings,
        ToolPanel::Workflow,
        ToolPanel::Tags,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ToolPanel::BasicFilters => "Basic filters",
            ToolPanel::DisplaySettings => "Display settings",
            ToolPanel::Workflow => "Workflow",
            ToolPanel::Tags => "Tags",
        }
    }

    pub fn default_on(self) -> bool {
        matches!(
            self,
            ToolPanel::BasicFilters | ToolPanel::DisplaySettings | ToolPanel::Workflow | ToolPanel::Tags
        )
    }
}

/// Bottom readout bar: informational panels (metrics today; more later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ReadoutPanel {
    Metrics = 0,
}

impl ReadoutPanel {
    pub const ALL: [ReadoutPanel; 1] = [ReadoutPanel::Metrics];

    pub fn label(self) -> &'static str {
        match self {
            ReadoutPanel::Metrics => "Metrics",
        }
    }

    pub fn default_on(self) -> bool {
        true
    }
}

/// Per-tab UI chrome configuration (nested inside the active tab's workspace).
#[derive(Clone, Debug)]
pub struct ChromeConfig {
    pub tools: [bool; 4],
    pub readouts: [bool; 1],
    /// Advanced tools (pre-warm, shared cache path) — floating window, not a rail panel.
    pub advanced_open: bool,
}

impl Default for ChromeConfig {
    fn default() -> Self {
        let mut tools = [false; 4];
        for p in ToolPanel::ALL {
            tools[p as usize] = p.default_on();
        }
        let mut readouts = [false; 1];
        for p in ReadoutPanel::ALL {
            readouts[p as usize] = p.default_on();
        }
        Self {
            tools,
            readouts,
            advanced_open: false,
        }
    }
}

impl ChromeConfig {
    pub fn tool(&self, panel: ToolPanel) -> bool {
        self.tools[panel as usize]
    }

    pub fn set_tool(&mut self, panel: ToolPanel, on: bool) {
        self.tools[panel as usize] = on;
    }

    pub fn readout(&self, panel: ReadoutPanel) -> bool {
        self.readouts[panel as usize]
    }

    pub fn set_readout(&mut self, panel: ReadoutPanel, on: bool) {
        self.readouts[panel as usize] = on;
    }
}