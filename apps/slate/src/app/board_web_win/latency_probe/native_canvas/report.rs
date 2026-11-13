//! Measurement summaries and native-process CPU accounting for this probe.
use super::*;

fn quantile(sorted: &[f64], fraction: f64) -> Option<f64> {
    sorted
        .get(((sorted.len().saturating_sub(1)) as f64 * fraction).ceil() as usize)
        .copied()
}

impl NativeBench {
    pub(super) fn finish(&mut self, ctx: &egui::Context, idle_frames: usize) {
        let idle_wall_ms = self
            .idle
            .as_ref()
            .map(|(at, _, _)| at.elapsed().as_secs_f64() * 1000.0);
        let idle_cpu_ms = self
            .idle
            .as_ref()
            .and_then(|(_, _, cpu)| *cpu)
            .zip(process_cpu_ms())
            .map(|(before, after)| after - before);
        let idle_cpu_percent = idle_cpu_ms
            .zip(idle_wall_ms)
            .map(|(cpu, wall)| cpu / wall * 100.0);
        let phases = ["pan", "zoom", "selection"].map(|phase| {
            let selected: Vec<_> = self.samples.iter().filter(|s| s.phase == phase && s.response_changed && s.response_pixels_changed).collect();
            let mut values: Vec<_> = selected.iter().map(|s| s.input_to_capture_ms).collect();
            let mut produced: Vec<_> = selected.iter().filter_map(|s| s.request_to_compositor_ms).collect();
            let mut observer: Vec<_> = selected.iter().filter_map(|s| s.observer_after_compositor_ms).collect();
            let mut available: Vec<_> = selected.iter().filter_map(|s| s.input_to_marker_available_ms).collect();
            let mut roi: Vec<_> = selected.iter().filter_map(|s| s.roi_readback_and_hash_ms).collect();
            values.sort_by(f64::total_cmp);
            produced.sort_by(f64::total_cmp);
            observer.sort_by(f64::total_cmp);
            available.sort_by(f64::total_cmp);
            roi.sort_by(f64::total_cmp);
            let q = |p: f64| quantile(&values, p);
            serde_json::json!({"phase": phase, "n": values.len(), "p50_ms": q(0.5), "p95_ms": q(0.95), "p99_ms": q(0.99), "max_ms": q(1.0),
                "compositor_clock_n": produced.len(), "p95_request_to_compositor_ms": quantile(&produced, 0.95),
                "p95_observer_after_compositor_ms": quantile(&observer, 0.95),
                "marker_available_clock_n": available.len(), "p95_input_to_marker_available_ms": quantile(&available, 0.95),
                "p95_roi_readback_and_hash_ms": quantile(&roi, 0.95)})
        });
        *self.output.lock().unwrap() = Some(serde_json::json!({
            "projection": if std::env::var_os("SLATE_LATENCY_EAGER").is_some() { "equivalent-eager-control" } else { "lazy-active-sources" },
            "mesh_colors": if std::env::var_os("SLATE_MESH_LEGACY_COLORS").is_some() { "legacy" } else { "current" },
            "paint_transform": "current-frame-camera",
            "fit_scale": self.fit_scale,
            "start_zoom": self.start_zoom,
            "baseline_detail": self.baseline_detail,
            "section_profile": {"enabled": self.profile_sections,
                "encoding": "[egui cumulative pass, current pending request phase or null, PREVIOUS outer native-frame work ms or null, ordered [section, elapsed_ms] intervals]",
                "previous_frame_work": "Existing eframe Frame.info().cpu_usage * 1000: PREVIOUS outer frame update+tessellation/renderer work, excluding swap/vsync wait. Not the current pending input, GPU/present time or process CPU utilization. First value may be null; discarded passes may repeat a value. Do not infer previous outer-frame identity from cumulative pass minus one, sum repeated values, or subtract separate percentiles. Getter/retention overhead is after app_ms, inside observed latency; this is optional diagnostic mode only.",
                "boundary": "Existing brush_prof begin after app clock starts, take after app_ms ends. Clock/TLS/lap allocation overhead inside app_ms; take and trace retention outside app_ms but inside observed latency. Intervals are partial and must not be summed as total app time or summed as separate percentiles. Diagnostic mode only; do not compare its latency directly with unprofiled runs.",
                "frames": self.section_laps},
            "model_live_after": self.model.is_some_and(|id| self.h.app.model3d.live_texture_id(id).is_some()),
            "model_triangles": self.model.map(|_| self.model_grid * self.model_grid * 2),
            "synthetic_outputs_per_source": (self.scene == "image-board").then(image_fixture::outputs_per_source),
            "model_gl_counts": self.h.app.model3d.gl_counts(), "scene": self.scene, "nodes": self.h.app.doc().scene.nodes.len(), "pixels_per_point": ctx.pixels_per_point(),
            "screen_points": [ctx.screen_rect().width(), ctx.screen_rect().height()], "vsync": true, "independent_producer_delay_ms": [5,21], "requests_in_flight": 1,
            "paint_binding": "Marker is armed only after the actual scene-paint transform/selection witness differs from the request-before value; live model requires its rendered camera stamp to match the changed current camera. State-only changes never arm early. No extra redraw is requested to satisfy this witness.",
            "clock_reference": "https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.direct3d11captureframe.systemrelativetime",
            "marker_availability_boundary": "input_to_marker_available_ms ends after the matching 1px marker copy/Map/Unmap, before the separate same-frame ROI readback/hash; that ROI is still independently validated before the sample qualifies. These additional QPC intervals do not depend on a valid WGC production timestamp and have their own null/issue ordering checks. Includes capture/poll/1px readback overhead; not earliest-present, physical device, OS mouse translation or scanout. Original observed-readback metric and raw clocks remain unchanged.",
            "clock_boundary": "Additional request_to_compositor_ms uses an independent request QPC sample and SystemRelativeTime from the SAME marker/ROI capture frame. observer_after_compositor_ms ends after ROI readback/hash. Original Instant-based input_to_capture_ms is retained. Failed reads/frequency/range or invalid ordering yield null additional durations plus clock_issue, never a clamped zero. Compositor time is not physical device/OS mouse translation/scanout or earliest-present; capture work still perturbs the measured process/GPU.",
            "idle_seconds": 3, "idle_frames": idle_frames, "idle_wall_ms": idle_wall_ms, "idle_cpu_ms": idle_cpu_ms, "idle_cpu_percent_one_core": idle_cpu_percent, "summary": phases, "samples": self.samples,
            "failure": self.failure,
            "caveat": "A bounded independent worker timestamps an input request before posting it and waking the actual native event loop via Context.request_repaint. Raw_input_hook later admits that request into the normal production egui pan/wheel/selection paths. Queue_to_hook_ms includes UI-thread backlog and native wake scheduling; the latency start is not postponed until a busy UI becomes free. End is an independently observed Windows.Graphics.Capture frame carrying the sequence rendered alongside that response. Summary also requires an independently changed sampled central-canvas pixel signature relative to the preceding observed marked frame. The first marked frame has no signature reference and is excluded. Includes egui, CPU, tessellation, GPU, swap/compositor/capture and observer scheduling/readback; excludes physical-device input and the OS mouse-message translation path (requests use the native repaint/user-event wake), and is not a physical scanout/photon measurement. Capture polling is 2ms and each diagnostic marker/central-canvas GPU readback adds work. Pixel differences witness visible central-scene change in the marked frame, not a full per-target visual equivalence proof. Setup is synthetic using the shared mixed-board fixture owner; no providers or user data."
        }));
        // Close the capture session before the owned window disappears.
        crate::app::board::paint_probe::enable(false);
        self.capture.take();
        self.done = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

pub(super) fn process_cpu_ms() -> Option<f64> {
    use windows::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    }
    .ok()?;
    let ticks = |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
    Some((ticks(kernel) + ticks(user)) as f64 / 10_000.0)
}
