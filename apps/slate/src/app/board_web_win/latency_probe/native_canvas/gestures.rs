//! Deterministic normal native input events, confined to the owned probe.
use crate::app::SlateApp;
use eframe::egui::{self, Vec2};

pub(super) fn input(
    app: &SlateApp,
    step: usize,
    live_model: bool,
) -> Option<(Vec<egui::Event>, Option<&'static str>, egui::Modifiers)> {
    let center = app.canvas_rect.center();
    let modifiers = egui::Modifiers {
        shift: live_model && step <= 41,
        ..Default::default()
    };
    let button = if live_model {
        egui::PointerButton::Secondary
    } else {
        egui::PointerButton::Middle
    };
    let middle = |pressed| egui::Event::PointerButton {
        pos: center,
        button,
        pressed,
        modifiers,
    };
    let (events, phase) = match step {
        0 => (vec![egui::Event::PointerMoved(center), middle(true)], None),
        1..=40 => (
            vec![egui::Event::PointerMoved(
                center
                    + Vec2::new(
                        (step as f32 * 0.17).sin() * 100.0,
                        (step as f32 * 0.13).cos() * 70.0,
                    ),
            )],
            Some("pan"),
        ),
        41 => (vec![middle(false)], None),
        42..=81 => (
            vec![
                egui::Event::PointerMoved(center),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, if step < 62 { 20.0 } else { -20.0 }),
                    modifiers: Default::default(),
                },
            ],
            Some("zoom"),
        ),
        82..=121 if !live_model => {
            let mut targets: Vec<_> = app
                .doc()
                .scene
                .nodes
                .iter()
                .filter(|node| matches!(&node.kind, slate_doc::NodeKind::Image(_)))
                .filter_map(|node| {
                    let (x, y) = node.rect.center();
                    let point = app.world_to_screen(egui::pos2(x, y));
                    app.canvas_rect
                        .shrink(40.0)
                        .contains(point)
                        .then_some(point)
                })
                .collect();
            targets.sort_by(|a, b| a.distance_sq(center).total_cmp(&b.distance_sq(center)));
            let p = if targets.len() >= 2 {
                targets[(step - 82) / 2 % 2]
            } else {
                center
            };
            (
                vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed: step % 2 == 0,
                        modifiers: Default::default(),
                    },
                ],
                Some("selection"),
            )
        }
        _ => return None,
    };
    Some((events, phase, modifiers))
}

/// A bounded, independent producer timestamps a request before waking the real
/// native event loop. A busy UI cannot postpone the measurement's start.
pub(super) struct Delivered {
    pub events: Vec<egui::Event>,
    pub phase: Option<&'static str>,
    pub modifiers: egui::Modifiers,
    pub issued: std::time::Instant,
    pub issued_clock_100ns: Option<i64>,
}

type Job = (Vec<egui::Event>, Option<&'static str>, egui::Modifiers);
pub(super) struct Driver {
    jobs: Option<crossbeam_channel::Sender<Job>>,
    ready: crossbeam_channel::Receiver<Delivered>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Driver {
    pub fn new(ctx: egui::Context) -> Self {
        let (send, jobs) = crossbeam_channel::bounded::<Job>(1);
        let (ready, receive) = crossbeam_channel::bounded(1);
        let thread = std::thread::spawn(move || {
            let mut index = 0u64;
            while let Ok((events, phase, modifiers)) = jobs.recv() {
                // Deterministic jitter exercises arrival during existing work;
                // it is outside the measured request->capture interval.
                std::thread::sleep(std::time::Duration::from_millis(5 + index * 7 % 17));
                index += 1;
                let issued = std::time::Instant::now();
                let issued_clock_100ns = super::super::clock::now_100ns();
                if ready
                    .send(Delivered {
                        events,
                        phase,
                        modifiers,
                        issued,
                        issued_clock_100ns,
                    })
                    .is_err()
                {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            jobs: Some(send),
            ready: receive,
            thread: Some(thread),
        }
    }
    pub fn schedule(&self, job: Job) {
        self.jobs
            .as_ref()
            .unwrap()
            .try_send(job)
            .expect("one owned input request in flight");
    }
    pub fn take(&self) -> Option<Delivered> {
        self.ready.try_recv().ok()
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
