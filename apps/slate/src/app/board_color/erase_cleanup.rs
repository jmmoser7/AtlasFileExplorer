use super::*;

impl SlateApp {
    /// Start the bounded raster check that completes an already committed
    /// targeted erase. The check is tied to that exact group and author, so
    /// a later edit or another author's group cannot be amended by its reply.
    pub(crate) fn queue_targeted_erase_cleanup(
        &mut self,
        cmds: &[slate_doc::scene::SceneCmd],
        author: slate_doc::scene::CmdAuthor,
    ) {
        let Some(token) = self.tab().journal.top_token() else {
            return;
        };
        if self.tab().journal.last_author() != Some(&author) {
            return;
        }
        let pending: Vec<_> = cmds
            .iter()
            .filter_map(|cmd| {
                let slate_doc::scene::SceneCmd::Patch { before, after } = cmd else {
                    return None;
                };
                let (NodeKind::Shape(before_shape), NodeKind::Shape(after_shape)) =
                    (&before.kind, &after.kind)
                else {
                    return None;
                };
                let erasures = &after_shape.path.as_ref()?.erase;
                if erasures.is_empty() || before_shape.path.as_ref()?.erase == *erasures {
                    return None;
                }
                let key = board_path::node_stamp_key(after)?;
                (board_path::node_stamp_key(before) != Some(key)
                    && self
                        .doc()
                        .scene
                        .node(after.id)
                        .and_then(board_path::node_stamp_key)
                        == Some(key))
                .then(|| (after.id, key, (**after).clone()))
            })
            .collect();
        for (id, key, after) in pending {
            self.brush_tiles.ask_ink(&after, key);
            self.erase_settle.check_as(id, key, token, author.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;
    use slate_doc::scene::{CmdAuthor, SceneCmd};

    #[test]
    fn targeted_style_patch_cannot_schedule_eraser_removal() {
        let mut h = Harness::new("targeted_erase_cleanup_scope");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let prepared = h.app.prepare_targeted_command("board.paths.draw", &[],
            &serde_json::json!({"strokes":[{"tool":"brush","points":[[0,0],[60,0]],"width":12}]})).unwrap();
        let before = match &prepared.cmds[0] {
            SceneCmd::Add { node, .. } => node.clone(),
            _ => unreachable!(),
        };
        h.app
            .finish_targeted_command(prepared, "Dot", true, "fixture")
            .unwrap();
        let mut faded = before.clone();
        if let NodeKind::Shape(shape) = &mut faded.kind {
            shape.stroke.color.0[3] = 0;
            for tip in &mut std::sync::Arc::make_mut(shape.path.as_mut().unwrap()).tips {
                tip.color.0[3] = 0;
            }
        }
        assert_ne!(
            board_path::node_stamp_key(&before),
            board_path::node_stamp_key(&faded)
        );
        let cmds = vec![SceneCmd::Patch {
            before: Box::new(before.clone()),
            after: Box::new(faded.clone()),
        }];
        assert!(h.app.commit_scene_as(cmds.clone(), CmdAuthor::Human));
        h.app.queue_targeted_erase_cleanup(&cmds, CmdAuthor::Human);
        assert!(
            !h.app.erase_settle.has_checks(),
            "a transparent style edit must retain its editable stroke"
        );

        let span = StrokeSpan {
            width: 24.0,
            softness: 0.0,
            color: Rgba::BLACK,
            texture: slate_doc::scene::BrushTexture::Smooth,
        };
        let erased = super::super::with_erase_mark(
            &faded,
            &[Pos2::new(0.0, 0.0), Pos2::new(60.0, 0.0)],
            span,
        );
        let cmds = vec![SceneCmd::Patch {
            before: Box::new(faded),
            after: Box::new(erased),
        }];
        let author = CmdAuthor::Agent("Dot".into());
        assert!(h.app.commit_scene_as(cmds.clone(), author.clone()));
        h.app.queue_targeted_erase_cleanup(&cmds, author);
        assert!(
            h.app.erase_settle.has_checks(),
            "an actual erase still schedules its attributed raster cleanup"
        );
    }
}
