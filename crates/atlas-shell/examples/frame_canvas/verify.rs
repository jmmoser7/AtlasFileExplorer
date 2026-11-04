use super::*;
use atlas_shell::frame_raster::FrameRaster;
use std::path::Path;

pub(super) struct Harness {
    pub app: Specimen,
    ctx: egui::Context,
    raster: FrameRaster,
    size: Rect,
    time: f64,
    modifiers: egui::Modifiers,
}

impl Harness {
    pub fn new(large: bool) -> Self {
        let width = if large { 780 } else { 1280 };
        Self {
            app: Specimen::new(large),
            ctx: egui::Context::default(),
            raster: FrameRaster::new(width, 720),
            size: Rect::from_min_size(Pos2::ZERO, vec2(width as f32, 720.0)),
            time: 0.0,
            modifiers: egui::Modifiers::NONE,
        }
    }
    pub fn step(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 1.0 / 60.0;
        self.ctx.run(
            egui::RawInput {
                screen_rect: Some(self.size),
                time: Some(self.time),
                modifiers: self.modifiers,
                events,
                ..Default::default()
            },
            |ctx| self.app.show(ctx),
        )
    }
    pub fn click(&mut self, pos: Pos2) {
        self.step(vec![
            egui::Event::PointerMoved(pos),
            button(pos, true, self.modifiers),
        ]);
        self.step(vec![button(pos, false, self.modifiers)]);
        self.step(vec![]);
    }
    pub fn key(&mut self, key: Key) {
        self.step(vec![egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: self.modifiers,
        }]);
        self.step(vec![egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: false,
            repeat: false,
            modifiers: self.modifiers,
        }]);
    }
    pub fn open(&mut self) {
        let hit = self.app.metrics.as_ref().unwrap().0.hit(3);
        self.click(hit.center());
        assert!(self.app.drawer);
    }
    pub fn capture(&mut self, path: &Path) {
        self.step(vec![egui::Event::PointerGone]);
        let output = self.step(vec![]);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        self.raster.apply(&output.textures_delta);
        self.raster.sync_fonts(&self.ctx);
        self.raster.draw(&primitives);
        self.raster.save(path);
    }
}

