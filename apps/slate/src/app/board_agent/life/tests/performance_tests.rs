//! Repeatable source-session pump benchmark using the lifecycle fixtures.
use super::*;

/// A train of `cards` exchange cards over one saved `exchanges`-long
/// Cursor conversation, with a cropped picture wired to its tail.
fn long_train(
    h: &mut Harness,
    ws: &std::path::Path,
    exchanges: usize,
    cards: usize,
) -> Vec<NodeId> {
    use slate_doc::scene::{ConnectorEnd, Crop, ImageNode, Side, WorldRect};
    let first = cursor_chat(h, "conv-long");
    let dir = h.app.agent_link_dir(first, ws).unwrap();
    write_session(&dir, &history("conv-long", exchanges));
    let per = exchanges * 2 / cards;
    let window = |i: usize| (i * per, (i + 1 < cards).then_some((i + 1) * per));
    let view = |n: &mut Node, parent, (start, end): (usize, Option<usize>)| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let chat = &mut p.agent.as_mut().unwrap().chat;
            chat.train = true;
            chat.linear = true;
            chat.parent = parent;
            chat.start = start;
            chat.end = end;
            chat.detail = slate_doc::agent_chat::Detail::Pair;
        }
    };
    h.app.patch_nodes(&[first], |n| view(n, None, window(0)));
    let mut ids = vec![first];
    for i in 1..cards {
        let prev = *ids.last().unwrap();
        let original = h.app.doc().scene.node(prev).unwrap().clone();
        let mut next = h.app.doc_mut().scene.build_duplicate(&original, 420.0, 0.0);
        view(&mut next, Some(prev), window(i));
        ids.push(h.app.add_nodes(vec![next])[0]);
    }
    // A camera-sized photo with detail, as a person wires one.
    let src = h.base.join("photo.jpg");
    image::RgbImage::from_fn(4032, 3024, |x, y| {
        let v = x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503);
        image::Rgb([(x ^ y) as u8, (v >> 8) as u8, (x / 7 + y / 5) as u8])
    })
    .save(&src)
    .unwrap();
    let item = h.app.add_paths(std::slice::from_ref(&src))[0];
    let mut picture = h.app.doc_mut().scene.build_node(
        WorldRect::new(-600.0, 0.0, 200.0, 100.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    if let NodeKind::Image(img) = &mut picture.kind {
        img.crop = Crop {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
    }
    let picture = h.app.add_nodes(vec![picture])[0];
    let anchored = |node, side| ConnectorEnd::Anchored { node, side, t: 0.5 };
    h.app
        .add_connector(anchored(picture, Side::Right), anchored(first, Side::Left))
        .unwrap();
    ids
}

fn spread(samples: &mut [f64]) -> String {
    samples.sort_by(f64::total_cmp);
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    format!(
        "mean {mean:.3} ms, p95 {p95:.3} ms, max {:.3} ms (n={})",
        samples.last().unwrap(),
        samples.len()
    )
}

/// Frame-loop cost of the sessions pass for a 200-message conversation on
/// ten cards: steady frames, context-publish frames, and streamed updates.
/// `cargo test -p slate --release agent_sessions_pass_bench -- --ignored --nocapture`
#[test]
#[ignore]
fn agent_sessions_pass_bench() {
    let (mut h, ws) = linked_board("agent_sessions_bench");
    let eager = std::env::var_os("SLATE_SESSION_EAGER").is_some();
    h.app.legacy_input_projection_for_test(eager);
    let cards = std::env::var("SLATE_SESSION_CARDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10usize);
    let exchanges = std::env::var("SLATE_SESSION_EXCHANGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100usize);
    assert!((1..=400).contains(&cards) && exchanges >= cards && exchanges <= 1000);
    let ids = long_train(&mut h, &ws, exchanges, cards);
    let distinct = std::env::var_os("SLATE_SESSION_DISTINCT").is_some();
    if distinct {
        for (index, &id) in ids.iter().enumerate().skip(1) {
            let dir = ws.join(format!("synthetic-source-{index}"));
            let source = dir.join("session.json").to_string_lossy().into_owned();
            h.app.patch_nodes(&[id], |node| {
                let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
                agent.session = format!("synthetic-distinct-{index}");
                agent.bundle = Some(slate_doc::SourceUri {
                    locator: source.clone(),
                });
                agent.chat.parent = None;
                agent.chat.start = 0;
                agent.chat.end = None;
            });
            write_session(&dir, &history("conv-long", exchanges));
        }
    }
    let publish_dir = h.app.agent_link_dir(ids[0], &ws).unwrap();
    let last = *ids.last().unwrap();
    let dir = h.app.agent_link_dir(last, &ws).unwrap();
    frames_until(&mut h, "the saved history", |h| {
        shows(h, last, &format!("answer {}", exchanges - 1))
    });
    let settle = Instant::now() + Duration::from_secs(2);
    while Instant::now() < settle {
        h.frame();
        std::thread::sleep(Duration::from_millis(10));
    }
    let ctx = h.ctx.clone();
    let pass = |h: &mut Harness, publish: bool| {
        if publish {
            h.app.agents.context_tick = None;
        }
        let t = Instant::now();
        h.app.pump_agent_sessions(&ctx, &ws);
        t.elapsed().as_secs_f64() * 1000.0
    };
    let mut steady: Vec<f64> = (0..300)
        .map(|_| {
            std::thread::sleep(Duration::from_millis(2));
            pass(&mut h, false)
        })
        .collect();
    let mut publishing: Vec<f64> = (0..100)
        .map(|_| {
            std::thread::sleep(Duration::from_millis(5));
            pass(&mut h, true)
        })
        .collect();
    let context = std::fs::read_to_string(publish_dir.join("context.json")).unwrap();
    assert!(
        context.contains("slate-crop"),
        "the publish carries the clip"
    );
    let mut updates = Vec::new();
    let mut exchanges = exchanges;
    for _ in 0..50 {
        exchanges += 1;
        let mut next = history("conv-long", exchanges);
        next.turns.pop();
        write_session(&dir, &next);
        let read = h.app.agents.sessions.get(&last).cloned();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "the update never arrived");
            let ms = pass(&mut h, false);
            let now = h.app.agents.sessions.get(&last).cloned();
            if now.as_ref().map(std::sync::Arc::as_ptr) != read.as_ref().map(std::sync::Arc::as_ptr)
            {
                updates.push(ms);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        h.frame();
    }
    if let Ok(label) = std::env::var("SLATE_SESSION_LABEL") {
        assert!(label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/performance-recovery/20261031");
        std::fs::create_dir_all(&root).unwrap();
        let report = serde_json::json!({"cards": cards, "equivalent_eager_control": eager, "distinct_sources": distinct, "exchanges": exchanges - 50,
            "steady_ms": steady, "publishing_ms": publishing, "updates_ms": updates,
            "caveat": "CPU session pump only; synthetic saved local source and cropped image; no GPU/present or paid provider."});
        std::fs::write(
            root.join(format!("sessions-{label}-{cards}.json")),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
    eprintln!("sessions pass, steady:     {}", spread(&mut steady));
    eprintln!("sessions pass, publishing: {}", spread(&mut publishing));
    eprintln!("sessions pass, update:     {}", spread(&mut updates));
}
