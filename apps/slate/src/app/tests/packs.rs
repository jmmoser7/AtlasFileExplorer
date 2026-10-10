//! Pack registry (PK1) and the OpenAI "add API key" exception (PK2). Pack
//! health is pinned per test, so nothing here depends on what this machine
//! has installed.

use super::*;
use atlas_ai::packs::{PackHealth, ADD_KEY_LABEL, OPENAI};

const PACKS: [&str; 7] = [
    "cursor", "codex", "ollama", "comfy", OPENAI, "sam", "pdfium",
];

/// A board whose packs answer Ok only for `ok`; the startup probe has landed.
pub(crate) fn pack_board(tag: &str, ok: &[&str]) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    for id in PACKS {
        let health = if ok.contains(&id) {
            PackHealth::Ok
        } else {
            PackHealth::Missing
        };
        h.app.pin_pack_for_test(id, health);
    }
    h.app.ai.packs.refresh(None);
    settle(&mut h);
    h
}

pub(crate) fn settle(h: &mut Harness) {
    for _ in 0..500 {
        h.app.ensure_agent_programs();
        if !h.app.ai.packs.probe_pending() {
            h.app.ensure_agent_programs();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the pack probe did not finish");
}

pub(crate) fn look_at(h: &mut Harness, id: NodeId) {
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
}

/// An unbound agent portal in view: its program grid is the chooser.
pub(crate) fn chooser_in_view(h: &mut Harness) -> NodeId {
    h.app.place_agent_portal_at(Pos2::new(40.0, 40.0));
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    look_at(h, id);
    id
}

/// A picture with its Agent editor open and the model list dropped down.
pub(crate) fn generate_chooser_open(h: &mut Harness) -> NodeId {
    let picture = super::super::board_flow::tests::place(h, "site.png", -900.0);
    h.app.agent_blur();
    h.app.board_sel = std::iter::once(picture).collect();
    look_at(h, picture);
    h.frame();
    h.frame();
    h.app.shape_properties.panel = Some(super::super::board_properties::Panel::Agent);
    h.app.open_agent_model_menu_for_test(picture);
    picture
}

fn shown(h: &mut Harness) -> Vec<String> {
    // egui sizes a new area on its first frame without painting it.
    painted(h, Pos2::new(2.0, 890.0));
    painted(h, Pos2::new(2.0, 890.0))
}

#[test]
fn a_chooser_with_no_chat_pack_has_no_cursor_and_no_launch_button() {
    let mut h = pack_board("pk1_no_chat", &[]);
    chooser_in_view(&mut h);
    let texts = shown(&mut h);
    for absent in ["Cursor", "Codex", "Ollama", "Launch Cursor", "ComfyUI"] {
        assert!(!texts.iter().any(|t| t == absent), "{absent}: {texts:?}");
    }
    assert!(texts.iter().any(|t| t == ADD_KEY_LABEL), "{texts:?}");
    assert!(!h.app.ai.cursor_ok());
}

#[test]
fn a_cursor_install_shows_in_the_chooser_without_a_restart() {
    let mut h = pack_board("pk1_install", &[]);
    let portal = chooser_in_view(&mut h);
    let texts = shown(&mut h);
    assert!(!texts.iter().any(|t| t == "Cursor"), "{texts:?}");
    settle(&mut h);
    let probes = h.app.ai.packs.probes();

    // Cursor is installed while Slate runs; the chooser closes and opens again.
    h.app.ai.packs.set_health_override("cursor", PackHealth::Ok);
    h.app.tab_mut().cam.offset = egui::vec2(40_000.0, 40_000.0);
    shown(&mut h);
    look_at(&mut h, portal);
    let mut texts = Vec::new();
    for _ in 0..200 {
        texts = painted(&mut h, Pos2::new(2.0, 890.0));
        if texts.iter().any(|t| t == "Cursor") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        h.app.ai.packs.probes() > probes,
        "opening the chooser probed"
    );
    assert!(texts.iter().any(|t| t == "Cursor"), "{texts:?}");
    assert!(h.app.ai.cursor_ok());
}

#[test]
fn pk2_generate_offers_add_api_key_until_a_key_is_stored() {
    let mut h = pack_board("pk2_flip", &[]);
    let rows = h.app.agent_models(true);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].2, ADD_KEY_LABEL);
    assert!(h.app.ai.packs.needs_key(OPENAI));
    generate_chooser_open(&mut h);
    let texts = shown(&mut h);
    assert!(texts.iter().any(|t| t == ADD_KEY_LABEL), "{texts:?}");

    // What `save_openai_key` records once the key is in the store.
    h.app.pin_pack_for_test(OPENAI, PackHealth::Ok);
    let rows = h.app.agent_models(true);
    assert!(rows.iter().all(|r| r.2 != ADD_KEY_LABEL), "{rows:?}");
    assert!(
        rows.iter().all(|r| r.2.starts_with("OpenAI · ")),
        "{rows:?}"
    );
    let texts = shown(&mut h);
    assert!(!texts.iter().any(|t| t == ADD_KEY_LABEL), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.starts_with("OpenAI · GPT Image")),
        "{texts:?}"
    );
}

#[test]
fn pk2_key_entry_links_to_the_key_page_and_keeps_the_key_out_of_the_document() {
    let mut h = pack_board("pk2_key_window", &[]);
    h.app.ai.packs.key_entry = true;
    let texts = shown(&mut h);
    assert!(texts.iter().any(|t| t == "OpenAI API key"), "{texts:?}");
    assert!(
        texts
            .iter()
            .any(|t| t == "Get an API key at platform.openai.com"),
        "{texts:?}"
    );
    assert_eq!(
        atlas_ai::packs::KEY_URL,
        "https://platform.openai.com/api-keys"
    );
    h.app.ai.packs.key_draft = "placeholder-not-a-key".into();
    shown(&mut h);
    let path = h.base.join("key.slate");
    h.app.doc().save_to(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("placeholder-not-a-key"));
}

#[test]
fn a_missing_pack_document_keeps_its_poster_and_names_the_pack() {
    let mut h = pack_board("missing_pack_poster", &["comfy"]);
    let id = chooser_in_view(&mut h);
    h.app.set_agent_program(id, "comfy");
    let poster = h.base.join("last-poster.png");
    image::RgbImage::new(8, 8).save(&poster).unwrap();
    h.app.patch_nodes(&[id], |n| {
        slate_doc::agent_chat::agent_mut(n).unwrap().seed = Some(atlas_ai::agent::ImageOutput {
            id: "poster".into(),
            source: "last-poster.png".into(),
            ..Default::default()
        });
    });
    let path = h.base.join("missing-pack.slate");
    h.app.doc().save_to(&path).unwrap();

    h.app.pin_pack_for_test("comfy", PackHealth::Missing);
    let loaded = slate_doc::SlateDoc::load_from(&path).expect("the document opens");
    let seed = loaded
        .scene
        .nodes
        .iter()
        .find_map(|n| slate_doc::agent_chat::agent(n).and_then(|a| a.seed.clone()))
        .expect("the poster is kept");
    assert_eq!(seed.source, "last-poster.png");
    let caption = h.app.pack_caption("comfy").expect("caption");
    assert_eq!(caption, "ComfyUI · Missing");
    let texts = shown(&mut h);
    assert!(texts.iter().any(|t| t == "ComfyUI · Missing"), "{texts:?}");
}
