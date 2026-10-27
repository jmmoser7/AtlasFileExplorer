//! The Rhino adapter's one concrete native-window command pairing.
pub(super) struct WindowCommands {
    pub surface: &'static str,
    pub open: &'static str,
    pub hide: &'static str,
}
pub(super) const GRASSHOPPER: WindowCommands = WindowCommands {
    surface: "grasshopper",
    open: "org.slate.rhino.grasshopper_show",
    hide: "org.slate.rhino.grasshopper_hide",
};

pub(super) fn offered(
    commands: &[atlas_connector::CommandDescriptor],
    surface: &str,
    open: &str,
) -> Option<&'static WindowCommands> {
    let pair = &GRASSHOPPER;
    (pair.surface == surface
        && pair.open == open
        && [pair.open, pair.hide].iter().all(|id| {
            commands
                .iter()
                .any(|c| c.id == *id && c.effect == atlas_connector::CommandEffect::EphemeralView)
        }))
    .then_some(pair)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_exact_ephemeral_commands_must_be_offered() {
        let command = |id: &str| atlas_connector::CommandDescriptor {
            id: id.into(),
            title: id.into(),
            effect: atlas_connector::CommandEffect::EphemeralView,
            undo: atlas_connector::UndoOwner::NativeApplication,
            cancellable: false,
            agent_allowed: false,
            parameters: serde_json::json!({"type":"object"}),
        };
        let mut commands = vec![command(GRASSHOPPER.open)];
        assert!(offered(&commands, GRASSHOPPER.surface, GRASSHOPPER.open).is_none());
        commands.push(command(GRASSHOPPER.hide));
        assert!(offered(&commands, GRASSHOPPER.surface, GRASSHOPPER.open).is_some());
        assert!(offered(&commands, "other-window", GRASSHOPPER.open).is_none());
        commands[1].effect = atlas_connector::CommandEffect::DraftMutation;
        assert!(offered(&commands, GRASSHOPPER.surface, GRASSHOPPER.open).is_none());
    }
}
