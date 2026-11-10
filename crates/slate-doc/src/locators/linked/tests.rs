use super::*;
use crate::scene::{PortalNode, SourceUri, TextNode, WorldRect};

#[test]
fn text_answer_origins_rebase_but_remote_and_absolute_history_keep_identity() {
    let from = std::env::temp_dir().join("origin-old/Board.slate");
    let to = std::env::temp_dir().join("origin-new/Board.slate");
    let mut agent = *PortalNode::unbound_agent("text", "codex").agent.unwrap();
    agent.generator.owned_text_output = Some(SourceUri {
        locator: "private/response.txt".into(),
    });
    let absolute = std::env::temp_dir().join("absolute-history.json");
    agent.generator.archived_history = vec![
        SourceUri {
            locator: "https://example.com/history".into(),
        },
        SourceUri {
            locator: absolute.to_string_lossy().into(),
        },
    ];
    let mut text = TextNode::agent_note(agent);
    let mut answer = crate::text_answers::TextAnswer::new(
        "x",
        0..1,
        crate::text_answers::TextAnswerRef {
            session: "fixture".into(),
            through: 1,
        },
        "q".into(),
        "a".into(),
    )
    .unwrap();
    answer.origin = Some(SourceUri {
        locator: "private/answer.json".into(),
    });
    text.answers.push(answer);
    let mut scene = scene::Scene::default();
    let node = scene.build_node(WorldRect::new(0., 0., 100., 100.), NodeKind::Text(text));
    scene.nodes.push(node);
    assert!(rebase_linked_scene(&mut scene, &from, &to));
    let NodeKind::Text(text) = &scene.nodes[0].kind else {
        panic!()
    };
    assert_eq!(
        scene::resolve_source(
            Some(&to),
            &text
                .agent
                .as_ref()
                .unwrap()
                .generator
                .owned_text_output
                .as_ref()
                .unwrap()
                .locator
        ),
        from.parent().unwrap().join("private/response.txt")
    );
    assert_eq!(
        scene::resolve_source(Some(&to), &text.answers[0].origin.as_ref().unwrap().locator),
        from.parent().unwrap().join("private/answer.json")
    );
    assert_eq!(
        text.agent.as_ref().unwrap().generator.archived_history[0].locator,
        "https://example.com/history"
    );
    assert_eq!(
        Path::new(&text.agent.as_ref().unwrap().generator.archived_history[1].locator),
        absolute
    );
}
