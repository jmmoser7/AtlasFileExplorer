//! World geometry for named Grasshopper grips.
use super::{Side, WireHost, FLOW_PORT_PITCH, FLOW_PORT_TOP};

const GRASSHOPPER_PORT_BASE: f32 = 2.0;
const GRASSHOPPER_PORT_TOP: f32 = 24.0;
pub(super) const GRASSHOPPER_PORT_PITCH: f32 = 21.0;

/// Reserved identity range for named Grasshopper grips on each side.
pub fn grasshopper_port_t(row: usize) -> f32 {
    GRASSHOPPER_PORT_BASE + row as f32
}

pub fn grasshopper_port_row(t: f32) -> Option<usize> {
    (t.is_finite() && t >= GRASSHOPPER_PORT_BASE && t.fract() == 0.0)
        .then_some((t - GRASSHOPPER_PORT_BASE) as usize)
}

impl WireHost {
    pub(super) fn grasshopper_rows_on(&self, side: Side) -> usize {
        match side {
            Side::Left => self.grasshopper_rows[0],
            Side::Right => self.grasshopper_rows[1],
            _ => 0,
        }
    }

    pub(super) fn grasshopper_anchor_t(&self, side: Side, row: usize) -> f32 {
        let rows = self.grasshopper_rows_on(side).max(row.saturating_add(1));
        let needed = FLOW_PORT_TOP + rows as f32 * GRASSHOPPER_PORT_PITCH + self.flow_tail() + 14.0;
        (GRASSHOPPER_PORT_TOP + row as f32 * GRASSHOPPER_PORT_PITCH) / needed.max(self.rect.h)
    }

    pub(super) fn flow_tail(&self) -> f32 {
        let rows = self
            .flow
            .iter()
            .filter(|p| p.slot != atlas_agent::InputSlot::View)
            .count();
        let rows = rows.max(if self.image && rows > 0 {
            crate::agent_inputs::GENERATOR_PORTS.len()
        } else {
            1
        });
        (rows - 1) as f32 * FLOW_PORT_PITCH
    }
}
