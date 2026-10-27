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

pub(super) fn child(peer: &mut Exchange, command: &str) -> Result<(), String> {
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
    if matches!(reply, ReplyBody::CommandResult { value, .. } if value["visible"] == false) {
        return Err("The application window remains parked".into());
    }
    Ok(())
}
