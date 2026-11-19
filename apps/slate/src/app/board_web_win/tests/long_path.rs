use super::*;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Storage::FileSystem::GetShortPathNameW;

#[test]
#[ignore = "requires Windows WebView2 and a composition desktop"]
fn deep_local_dashboard_keeps_its_origin_relative_assets_and_typed_output() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("slate-deep-web-{unique}"));
    let mut folder = root.clone();
    for i in 0..48 {
        folder.push(format!("d{i:06}"));
    }
    std::fs::create_dir_all(&folder).unwrap();
    let page = folder.join("page.htm");
    let body = r#"<!doctype html><link rel="stylesheet" href="style.css?v=1">
    <script id="slate-outputs" type="application/slate-outputs+json">{"version":1,"outputs":[{"id":"4f09331f-a7fd-4236-9f0e-49801a01e777","name":"Fin depth","type":"number","access":"item","value":60}]}</script>
    <input type="range" min="20" max="200" value="60"><img src="./mark.svg"><script src="./app.js"></script>"#;
    std::fs::write(&page, body).unwrap();
    std::fs::write(
        folder.join("style.css"),
        "body {background-color:rgb(12, 34, 56)}",
    )
    .unwrap();
    std::fs::write(folder.join("mark.svg"),r#"<svg xmlns="http://www.w3.org/2000/svg" width="7" height="5"><rect width="7" height="5"/></svg>"#).unwrap();
    std::fs::write(folder.join("app.js"),r#"window.loadedSibling=true;const e=document.querySelector('#slate-outputs'),s=document.querySelector('input');s.oninput=()=>{let o=JSON.parse(e.textContent);o.outputs[0].value=s.valueAsNumber;e.textContent=JSON.stringify(o)}"#).unwrap();
    let page = page.canonicalize().unwrap();
    let wide = page
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let aliases = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), None) };
    assert!(
        aliases == 0 || aliases > 264,
        "fixture unexpectedly has a short browser path"
    );
    let req = local_request(&page);
    let mut host = host("long-file-dashboard").expect("WebView2 host");
    let id = NodeId(981);
    assert!(pump_until(&mut host, id, &req, 45, |h| h
        .current_url(id)
        .is_some()));
    let expected = navigate_uri(&req.target).unwrap();
    assert_eq!(
        eval(&host, id, "location.href", 5),
        Some(serde_json::to_string(&expected).unwrap())
    );
    let proof = eval(&host,id,"[loadedSibling, getComputedStyle(document.body).backgroundColor,document.images[0].naturalWidth,document.images[0].naturalHeight]",5).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&proof).unwrap(),
        serde_json::json!([true, "rgb(12, 34, 56)", 7, 5])
    );
    for value in [60, 200] {
        if value != 60 {
            eval(&host,id,"const input=document.querySelector('input');input.value=200;input.dispatchEvent(new Event('input'));true",5).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            host.request_dashboard_outputs(id);
            pump();
            host.admit(id, &req);
            if let Some(Some(raw)) = host.take_dashboard_outputs(id) {
                let (snapshot, _) =
                    slate_doc::dashboard_outputs::controller::observation(&raw).unwrap();
                let packet = snapshot.packets.values().next().unwrap();
                if packet["branches"][0]["items"][0]["value"] == value {
                    break;
                }
            }
            assert!(Instant::now() < deadline, "missing typed output {value}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(host.navigate(id, "about:blank"));
    assert!(pump_until(&mut host, id, &req, 12, |h| h
        .current_url(id)
        .as_deref()
        == Some("about:blank")));
    assert!(!host.views[&id]
        .shared
        .borrow()
        .local_resources
        .as_ref()
        .unwrap()
        .is_active());
    host.evict(id);
    drop(host);
    std::fs::remove_dir_all(root).unwrap();
}