fn button(pos: Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

pub(super) fn run(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let mut h = Harness::new(false);
    h.step(vec![]);
    assert!(h.app.metrics.as_ref().unwrap().0.blister.is_some());
    h.open();
    let layout = &h.app.metrics.as_ref().unwrap().1;
    assert!(layout.inline && layout.attached && layout.tiles.len() == 5);
    let host = h.app.frame(NodeId(1));
    assert_eq!(layout.rect.left(), host.left());
    assert_eq!(layout.rect.top(), host.bottom());
    assert_eq!(layout.rect.right(), host.right());
    h.capture(&dir.join("frame-five-native.png"));
    let add = h.app.metrics.as_ref().unwrap().1.membership.center();
    h.click(add);
    assert!(h.app.adding);
    h.click(h.app.canvas.left_top() + vec2(12.0, 12.0));
    assert!(!h.app.drawer && !h.app.adding);
    h.click(h.app.frame(NodeId(1)).center());
    h.open();
    h.click(h.app.metrics.as_ref().unwrap().1.selector.center());
    assert!(h.app.selector);
    h.key(Key::Escape);
    assert!(!h.app.drawer && !h.app.selector);
    let tiny = h.app.frame(NodeId(7)).center();
    h.click(tiny);
    assert!(h.app.metrics.as_ref().unwrap().0.blister.is_none());
    h.open();
    let (s, d) = h.app.metrics.as_ref().unwrap();
    assert!(!d.attached && d.rect.top() > s.rect.bottom());
    h.capture(&dir.join("frame-squircle-native.png"));
    println!("small native: selection strip, attachment, compact five, outside click, Esc, squircle clearance passed");

    // Real eframe input and canvas geometry at the browser's 100-slide width.
    let mut h = Harness::new(true);
    h.step(vec![]);
    h.open();
    let tiles = h.app.metrics.as_ref().unwrap().1.tiles.clone();
    assert_eq!(tiles.len(), 100);
    assert_eq!(h.app.metrics.as_ref().unwrap().1.columns, 10);
    assert!(tiles
        .iter()
        .all(|r| h.app.canvas.contains_rect(*r) && r.width() >= 20.0));
    let original = h.app.order.clone();
    h.click(tiles[46].center());
    h.modifiers = egui::Modifiers::SHIFT;
    h.click(tiles[47].center());
    h.modifiers = egui::Modifiers::COMMAND;
    h.click(tiles[52].center());
    h.modifiers = egui::Modifiers::NONE;
    assert_eq!(
        h.app.chosen,
        HashSet::from([NodeId(47), NodeId(48), NodeId(53)])
    );
    h.capture(&dir.join("frame-100-native.png"));
    let from = tiles[46].center();
    let to = tiles[10].left_center() + vec2(2.0, 0.0);
    h.step(vec![
        egui::Event::PointerMoved(from),
        button(from, true, h.modifiers),
    ]);
    for i in 1..=12 {
        h.step(vec![egui::Event::PointerMoved(
            from.lerp(to, i as f32 / 12.0),
        )]);
    }
    assert_eq!(h.app.drag, Some(NodeId(47)));
    assert_ne!(h.app.preview, original);
    assert_eq!(h.app.journal.undo_depth(), 0);
    h.step(vec![button(to, false, h.modifiers)]);
    h.step(vec![]);
    assert_eq!(
        &h.app.order[10..14],
        &[NodeId(47), NodeId(48), NodeId(53), NodeId(11)]
    );
    assert_eq!(h.app.journal.undo_depth(), 1);
    h.modifiers = egui::Modifiers::COMMAND;
    h.key(Key::Z);
    assert_eq!(h.app.order, original);
    h.key(Key::Y);
    assert_eq!(h.app.order[10], NodeId(47));
    h.modifiers = egui::Modifiers::NONE;
    h.key(Key::Delete);
    assert_eq!(h.app.order.len(), 97);
    assert!(h.app.scene.node(NodeId(47)).is_some());
    println!("large native: 100 visible tiles at 780 x 720, Shift/Ctrl selection, cross-row drag, single undo, redo and reference-only Delete passed");

    let mut h = Harness::new(false);
    h.step(vec![]);
    let original = h.app.order.clone();
    let other = h.app.scene.presentation_frames(Some(2));
    assert_eq!(h.app.scene.presentation_frames(None).len(), 7);
    h.open();
    h.click(h.app.metrics.as_ref().unwrap().1.membership.center());
    h.click(h.app.frame(NodeId(6)).center());
    assert_eq!(h.app.order.last(), Some(&NodeId(6)));
    assert_eq!(h.app.scene.presentation_frames(Some(2)), other);
    let a = h.app.artwork[&NodeId(2)][0];
    let f = h.app.scene.node(NodeId(2)).unwrap().rect;
    let content = h.app.scene.node(a).unwrap().rect;
    h.click(h.app.metrics.as_ref().unwrap().1.sort.center());
    let sorted = h.app.scene.node(NodeId(2)).unwrap().rect;
    let moved = h.app.scene.node(a).unwrap().rect;
    assert_ne!(f, sorted);
    assert_eq!(
        (moved.x - content.x, moved.y - content.y),
        (sorted.x - f.x, sorted.y - f.y)
    );
    assert_eq!(h.app.scene.presentation_frames(Some(2)), other);
    assert_eq!(h.app.order[..5], original);
    h.key(Key::F5);
    assert_eq!(h.app.media.current.as_ref().unwrap().id, NodeId(1));
    assert!(h.app.media.current.as_ref().unwrap().interactive);
    h.key(Key::ArrowRight);
    assert_eq!(h.app.media.current.as_ref().unwrap().id, NodeId(2));
    h.key(Key::Escape);
    assert!(h.app.presenting.is_none() && h.app.media.current.is_none());
    println!("named decks: shared references, legacy fallback, Sort with scene contents, playback order and deterministic media entry passed");
}
