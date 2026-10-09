//! Fake web host and portal settle helpers.

use super::*;

/// A host that reports a working runtime and hands out a solid frame, so the
/// pool, the states, input routing, and bake can all be driven without a
/// browser. The log is shared so a test can read what the page was sent.
#[derive(Default)]
pub(crate) struct FakeLog {
    pub(crate) escape: bool,
    pub(crate) admitted: std::collections::HashSet<slate_doc::NodeId>,
    pub(crate) admit_targets: Vec<(slate_doc::NodeId, String)>,
    pub(crate) admit_profiles: Vec<(slate_doc::NodeId, String)>,
    pub(crate) inputs: Vec<board_web::WebInput>,
    pub(crate) current_urls: std::collections::HashMap<slate_doc::NodeId, String>,
    pub(crate) navigations: Vec<(slate_doc::NodeId, String)>,
}

#[derive(Default, Clone)]
pub(crate) struct FakeWebHost(pub(crate) std::rc::Rc<std::cell::RefCell<FakeLog>>);

impl FakeWebHost {
    pub(crate) fn inputs(&self) -> Vec<board_web::WebInput> {
        self.0.borrow().inputs.clone()
    }
    pub(crate) fn navigations(&self) -> Vec<(slate_doc::NodeId, String)> {
        self.0.borrow().navigations.clone()
    }
    pub(crate) fn admit_targets(&self) -> Vec<(slate_doc::NodeId, String)> {
        self.0.borrow().admit_targets.clone()
    }
    pub(crate) fn set_current_url(&self, id: slate_doc::NodeId, url: &str) {
        self.0.borrow_mut().current_urls.insert(id, url.to_string());
    }
    pub(crate) fn current_url(&self, id: slate_doc::NodeId) -> Option<String> {
        self.0.borrow().current_urls.get(&id).cloned()
    }
    pub(crate) fn sent<T>(&self, pick: impl Fn(&board_web::WebInput) -> Option<T>) -> Vec<T> {
        self.inputs().iter().filter_map(pick).collect()
    }
}

impl board_web::WebHost for FakeWebHost {
    fn take_escape(&mut self) -> bool {
        std::mem::take(&mut self.0.borrow_mut().escape)
    }
    fn available(&self) -> bool {
        true
    }
    fn admit(&mut self, id: slate_doc::NodeId, req: &board_web::WebRequest) {
        let mut log = self.0.borrow_mut();
        log.admitted.insert(id);
        log.admit_targets.push((id, req.target.clone()));
        log.admit_profiles.push((id, req.profile.clone()));
        log.current_urls
            .entry(id)
            .or_insert_with(|| req.target.clone());
    }
    fn evict(&mut self, id: slate_doc::NodeId) {
        self.0.borrow_mut().admitted.remove(&id);
    }
    fn take_frame(&mut self, id: slate_doc::NodeId) -> Option<board_web::WebFrame> {
        self.0
            .borrow()
            .admitted
            .contains(&id)
            .then(|| egui::ColorImage::new([8, 8], egui::Color32::from_rgb(30, 90, 160)).into())
    }
    fn capture_poster(&mut self, _id: slate_doc::NodeId) -> Option<board_web::WebFrame> {
        Some(egui::ColorImage::new([8, 8], egui::Color32::from_rgb(30, 90, 160)).into())
    }
    fn send_input(&mut self, _id: slate_doc::NodeId, input: board_web::WebInput) {
        self.0.borrow_mut().inputs.push(input);
    }
    fn cursor(&self, _id: slate_doc::NodeId) -> Option<egui::CursorIcon> {
        None
    }
    fn load_error(&self, _id: slate_doc::NodeId) -> Option<String> {
        None
    }
    fn current_url(&self, id: slate_doc::NodeId) -> Option<String> {
        self.0.borrow().current_urls.get(&id).cloned()
    }
    fn navigate(&mut self, id: slate_doc::NodeId, target: &str) -> bool {
        let mut log = self.0.borrow_mut();
        log.navigations.push((id, target.to_string()));
        log.current_urls.insert(id, target.to_string());
        true
    }
}

pub(crate) fn web_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.kits = kits::KitState::builtin_only();
    h
}

pub(crate) fn with_fake_host(h: &mut Harness) -> FakeWebHost {
    let host = FakeWebHost::default();
    h.app.web.set_host(Box::new(host.clone()));
    host
}

/// Report the portals as painted at a given on-screen height, which is what
/// the pool sorts by, and run frames until the pipeline settles. Geometry is
/// normally recorded during painting; a headless harness supplies it directly,
/// and it only sticks once the pump has made the derived view.
pub(crate) fn web_settle(h: &mut Harness, sizes: &[(slate_doc::NodeId, f32)], frames: usize) {
    let clip = ERect::from_min_size(Pos2::ZERO, EVec2::new(4000.0, 4000.0));
    for _ in 0..frames {
        for (id, height) in sizes {
            let r = ERect::from_min_size(Pos2::ZERO, EVec2::new(height * 1.78, *height));
            h.app.note_web_geometry(*id, r, clip, 1.0);
        }
        h.frame();
    }
}

pub(crate) fn only_portal(h: &Harness) -> (slate_doc::NodeId, slate_doc::scene::PortalNode) {
    let node = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| matches!(&n.kind, NodeKind::Portal(p) if p.kind == slate_doc::scene::PortalKind::Web))
        .expect("a web portal on the board");
    let NodeKind::Portal(p) = &node.kind else {
        unreachable!()
    };
    (node.id, p.clone())
}
