//! Sticky text creation owns editor handoff as well as the scene add.
use super::*;

impl SlateApp {
    // ---------- sticky note (N) ----------

    /// Click-to-place sticky: a Text-node preset (white fill).
    /// The caret enters immediately and the tool returns to Select, so the
    /// place ghost does not stay under the pointer.
    pub(crate) fn place_sticky_at(&mut self, world: Pos2) {
        if self.refuse_read_only_edit() || !self.finish_text_compose_on_click_away() {
            return;
        }
        let rect = WorldRect::new(
            world.x - STICKY_SIZE * 0.5,
            world.y - STICKY_SIZE * 0.5,
            STICKY_SIZE.max(MIN_DRAW),
            STICKY_SIZE.max(MIN_DRAW),
        );
        let id = self.insert_sticky(rect);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.text_edit = Some((id, String::new()));
        self.disarm_create();
        self.push_history(
            atlas_commands::CommandId("board.tool.sticky"),
            Some("placed".into()),
        );
    }

    fn insert_sticky(&mut self, rect: WorldRect) -> NodeId {
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Text(TextNode {
                text: String::new(),
                style: Default::default(),
                family: Typeface::Sans,
                size: 24.0,
                color: STICKY_INK,
                align: TextAlign::Center,
                fill: Some(STICKY_FILL),
                stroke: Default::default(),
                agent: None,
            }),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        id
    }

    /// Tab / Shift+Tab while editing a sticky: spawn a sibling to the right
    /// (left with Shift), same size/fill, 24-unit gap, caret moves there.
    pub(crate) fn spawn_adjacent_sticky(&mut self, from: NodeId, dir: f32) {
        if self.refuse_read_only_edit() || !self.finish_text_compose_on_click_away() {
            return;
        }
        let Some(src) = self.doc().scene.node(from).cloned() else {
            return;
        };
        let NodeKind::Text(t) = &src.kind else {
            return;
        };
        let fill = t.fill;
        let style = t.style;
        let (family, size, color, align) = (t.family, t.size, t.color, t.align);
        let rect = src.rect.translated(dir * (src.rect.w + STICKY_GAP), 0.0);
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Text(TextNode {
                text: String::new(),
                style,
                family,
                size,
                color,
                align,
                fill,
                stroke: Default::default(),
                agent: None,
            }),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.text_edit = Some((id, String::new()));
        self.push_history(
            atlas_commands::CommandId("board.tool.sticky"),
            Some(
                if dir >= 0.0 {
                    "tab-spawn right"
                } else {
                    "tab-spawn left"
                }
                .into(),
            ),
        );
    }
}
