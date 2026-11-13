//! Entry point and evidence checks for the owned native benchmark window.
use super::*;

pub(super) fn native_canvas_probe() {
    let nodes = std::env::var("SLATE_LATENCY_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(335usize);
    let scene = std::env::var("SLATE_LATENCY_SCENE").unwrap_or_else(|_| "image-board".into());
    assert!(if scene == "live-model" {
        nodes == 1
    } else {
        (20..=5000).contains(&nodes)
    });
    let label = std::env::var("SLATE_LATENCY_LABEL").unwrap_or_else(|_| "baseline".into());
    assert!(label
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
    let output = Arc::new(Mutex::new(None));
    let sink = output.clone();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        vsync: true,
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_decorations(false)
            .with_title("Slate performance owned probe — synthetic"),
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Slate performance owned probe",
        options,
        Box::new(move |cc| Ok(Box::new(NativeBench::new(cc, nodes, sink)))),
    )
    .unwrap();
    let report = output
        .lock()
        .unwrap()
        .take()
        .expect("native benchmark writes its report before closing");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/performance-recovery/20261031/native-{label}-{nodes}.json"
    ));
    std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!(
        "native latency summary={} report={}",
        report["summary"],
        path.display()
    );
    assert!(report["failure"].is_null(), "{report}");
    assert!(report["start_zoom"].as_f64().is_some_and(|zoom| zoom > 0.0));
    for phase in if scene == "live-model" {
        &["pan", "zoom"][..]
    } else {
        &["pan", "zoom", "selection"][..]
    } {
        assert!(
            report["samples"]
                .as_array()
                .unwrap()
                .iter()
                .any(|sample| sample["phase"] == *phase
                    && sample["response_changed"] == true
                    && sample["response_pixels_changed"] == true),
            "{phase} needs independently changed composed canvas pixels"
        );
    }

    let samples = report["samples"].as_array().unwrap();
    assert_eq!(
        samples.len(),
        if scene == "live-model" { 80 } else { 120 },
        "each gesture frame was independently observed"
    );
    assert!(
        samples
            .iter()
            .any(|s| s["phase"] == "pan" && s["response_changed"] == true),
        "normal middle-drag input moved the camera"
    );
    assert!(
        samples
            .iter()
            .any(|s| s["phase"] == "zoom" && s["response_changed"] == true),
        "normal wheel input changed zoom"
    );
    if scene != "live-model" {
        assert!(
            samples
                .iter()
                .any(|s| s["phase"] == "selection" && s["response_changed"] == true),
            "normal clicks changed selection"
        );
    } else {
        assert_eq!(
            report["model_live_after"], true,
            "the actual live 3D texture remains on screen"
        );
    }
}
