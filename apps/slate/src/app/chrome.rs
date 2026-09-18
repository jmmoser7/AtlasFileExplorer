//! Slate's panel registry — which optional sub-panels are visible in the left
//! tools rail and bottom readout bar. The toggle mechanics live in
//! `atlas_shell::chrome`; only the panel *sets* are app-specific.

/// Left tools rail: panels that act on the slate canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ToolPanel {
    Selection = 4,
}

impl From<ToolPanel> for usize {
    fn from(p: ToolPanel) -> usize {
        p as usize
    }
}

/// Bottom readout bar: informational panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ReadoutPanel {
    Metrics = 0,
    /// Link health: how many workbook items point at missing files.
    LinkHealth = 1,
}

impl ReadoutPanel {
    pub const ALL: [ReadoutPanel; 2] = [ReadoutPanel::Metrics, ReadoutPanel::LinkHealth];

    pub fn label(self) -> &'static str {
        match self {
            ReadoutPanel::Metrics => "Metrics",
            ReadoutPanel::LinkHealth => "Link health",
        }
    }
}

impl From<ReadoutPanel> for usize {
    fn from(p: ReadoutPanel) -> usize {
        p as usize
    }
}

/// Per-tab UI chrome configuration.
pub type ChromeConfig = atlas_shell::chrome::ChromeConfig<6, 2>;

pub fn default_chrome() -> ChromeConfig {
    ChromeConfig::default()
}
