//! Retained app-renderer evidence distinguishes proposal ghosts from execution.
use super::*;

#[test]
#[ignore = "retains actual proposed/approved linked-image frames; native interactive acceptance is separate"]
fn accepted_listening_image_has_three_opaque_rendered_drag_states_before_its_only_commit() {
    let mut f = Fixture::new(-200.0);
    f.h.app.tab_mut().path = Some(f.h.base.join("approved-render.slate"));
    f.h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .mutation_autonomy = false;
    let target = f.images[0];
    let before = f.h.app.doc().scene.node(target).unwrap().clone();
    let authored = f.h.app.doc().scene.clone();
    let depth = f.h.app.tab().journal.undo_depth();
    f.invoke(
        0,
        "board.objects.move",
        &[target],
        json!({"dx":320,"dy":100}),
    );
    let mut proposed = false;
    for _ in 0..160 {
        let out = f.step();
        assert_eq!(f.h.app.doc().scene, authored);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        assert!(f.h.app.presence_preview(target).is_none());
        assert!(!f.h.app.presence.holds_node(target));
        if !proposed && f.sample().phase == MotionPhase::Work {
            f.draw(&out);
            assert_eq!(
                f.blue(&before),
                Some(true),
                "original image remains opaque during proposal"
            );
            f.save("unapproved-ghost", out);
            proposed = true;
        }
        if !f.pending() {
            break;
        }
    }
    assert!(proposed && !f.pending());
    let proposal = f.h.app.presence_proposal_rows(&f.who)[0].id.to_owned();
    let who = f.who.clone();
    f.h.app.presence_address(&f.h.ctx, &who).unwrap();
    f.h.app.accept_stage_proposal(&proposal);
    assert!(f.h.app.presence.agents[0].paused);
    let mut samples = Vec::new();
    for _ in 0..160 {
        let out = f.step();
        if !f.pending() {
            f.save("approved-committed-listening", out);
            break;
        }
        assert_eq!(f.h.app.doc().scene, authored);
        assert_eq!(f.h.app.tab().journal.undo_depth(), depth);
        assert_eq!(
            f.h.app
                .presence
                .avatar_activity(&f.h.app.presence.agents[0]),
            "moving"
        );
        if f.sample().phase != MotionPhase::Work {
            continue;
        }
        let preview = f.h.app.presence_preview(target).unwrap().clone();
        let progress = (preview.rect.x - before.rect.x) / 320.0;
        if !(0.04..0.97).contains(&progress) {
            continue;
        }
        let slot = if progress < 0.35 {
            0
        } else if progress < 0.7 {
            1
        } else {
            2
        };
        if slot != samples.len() {
            continue;
        }
        f.draw(&out);
        assert_eq!(
            f.blue(&preview),
            Some(true),
            "real linked pixels move opaquely after approval"
        );
        f.save(&format!("approved-drag-{}", slot + 1), out);
        samples.push(preview.rect);
    }
    assert_eq!(samples.len(), 3);
    assert!(samples.windows(2).all(|w| w[0].x < w[1].x));
    assert!(f.h.app.presence.agents[0].paused);
    assert_eq!(
        f.h.app
            .presence
            .avatar_activity(&f.h.app.presence.agents[0]),
        "listening"
    );
    assert_eq!(f.h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        f.h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    assert!(
        !f.h.app.presence.agents[0]
            .grant
            .as_ref()
            .unwrap()
            .mutation_autonomy
    );
    f.record(
        "approved-drag-samples",
        json!({"positions":samples,"singleCommit":true,
        "originalVisibleBeforeApproval":proposed,"globalPausedThroughout":true}),
    );
}
