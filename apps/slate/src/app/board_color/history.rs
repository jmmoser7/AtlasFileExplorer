//! One document-local history shared by tool and object color editors.

use super::{Node, NodeKind, SlateApp};

/// Color transitions at journal boundaries; geometry/alpha-only edits contribute nothing.
pub(crate) fn committed_colors<'a>(
    nodes: impl IntoIterator<Item = (Option<&'a Node>, &'a Node)>,
) -> Vec<[u8; 3]> {
    fn colors(node: &Node) -> [Option<[u8; 3]>; 3] {
        use slate_doc::scene::{fill_of, stroke_of};
        let text = match &node.kind {
            NodeKind::Text(text) => Some(text.color),
            NodeKind::Shape(shape) => shape.text.as_ref().map(|text| text.color),
            _ => None,
        };
        [
            fill_of(node),
            stroke_of(node).map(|stroke| stroke.color),
            text,
        ]
        .map(|color| color.map(|color| [color.0[0], color.0[1], color.0[2]]))
    }
    nodes
        .into_iter()
        .flat_map(|(before, after)| {
            let before = before.map(colors).unwrap_or([None; 3]);
            colors(after)
                .into_iter()
                .zip(before)
                .filter_map(|(after, before)| if after != before { after } else { None })
        })
        .collect()
}

impl SlateApp {
    pub(crate) fn seed_document_colors(&mut self) {
        if self.doc().view.recent_colors.is_none() {
            let colors = committed_colors(self.doc().scene.nodes.iter().map(|node| (None, node)));
            self.tab_mut().doc.view.seed_recent_colors(colors);
        }
    }

    pub(crate) fn remember_document_colors(&mut self, colors: Vec<[u8; 3]>) {
        self.seed_document_colors();
        let before = self.doc().view.recent_color_clock;
        for color in colors {
            self.tab_mut().doc.view.remember_color(color);
        }
        if self.doc().view.recent_color_clock != before {
            self.tab_mut().dirty = true;
        }
    }
}
