//! Existing whole-object preview interpolation.
use super::*;

pub(super) fn preview_pair(
    pair: &(Option<Node>, Option<Node>),
    sample: MotionSample,
    previews: &mut HashMap<slate_doc::NodeId, Node>,
) {
    let (before, after) = pair;
    let t = sample.progress;
    let t = t * t * (3.0 - 2.0 * t);
    match (before, after) {
        (Some(before), Some(after)) => {
            let node = previews.entry(after.id).or_insert_with(|| after.clone());
            node.rect = slate_doc::scene::WorldRect::new(
                before.rect.x + (after.rect.x - before.rect.x) * t,
                before.rect.y + (after.rect.y - before.rect.y) * t,
                before.rect.w + (after.rect.w - before.rect.w) * t,
                before.rect.h + (after.rect.h - before.rect.h) * t,
            );
            node.opacity = after.opacity;
        }
        (None, Some(after)) => {
            let node = previews.entry(after.id).or_insert_with(|| after.clone());
            if let NodeKind::Connector(conn) = &mut node.kind {
                conn.b = if sample.phase == MotionPhase::ReadyToCommit {
                    if let NodeKind::Connector(c) = &after.kind {
                        c.b
                    } else {
                        unreachable!()
                    }
                } else {
                    slate_doc::scene::ConnectorEnd::Free {
                        point: [sample.position.x as f32, sample.position.y as f32],
                    }
                };
                node.opacity = after.opacity * 0.8;
            } else {
                node.opacity = after.opacity * t;
            }
        }
        (Some(before), None) => {
            let node = previews.entry(before.id).or_insert_with(|| before.clone());
            node.opacity = before.opacity * (1.0 - t);
        }
        _ => {}
    }
}
