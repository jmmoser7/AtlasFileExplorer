//! Pack registry: a missing pack keeps its poster, and the chooser probes again.

use super::*;

#[test]
fn a_missing_pack_document_keeps_its_poster_and_names_the_pack() {
    let mut h = Harness::new("missing_pack_poster");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_agent_portal_at(Pos2::new(40.0, 40.0));
    let id = h.app.doc().scene.nodes[0].id;
    h.app.set_agent_program(id, "comfy");
    {
        let node = h.app.doc_mut().scene.node_mut(id).unwrap();
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.seed = Some(atlas_ai::agent::ImageOutput {
            id: "poster".into(),
            source: "assets/last-poster.png".into(),
            ..Default::default()
        });
    }
    let path = h.base.join("missing-pack.slate");
    h.app.ensure_agent_programs();
    for _ in 0..50 {
        if !h.app.ai.packs.probe_pending() {
            break;
        }
        h.app.ensure_agent_programs();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.app.doc().save_to(&path).unwrap();
    let loaded = slate_doc::Document::load_from(&path).expect("document opens");
    let seed = loaded
        .scene
        .nodes
        .iter()
        .find_map(|n| slate_doc::agent_chat::agent(n).and_then(|a| a.seed.clone()))
        .expect("poster");
    assert_eq!(seed.source, "assets/last-poster.png");
    h.app
        .ai
        .packs
        .record("comfy", atlas_ai::packs::PackHealth::Missing);
    let caption = h.app.pack_caption("comfy").expect("caption");
    assert!(caption.contains("ComfyUI"), "{caption}");
    assert!(caption.contains("Missing"), "{caption}");
}

#[test]
fn opening_the_chooser_probes_again_without_a_restart() {
    let mut h = Harness::new("chooser_reprobe");
    h.app.leave_home();
    h.app.ensure_agent_programs();
    for _ in 0..50 {
        if !h.app.ai.packs.probe_pending() {
            break;
        }
        h.app.ensure_agent_programs();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let after_start = atlas_ai::packs::probes_started();
    assert!(after_start >= 1, "startup probe ran");
    h.app.ai.packs.begin_frame();
    h.app.note_program_chooser();
    for _ in 0..50 {
        if !h.app.ai.packs.probe_pending() {
            break;
        }
        h.app.ensure_agent_programs();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        atlas_ai::packs::probes_started() > after_start,
        "opening the chooser probed again"
    );
}
