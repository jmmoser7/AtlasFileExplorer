//! Typed-media port roles, shared by chips, wires and their host affordances.
use super::*;

/// Port-role chip colours on agent nodes (prompt / geometry / image / style).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardAgentTokens {
    pub light: BoardAgentRoleTokens,
    pub dark: BoardAgentRoleTokens,
}

impl Default for BoardAgentTokens {
    fn default() -> Self {
        Self {
            light: BoardAgentRoleTokens::light(),
            dark: BoardAgentRoleTokens::dark(),
        }
    }
}

impl BoardAgentTokens {
    pub fn roles(&self, dark_mode: bool) -> AgentRoleInk {
        let t = if dark_mode { &self.dark } else { &self.light };
        AgentRoleInk {
            prompt: t.prompt.color(),
            geometry: t.geometry.color(),
            image: t.image.color(),
            style: t.style.color(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BoardAgentRoleTokens {
    pub prompt: Hex,
    pub geometry: Hex,
    pub image: Hex,
    pub style: Hex,
}

impl BoardAgentRoleTokens {
    /// Reference media-port hues; labels use the theme's legible ink.
    fn light() -> Self {
        Self {
            prompt: Hex::rgb(0x50, 0x50, 0x4f),
            geometry: Hex::rgb(0x13, 0x8a, 0x72),
            image: Hex::rgb(0x20, 0xb8, 0xc2),
            style: Hex::rgb(0xff, 0xf0, 0xa0),
        }
    }

    fn dark() -> Self {
        Self {
            prompt: Hex::rgb(0xd2, 0xd5, 0xda),
            geometry: Hex::rgb(0x5c, 0xcc, 0xaa),
            image: Hex::rgb(0x3b, 0xce, 0xd4),
            style: Hex::rgb(0xef, 0xe0, 0x9c),
        }
    }
}

impl Default for BoardAgentRoleTokens {
    fn default() -> Self {
        Self::dark()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AgentRoleInk {
    pub prompt: Color32,
    pub geometry: Color32,
    pub image: Color32,
    pub style: Color32,
}
