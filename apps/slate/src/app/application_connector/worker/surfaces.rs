//! A cold editor load has its own bounded allowance; normal commands keep theirs.
use super::*;
fn cold_launch_deadline(command: &str) -> Duration {
    if command == "org.slate.rhino.grasshopper_show" {
        Duration::from_secs(120)
    } else {
        DEADLINE
    }
}

pub(super) fn discover(
    peer: &mut Exchange,
    events: &Sender<Event>,
    launch: Option<super::super::children::LaunchRequest>,
) -> Result<(), String> {
    let result = (|| {
        if let Some(request) = &launch {
            if !peer
                .channel
                .session()
                .commands()
                .any(|c| c.id == request.command && c.effect == CommandEffect::EphemeralView)
            {
                return Err("This source no longer offers the application window command".into());
            }
            peer.human_with_deadline(
                Operation::InvokeCommand {
                    command_id: request.command.clone(),
                    parameters: serde_json::json!({}),
                },
                true,
                cold_launch_deadline(&request.command),
            )?;
        }
        match peer.human(Operation::DescribeSurfaces, true)? {
            ReplyBody::Surfaces { surfaces } => Ok(surfaces),
            _ => Err("Application did not return surface descriptors".into()),
        }
    })();
    if let Some(request) = launch {
        events
            .send(Event::SurfaceLaunchComplete {
                ticket: request.ticket,
                result: result.clone(),
            })
            .map_err(|e| e.to_string())?;
    } else if let Ok(surfaces) = &result {
        events
            .send(Event::Surfaces(surfaces.clone()))
            .map_err(|e| e.to_string())?;
    }
    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_cold_editor_load_receives_the_long_allowance() {
        assert_eq!(
            cold_launch_deadline("org.slate.rhino.grasshopper_show"),
            Duration::from_secs(120)
        );
        assert_eq!(
            cold_launch_deadline("org.slate.other.window_show"),
            Duration::from_secs(20)
        );
        assert_eq!(
            request_deadline(&Operation::InvokeCommand {
                command_id: "org.slate.rhino.grasshopper_show".into(),
                parameters: serde_json::json!({}),
            }),
            Duration::from_secs(20)
        );
        assert_eq!(
            request_deadline(&Operation::DescribeSurfaces),
            Duration::from_secs(20)
        );
    }
}
