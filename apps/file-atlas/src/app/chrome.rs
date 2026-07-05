//! Which optional sub-panels are visible in the left tools rail and bottom
//! readout bar. Toggled from the gear menus — the registry is the extension
//! point for future agent-added panels. The toggle mechanics live in
//! `atlas_shell::chrome`; only the panel *sets* are app-specific.

/// Left tools rail: panels that act on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ToolPanel {
    BasicFilters = 0,
    DisplaySettings = 1,
    Workflow = 2,
}

impl ToolPanel {
    pub const ALL: [ToolPanel; 3] = [
        ToolPanel::BasicFilters,
        ToolPanel::DisplaySettings,
        ToolPanel::Workflow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ToolPanel::BasicFilters => "Basic filters",
            ToolPanel::DisplaySettings => "Display settings",
            ToolPanel::Workflow => "Workflow",
        }
    }
}

impl From<ToolPanel> for usize {
    fn from(p: ToolPanel) -> usize {
        p as usize
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
}

impl From<ReadoutPanel> for usize {
    fn from(p: ReadoutPanel) -> usize {
        p as usize
    }
}

/// Per-tab UI chrome configuration (nested inside the active tab's workspace).
pub type ChromeConfig = atlas_shell::chrome::ChromeConfig<3, 2>;
