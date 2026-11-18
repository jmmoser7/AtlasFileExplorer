use super::*;
use std::path::PathBuf;

fn evaluated<T: serde::de::DeserializeOwned>(host: &Webview2Host, id: NodeId, expr: &str) -> T {
    serde_json::from_str(&eval(host, id, expr, 10).expect("WebView2 script completion")).unwrap()
}

fn send(host: &mut Webview2Host, id: NodeId, input: WebInput) {
    host.send_input(id, input);
    for _ in 0..4 {
        pump();
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn drag(host: &mut Webview2Host, id: NodeId, from: [f32; 2], to: [f32; 2]) {
    let [x, y] = from;
    send(host, id, WebInput::Move { x, y, buttons: 0 });
    send(host, id, WebInput::Down { x, y, button: 0 });
    let [x, y] = to;
    send(host, id, WebInput::Move { x, y, buttons: 1 });
    send(host, id, WebInput::Up { x, y, button: 0 });
}

fn output(host: &mut Webview2Host, id: NodeId) -> String {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        host.request_dashboard_outputs(id);
        pump();
        if let Some(Some(raw)) = host.take_dashboard_outputs(id) {
            let (snapshot, _) =
                slate_doc::dashboard_outputs::controller::observation(&raw).unwrap();
            let value = snapshot.packets.values().next().unwrap()["branches"][0]["items"][0]
                ["value"]
                .as_str()
                .unwrap()
                .to_owned();
            if value.split(',').count() == 2880 {
                return value;
            }
        }
        assert!(Instant::now() < deadline, "missing published spline field");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "requires a Windows desktop and WebView2"]
fn dashboard_outputs_real_spline_pointer_drag_add_remove_and_whole_curve() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../presentations/spatial-studies/controllers/Spline graph.html")
        .canonicalize()
        .unwrap();
    let mut host = host("spline-outputs").expect("WebView2 host");
    let id = NodeId(813);
    let mut req = local_request(&file);
    req.link_script = crate::app::board_web::controller_install_script(&req.target, None);
    req.width_css = 800;
    req.raster_w = 800;
    req.height_css = 480;
    req.raster_h = 480;
    assert!(pump_until(&mut host, id, &req, 35, |h| h
        .current_url(id)
        .is_some()));
    let first = output(&mut host, id);
    let xy = |h: &Webview2Host, expr: &str| evaluated::<[f32; 2]>(h, id, expr);
    let point = |h: &Webview2Host, x: f32, y: f32| {
        xy(h,&format!("(()=>{{const r=canvas.getBoundingClientRect();return [r.x+r.width*{x},r.y+r.height*{y}]}})()"))
    };
    let from = point(&host, 0.47, 0.66);
    let to = point(&host, 0.55, 0.37);
    drag(&mut host, id, from, to);
    let moved = output(&mut host, id);
    assert_ne!(first, moved);
    let button = |h: &Webview2Host, name: &str| {
        xy(h,&format!("(()=>{{const r=document.getElementById('{name}').getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()"))
    };
    let add = button(&host, "add");
    drag(&mut host, id, add, add);
    assert_eq!(evaluated::<usize>(&host, id, "curves.length"), 4);
    assert_ne!(output(&mut host, id), moved);
    let remove = button(&host, "remove");
    drag(&mut host, id, remove, remove);
    assert_eq!(evaluated::<usize>(&host, id, "curves.length"), 3);
    assert_eq!(output(&mut host, id), moved);
    let y = evaluated::<f32>(&host, id, "heightAt(curves[0],0.32)");
    let from = point(&host, 0.32, y);
    let to = point(&host, 0.32, y + 0.12);
    drag(&mut host, id, from, to);
    let final_field = output(&mut host, id);
    assert_ne!(final_field, moved);
    let state: serde_json::Value = evaluated(&host, id, "JSON.parse(el.dataset.slateEdit).state");
    let state = serde_json::from_value(state).unwrap();
    host.evict(id);
    req.link_script = crate::app::board_web::controller_install_script(&req.target, Some(&state));
    assert!(pump_until(&mut host, id, &req, 35, |h| h
        .current_url(id)
        .is_some()));
    assert_eq!(output(&mut host, id), final_field);
    assert!(!evaluated::<bool>(
        &host,
        id,
        "el.hasAttribute('data-slate-edit')"
    ));
    host.evict(id);
}

#[test]
#[ignore = "requires a Windows desktop and WebView2"]
fn dashboard_outputs_read_dynamic_json_from_real_webview2() {
    let page = std::env::temp_dir().join(format!(
        "slate-dashboard-outputs-{}.html",
        std::process::id()
    ));
    std::fs::write(&page, r#"<script id="slate-outputs" type="application/slate-outputs+json">{"version":1,"outputs":[]}</script>"#).unwrap();
    let mut browser = host("dashboard-outputs").expect("WebView2 host");
    let id = NodeId(815);
    let req = local_request(&page);
    assert!(pump_until(&mut browser, id, &req, 30, |h| h
        .current_url(id)
        .is_some()));
    let read = |browser: &mut Webview2Host| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            browser.request_dashboard_outputs(id);
            pump();
            if let Some(raw) = browser.take_dashboard_outputs(id) {
                break raw;
            }
            assert!(Instant::now() < deadline, "dashboard output read timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let first = read(&mut browser).unwrap();
    assert!(slate_doc::dashboard_outputs::parse(&first)
        .unwrap()
        .ports
        .is_empty());
    // The observer is installed before the script changes the element. A
    // stationary dashboard must leave the UI asleep even across several old
    // polling intervals.
    browser.request_dashboard_outputs(id);
    let quiet = Instant::now() + Duration::from_millis(350);
    while Instant::now() < quiet {
        pump();
        assert!(browser.take_dashboard_outputs(id).is_none());
        std::thread::sleep(Duration::from_millis(5));
    }
    eval(&browser,id,r#"document.querySelector('#slate-outputs').textContent=JSON.stringify({version:1,outputs:[{id:"1b993386-89d8-4b25-b102-452ea7120eb4",name:"Weights",type:"number",access:"list",value:[0.2,0.8]}]})"#,5).unwrap();
    let updated = slate_doc::dashboard_outputs::parse(&read(&mut browser).unwrap()).unwrap();
    assert_eq!(
        updated.packets.values().next().unwrap()["branches"][0]["items"][1]["value"],
        0.8
    );
    eval(
        &browser,
        id,
        "document.querySelector('#slate-outputs').remove()",
        5,
    )
    .unwrap();
    assert!(read(&mut browser).is_none());
    // Legacy controls can change only JavaScript state, with no DOM mutation.
    eval(
        &browser,
        id,
        "window.slateControl={__drain:()=>window.testControl??'first'}",
        5,
    )
    .unwrap();
    assert!(browser.request_controls(id));
    assert!(pump_until(&mut browser, id, &req, 5, |h| h.has_control_reply(id)));
    assert_eq!(browser.take_controls(id).unwrap().unwrap(), "first");
    assert!(browser.request_controls(id));
    let quiet = Instant::now() + Duration::from_millis(350);
    while Instant::now() < quiet {
        pump();
        assert!(!browser.has_control_reply(id));
        std::thread::sleep(Duration::from_millis(5));
    }
    eval(&browser, id, "window.testControl='second'", 5).unwrap();
    assert!(pump_until(&mut browser, id, &req, 5, |h| h.has_control_reply(id)));
    assert_eq!(browser.take_controls(id).unwrap().unwrap(), "second");
    browser.request_dashboard_outputs(id);
    assert!(browser.request_controls(id));
    let generation = browser.control_epoch(id);
    eval(&browser, id, "location.hash='while-watched'", 5).unwrap();
    assert!(pump_until(&mut browser, id, &req, 5, |h| h
        .control_epoch(id)
        != generation));
    // Identical values from a new page generation are a fresh sample. The
    // callbacks associated with the old receivers cannot satisfy these reads.
    assert!(read(&mut browser).is_none());
    assert!(browser.request_controls(id));
    assert!(pump_until(&mut browser, id, &req, 5, |h| h.has_control_reply(id)));
    assert_eq!(browser.take_controls(id).unwrap().unwrap(), "second");
    browser.request_dashboard_outputs(id);
    assert!(browser.request_controls(id));
    pump();
    // A normal window close can call script completions inside controller.Close().
    // Both watches are still live. Dropping the host must detach them first.
    drop(browser);
    pump();
    std::fs::remove_file(page).unwrap();
}
