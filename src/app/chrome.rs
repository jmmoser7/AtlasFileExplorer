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
            ToolPanel::BasicFilters
                | ToolPanel::DisplaySettings
                | ToolPanel::Workflow
                | ToolPanel::Tags
        )
    }
}

/// Bottom readout bar: informational panels (metrics today; more later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ReadoutPanel {
    Metrics = 0,
    ActivityHeatmap = 1,
}

impl ReadoutPanel {
    pub const ALL: [ReadoutPanel; 2] = [ReadoutPanel::Metrics, ReadoutPanel::ActivityHeatmap];

    pub fn label(self) -> &'static str {
        match self {
            ReadoutPanel::Metrics => "Metrics",
            ReadoutPanel::ActivityHeatmap => "Activity heatmap",
        }
    }

    pub fn default_on(self) -> bool {
        match self {
            ReadoutPanel::Metrics => true,
            ReadoutPanel::ActivityHeatmap => true,
        }
    }

    pub fn default_expanded(self) -> bool {
        match self {
            ReadoutPanel::Metrics => true,
            ReadoutPanel::ActivityHeatmap => true,
        }
    }

    pub fn default_width_frac(self) -> f32 {
        match self {
            ReadoutPanel::Metrics => 1.0,
            ReadoutPanel::ActivityHeatmap => 0.62,
        }
    }

    /// Short title for the capsule header (sub-dashboards only).
    pub fn dashboard_title(self) -> &'static str {
        match self {
            ReadoutPanel::Metrics => "Metrics",
            ReadoutPanel::ActivityHeatmap => "Activity",
        }
    }

    pub fn is_sub_dashboard(self) -> bool {
        !matches!(self, ReadoutPanel::Metrics)
    }
}

/// Per-tab UI chrome configuration (nested inside the active tab's workspace).
#[derive(Clone, Debug)]
pub struct ChromeConfig {
    pub tools: [bool; 4],
    /// Within-section expand/collapse (gear menu still controls overall visibility).
    pub tools_expanded: [bool; 4],
    pub readouts: [bool; 2],
    /// Sub-dashboard body expand/collapse (metrics ticker ignores this).
    pub readouts_expanded: [bool; 2],
    /// Sub-dashboard width as a fraction of the bottom bar (metrics ignores this).
    pub readout_width_frac: [f32; 2],
    /// Advanced tools (pre-warm, shared cache path) — floating window, not a rail panel.
    pub advanced_open: bool,
}

impl Default for ChromeConfig {
    fn default() -> Self {
        let mut tools = [false; 4];
        for p in ToolPanel::ALL {
            tools[p as usize] = p.default_on();
        }
        let mut tools_expanded = [false; 4];
        for p in ToolPanel::ALL {
            tools_expanded[p as usize] = true;
        }
        let mut readouts = [false; 2];
        for p in ReadoutPanel::ALL {
            readouts[p as usize] = p.default_on();
        }
        let mut readouts_expanded = [false; 2];
        for p in ReadoutPanel::ALL {
            readouts_expanded[p as usize] = p.default_expanded();
        }
        let mut readout_width_frac = [0.0f32; 2];
        for p in ReadoutPanel::ALL {
            readout_width_frac[p as usize] = p.default_width_frac();
        }
        Self {
            tools,
            tools_expanded,
            readouts,
            readouts_expanded,
            readout_width_frac,
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

    pub fn tool_expanded(&self, panel: ToolPanel) -> bool {
        self.tools_expanded[panel as usize]
    }

    pub fn set_tool_expanded(&mut self, panel: ToolPanel, expanded: bool) {
        self.tools_expanded[panel as usize] = expanded;
    }

    pub fn readout(&self, panel: ReadoutPanel) -> bool {
        self.readouts[panel as usize]
    }

    pub fn set_readout(&mut self, panel: ReadoutPanel, on: bool) {
        self.readouts[panel as usize] = on;
    }

    pub fn readout_expanded(&self, panel: ReadoutPanel) -> bool {
        self.readouts_expanded[panel as usize]
    }

    pub fn set_readout_expanded(&mut self, panel: ReadoutPanel, expanded: bool) {
        self.readouts_expanded[panel as usize] = expanded;
    }

    pub fn readout_width_frac(&self, panel: ReadoutPanel) -> f32 {
        self.readout_width_frac[panel as usize]
    }

    pub fn set_readout_width_frac(&mut self, panel: ReadoutPanel, frac: f32) {
        self.readout_width_frac[panel as usize] = frac;
    }
}
