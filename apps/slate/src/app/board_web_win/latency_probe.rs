//! A real local page changes color on pointer input. No remote service or
//! dashboard instrumentation is needed to measure input-to-captured-frame time.

use super::super::board_web::{WebHost, WebInput, WebRequest};
use super::*;
use std::time::{Duration, Instant};

fn wait_for(host: &mut Webview2Host, id: NodeId, green: bool) -> Option<Duration> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        probe::pump();
        if let Some(frame) = host.take_frame(id) {
            let p = frame.pixels[frame.pixels.len() / 2];
            if if green {
                p.g() > 180 && p.r() < 80
            } else {
                p.r() > 180 && p.g() < 80
            } {
                return Some(started.elapsed());
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    None
}

#[test]
#[ignore = "requires a Windows desktop, GPU compositor, and WebView2 runtime"]
fn local_dashboard_pointer_latency() {
    let dir = std::env::temp_dir().join(format!("slate-pointer-latency-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("pointer.html");
    std::fs::write(
        &page,
        r#"<!doctype html><html><body style="margin:0;background:rgb(255,0,0)">
        <script>addEventListener('pointermove',e=>{
        document.body.style.background=e.clientX<160?'rgb(255,0,0)':'rgb(0,255,0)';
        });</script></body></html>"#,
    )
    .unwrap();
    let mut host = probe::host("pointer-latency")
        .expect("pointer latency probe needs WebView2, D3D11, and Windows composition");
    // A hidden parent triggers Chromium occlusion throttling and does not
    // represent Slate's visible window. Show only this probe's owned HWND.
    let parent = host.parent;
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
            parent,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
        );
    }
    let id = NodeId(998);
    let req = WebRequest {
        target: page.to_string_lossy().into_owned(),
        kind: slate_doc::scene::WebSourceKind::LocalFile,
        width_css: 320,
        height_css: 200,
        raster_w: 320,
        raster_h: 200,
        link_script: String::new(),
        anchor_px: None,
        profile: "local".into(),
    };
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut ready = false;
    while Instant::now() < deadline {
        host.admit(id, &req);
        if wait_for(&mut host, id, false).is_some() {
            ready = true;
            break;
        }
    }
    assert!(
        ready,
        "local fixture never painted: {:?}",
        host.load_error(id)
    );
    let mut samples = Vec::new();
    for i in 0..20 {
        let green = i % 2 == 0;
        let input_at = Instant::now();
        host.send_input(
            id,
            WebInput::Move {
                x: if green { 240.0 } else { 80.0 },
                y: 100.0,
                buttons: 0,
            },
        );
        wait_for(&mut host, id, green).expect("pointer input never reached the page");
        samples.push(input_at.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    println!("local pointer → captured frame: n={} median={:.2}ms p95={:.2}ms max={:.2}ms (8ms polling; native host only)",
        samples.len(), (samples[9] + samples[10]) * 0.5, samples[18], samples[19]);
    host.evict(id);
    drop(host);
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(parent);
    }
    let _ = std::fs::remove_file(page);
}
