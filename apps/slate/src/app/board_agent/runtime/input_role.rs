//! Input-role classification and its shared chip presentation.
use super::*;

impl InputRole {
    /// The role a port reads before anything is wired to it.
    pub fn of_slot(slot: atlas_agent::InputSlot) -> Self {
        match slot {
            atlas_agent::InputSlot::Media | atlas_agent::InputSlot::View => Self::Image,
            atlas_agent::InputSlot::Prompt => Self::Prompt,
            atlas_agent::InputSlot::Style => Self::Style,
        }
    }

    /// Chip name and the colour a chip dot and its port share.
    pub fn look(self, roles: &atlas_shell::tokens::AgentRoleInk) -> (&'static str, Color32) {
        match self {
            Self::Prompt => ("Prompt", roles.prompt),
            Self::Geometry => ("Geometry", roles.geometry),
            Self::Image => ("Image", roles.image),
            Self::Style => ("Style", roles.style),
        }
    }
}
