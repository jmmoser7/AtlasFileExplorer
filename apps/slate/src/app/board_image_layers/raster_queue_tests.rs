use super::*;

fn injected() -> (
    ImageRasterJobs,
    crossbeam_channel::Sender<RasterCompletion>,
    crossbeam_channel::Receiver<RasterRequest>,
) {
    let (tx, requests) = crossbeam_channel::bounded(1);
    let (results, rx) = crossbeam_channel::unbounded();
    (
        ImageRasterJobs {
            tx,
            rx,
            pending_hosts: HashSet::new(),
            pending_thumbs: HashSet::new(),
        },
        results,
        requests,
    )
}

fn input(size: (u32, u32)) -> RasterInput {
    let mut doc = slate_doc::SlateDoc::new("queue fixture");
    let image = ImageNode::new(slate_doc::ItemId::NONE);
    let host = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 10.0, 10.0),
        NodeKind::Image(image.clone()),
    );
    RasterInput::new(
        host,
        image,
        Arc::new(std::sync::Mutex::new(doc)),
        None,
        size,
    )
}

fn complete(
    jobs: &mut ImageRasterJobs,
    results: &crossbeam_channel::Sender<RasterCompletion>,
    node: u64,
    ready: bool,
) {
    jobs.pending_hosts.insert((NodeId(node), node as u128));
    results
        .send(RasterCompletion {
            target: RasterTarget::Host {
                node: NodeId(node),
                key: node as u128,
            },
            w: 1,
            h: 1,
            rgba: ready.then(|| vec![0; 4]),
        })
        .unwrap();
}

#[test]
fn five_ready_rasters_are_drained_over_three_frames() {
    let ctx = egui::Context::default();
    let (mut jobs, results, _requests) = injected();
    for node in 0..5 {
        complete(&mut jobs, &results, node, true);
    }
    assert_eq!(jobs.drain(&ctx).len(), 2);
    assert_eq!(jobs.pending_hosts.len(), 3);
    assert_eq!(jobs.rx.len(), 3);
    assert_eq!(jobs.drain(&ctx).len(), 2);
    assert_eq!(jobs.pending_hosts.len(), 1);
    assert_eq!(jobs.drain(&ctx).len(), 1);
    assert!(jobs.pending_hosts.is_empty());
    assert!(jobs.rx.is_empty());
}

#[test]
fn failed_rasters_have_a_bookkeeping_budget_and_release_inflight_keys() {
    let ctx = egui::Context::default();
    let (mut jobs, results, _requests) = injected();
    for node in 0..70 {
        complete(&mut jobs, &results, node, false);
    }
    assert_eq!(jobs.drain(&ctx).len(), 32);
    assert!(!jobs.pending_hosts.contains(&(NodeId(0), 0)));
    assert_eq!(jobs.pending_hosts.len(), 38);
    assert_eq!(jobs.drain(&ctx).len(), 32);
    assert_eq!(jobs.drain(&ctx).len(), 6);
    assert!(jobs.pending_hosts.is_empty());
}

#[test]
fn full_or_disconnected_request_queue_never_adds_phantom_pending_work() {
    let ctx = egui::Context::default();
    let (mut jobs, _results, requests) = injected();
    assert!(jobs.request_host(NodeId(1), 10, input((1, 1)), ctx.clone()));
    assert!(!jobs.request_host(NodeId(2), 20, input((1, 1)), ctx.clone()));
    assert_eq!(jobs.pending_hosts.len(), 1);
    drop(requests);
    assert!(!jobs.request_host(NodeId(3), 30, input((1, 1)), ctx));
    assert_eq!(jobs.pending_hosts.len(), 1);
}

#[test]
fn disconnected_completion_queue_retires_both_pending_kinds() {
    let ctx = egui::Context::default();
    let (mut jobs, results, _requests) = injected();
    jobs.pending_hosts.insert((NodeId(1), 10));
    let key = (
        NodeId(2),
        ImageResultId::Item(slate_doc::ItemId::NONE),
        PaintLayerId(3),
    );
    jobs.pending_thumbs.insert((key, 4));
    drop(results);
    assert!(jobs.drain(&ctx).is_empty());
    assert!(jobs.pending_hosts.is_empty());
    assert!(jobs.pending_thumbs.is_empty());
}

#[test]
fn invalid_raster_dimensions_and_crop_never_enter_the_worker_queue() {
    let ctx = egui::Context::default();
    let (mut jobs, _results, requests) = injected();
    assert!(!jobs.request_host(NodeId(1), 1, input((0, 1)), ctx.clone()));
    assert!(!jobs.request_host(NodeId(1), 1, input((4097, 1)), ctx.clone()));
    jobs.request_thumb(
        (
            NodeId(1),
            ImageResultId::Item(slate_doc::ItemId::NONE),
            PaintLayerId(1),
        ),
        1,
        input((1, 1)),
        2,
        ctx,
    );
    assert!(requests.is_empty());
    assert!(jobs.pending_hosts.is_empty());
    assert!(jobs.pending_thumbs.is_empty());
}

#[test]
fn completed_raster_channel_has_only_two_ready_slots() {
    let jobs = ImageRasterJobs::new();
    assert_eq!(jobs.rx.capacity(), Some(2));
}

#[test]
fn malformed_raster_crop_is_a_failure_instead_of_a_worker_panic() {
    assert_eq!(crop_square(&[0; 15], 2, 0, 0, 2), None);
    assert_eq!(crop_square(&[0; 16], 0, 0, 0, 1), None);
    assert_eq!(crop_square(&[0; 16], 2, 1, 0, 2), None);
    assert_eq!(crop_square(&[0; 16], 2, 0, 0, 2), Some(vec![0; 16]));
}
