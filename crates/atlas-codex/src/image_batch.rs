//! A requested image count is a bounded sequence of single-image turns.
use super::*;

impl Client {
    /// Worker entry point used by Slate's bounded steering channel. Each image
    /// turn has the same captured inputs and explicit history, never the reply
    /// or output of the preceding image. No failed pass is retried.
    pub fn run_with_steering(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        cancel: &AtomicBool,
        steering: Option<&std::sync::mpsc::Receiver<SteerCommand>>,
        mut changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        if request
            .image
            .as_ref()
            .is_none_or(|image| image.images() == 1)
        {
            return self.run_turn(request, session, cancel, steering, true, changed);
        }
        run_images(
            request,
            session,
            cancel,
            &mut changed,
            |pass, single, state, progress| {
                self.run_turn(single, state, cancel, steering, pass == 0, progress)
            },
        )
    }
}

fn run_images(
    request: &AgentRequest,
    session: &mut AgentSession,
    cancel: &AtomicBool,
    changed: &mut dyn FnMut(&AgentSession),
    mut run_turn: impl FnMut(
        usize,
        &AgentRequest,
        &mut AgentSession,
        &mut dyn FnMut(&AgentSession),
    ) -> Result<(), String>,
) -> Result<(), String> {
    let image = request.image.as_ref().expect("image batch parameters");
    let requested = usize::from(image.images());
    let before = session.bundle.images.len();
    let mut single = request.clone();
    single.image.as_mut().unwrap().count = 1;
    for pass in 0..requested {
        if cancel.load(Ordering::Relaxed) {
            let error = format!("Image generation stopped after {} of {requested} requested images. Completed images were kept.", session.bundle.images.len() - before);
            session.status = AgentStatus::Error(error.clone());
            changed(session);
            return Err(error);
        }
        let outcome = run_turn(pass, &single, session, &mut |state| {
            // A completed single-image turn is progress within this batch. It
            // must not unlock Slate's next queued request between passes.
            if state.status == AgentStatus::Idle {
                let mut running = state.clone();
                running.status = AgentStatus::Thinking;
                changed(&running);
            } else {
                changed(state);
            }
        });
        let returned = session.bundle.images.len() - before;
        let failure = outcome.err().or_else(|| match &session.status {
            AgentStatus::Error(error) => Some(error.clone()),
            _ => None,
        });
        if let Some(error) = failure {
            let error = format!("Codex image generation ended after {returned} of {requested} requested images. Completed images were kept. {error}");
            session.status = AgentStatus::Error(error.clone());
            changed(session);
            return Err(error);
        }
        // Guard the actual count independently of the provider's turn status.
        if returned != pass + 1 {
            let error = format!("Codex returned {returned} images while completing image {} of {requested}. Returned images were kept; the remaining passes were canceled.", pass + 1);
            session.status = AgentStatus::Error(error.clone());
            changed(session);
            return Err(error);
        }
        session.status = AgentStatus::Thinking;
        session.record_activity(AgentActivity {
            id: format!("image-batch:{pass}"),
            request: request.id.clone(),
            kind: AgentActivityKind::Commentary,
            text: format!("{returned} of {requested} images ready"),
            at: now(),
            semantics: None,
        });
        changed(session);
    }
    image.validate_output_count("Codex", session.bundle.images.len() - before)?;
    session.status = AgentStatus::Idle;
    changed(session);
    Ok(())
}

#[cfg(test)]
mod tests;
