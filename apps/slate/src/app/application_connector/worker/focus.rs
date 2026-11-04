//! Trusted UI focus uses native acknowledgements, separate from content intent.
use super::*;

pub(super) fn main(
    peer: &mut Exchange,
    address: &PipeAddress,
    pid: u32,
    events: &Sender<Event>,
    focused: bool,
) -> Result<(), String> {
    if focused {
        peer.human(
            Operation::Surface {
                action: SurfaceAction::Show,
            },
            true,
        )?;
        peer.human(
            Operation::Surface {
                action: SurfaceAction::Focus,
            },
            true,
        )?;
    } else {
        match capture::preview(peer, address, pid) {
            Ok((width, height, rgba)) => {
                let _ = events.try_send(Event::Preview {
                    rgba,
                    width,
                    height,
                });
            }
            Err(error) => {
                let _ = events.try_send(Event::PreviewUnavailable(error));
            }
        }
        peer.human(
            Operation::Surface {
                action: SurfaceAction::Blur,
            },
            true,
        )?;
        peer.human(
            Operation::Surface {
                action: SurfaceAction::Hide,
            },
            true,
        )?;
    }
    Ok(())
}

pub(super) fn child(
    peer: &mut Exchange,
    surface: &str,
    command: &str,
    focused: bool,
) -> Result<(), String> {
    if !peer.channel.session().commands().any(|descriptor| {
        descriptor.id == command && descriptor.effect == CommandEffect::EphemeralView
    }) {
        return Err("This native source no longer offers the window command".into());
    }
    let reply = peer.human(
        Operation::InvokeCommand {
            command_id: command.into(),
            parameters: serde_json::json!({}),
        },
        true,
    )?;
    if !focused {
        return acknowledges_hidden(&reply, surface)
            .then_some(())
            .ok_or_else(|| "Application did not acknowledge its managed window was hidden".into());
    }
    if matches!(reply, ReplyBody::CommandResult { value, .. } if value["visible"] == false) {
        return Err("The application window remains parked".into());
    }
    Ok(())
}

fn acknowledges_hidden(reply: &ReplyBody, surface: &str) -> bool {
    matches!(reply, ReplyBody::CommandResult { value, .. }
        if value["visible"].as_bool() == Some(false)
            && value["surface_id"].as_str() == Some(surface))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_hide_requires_explicit_false_for_the_expected_surface() {
        assert!(!acknowledges_hidden(&ReplyBody::Ack, "child"));
        for (value, expected) in [
            (
                serde_json::json!({"surface_id":"child","visible":false}),
                true,
            ),
            (
                serde_json::json!({"surface_id":"other","visible":false}),
                false,
            ),
            (serde_json::json!({"surface_id":"child"}), false),
            (
                serde_json::json!({"surface_id":"child","visible":true}),
                false,
            ),
        ] {
            assert_eq!(
                acknowledges_hidden(
                    &ReplyBody::CommandResult {
                        value,
                        revision: None
                    },
                    "child"
                ),
                expected
            );
        }
    }
}
