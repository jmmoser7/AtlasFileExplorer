//! Pure pixel planning, independently testable without a GPU.

use eframe::egui;

const MAX_SOURCE_EDGE: u32 = 8192;
const MAX_SOURCE_PIXELS: u64 = 33_177_600;
const MAX_OUTPUT_EDGE: u32 = 4096;
const MAX_OUTPUT_PIXELS: u64 = 8_294_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Size {
    pub(super) source: [u32; 2],
    pub(super) output: [u32; 2],
}

#[derive(Debug)]
pub(super) struct Plan {
    pub(super) size: Size,
    /// Source bottom-left origin, width, height. Resolve keeps these dimensions.
    pub(super) source: [i32; 4],
    /// Output left, bottom, right, top. The GL copy flips top and bottom.
    pub(super) destination: [i32; 4],
}

pub(super) fn plan(
    info: &egui::PaintCallbackInfo,
    maximum: [u32; 2],
    fixed: Option<[u32; 2]>,
) -> Result<Plan, &'static str> {
    let rect = info.viewport;
    let ppp = info.pixels_per_point;
    let screen = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            info.screen_size_px[0] as f32 / ppp,
            info.screen_size_px[1] as f32 / ppp,
        ),
    );
    if !rect.is_finite() || !rect.is_positive() || !ppp.is_finite() || ppp <= 0.0 {
        return Err("The portal has no visible content to capture");
    }
    // Half a physical pixel tolerates ordinary layout rounding, never a crop.
    let visible = info.clip_rect.intersect(screen).expand(0.5 / ppp);
    if !visible.contains_rect(rect) {
        return Err("Show the whole portal on the canvas, or maximize it, before capturing");
    }
    let viewport = info.viewport_in_pixels();
    let source = [viewport.width_px as u32, viewport.height_px as u32];
    if source
        .into_iter()
        .any(|edge| edge < 2 || edge > MAX_SOURCE_EDGE)
        || u64::from(source[0]) * u64::from(source[1]) > MAX_SOURCE_PIXELS
    {
        return Err("The portal is outside the supported capture size");
    }
    let output = match fixed {
        Some(size) => size,
        None => {
            let bounds = maximum.map(|edge| edge.min(MAX_OUTPUT_EDGE));
            let fitted = fit(source, bounds, false);
            let area = fitted[0] as f64 * fitted[1] as f64;
            let scale = (MAX_OUTPUT_PIXELS as f64 / area).sqrt().min(1.0);
            [
                even((fitted[0] as f64 * scale).floor() as u32),
                even((fitted[1] as f64 * scale).floor() as u32),
            ]
        }
    };
    if output
        .into_iter()
        .any(|edge| edge < 2 || edge > MAX_OUTPUT_EDGE)
        || output[0] > maximum[0]
        || output[1] > maximum[1]
        || u64::from(output[0]) * u64::from(output[1]) > MAX_OUTPUT_PIXELS
    {
        return Err("The requested capture dimensions are outside the supported limit");
    }
    let content = fit(source, output, true);
    let left = ((output[0] - content[0]) / 2) as i32;
    let bottom = ((output[1] - content[1]) / 2) as i32;
    Ok(Plan {
        size: Size { source, output },
        source: [
            viewport.left_px,
            viewport.from_bottom_px,
            viewport.width_px,
            viewport.height_px,
        ],
        destination: [
            left,
            bottom,
            left + content[0] as i32,
            bottom + content[1] as i32,
        ],
    })
}

fn even(edge: u32) -> u32 {
    edge & !1
}

fn fit(source: [u32; 2], bounds: [u32; 2], upscale: bool) -> [u32; 2] {
    let scale = (bounds[0] as f64 / source[0] as f64).min(bounds[1] as f64 / source[1] as f64);
    let scale = if upscale { scale } else { scale.min(1.0) };
    [
        (source[0] as f64 * scale).floor().max(1.0) as u32,
        (source[1] as f64 * scale).floor().max(1.0) as u32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(rect: egui::Rect, ppp: f32, screen: [u32; 2]) -> egui::PaintCallbackInfo {
        egui::PaintCallbackInfo {
            viewport: rect,
            clip_rect: egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(20_000.0, 20_000.0)),
            pixels_per_point: ppp,
            screen_size_px: screen,
        }
    }

    #[test]
    fn cropped_msaa_resolve_uses_original_physical_dimensions() {
        let info = info(
            egui::Rect::from_min_size(egui::pos2(100.0, 200.0), egui::vec2(2000.0, 1000.0)),
            1.5,
            [3840, 2160],
        );
        let plan = plan(&info, [1920, 1080], None).unwrap();
        assert_eq!(plan.source, [150, 360, 3000, 1500]);
        assert_eq!(plan.size.source, [3000, 1500]);
        assert_eq!(plan.size.output, [1920, 960]);
    }

    #[test]
    fn resizing_recorded_portal_is_contained_in_fixed_even_output() {
        let info = info(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 800.0)),
            1.0,
            [1920, 1080],
        );
        let plan = plan(&info, [1920, 1080], Some([1920, 1080])).unwrap();
        assert_eq!(plan.size.output, [1920, 1080]);
        assert_eq!(plan.destination, [420, 0, 1500, 1080]);
    }

    #[test]
    fn offscreen_or_partially_clipped_content_is_refused() {
        let mut info = info(
            egui::Rect::from_min_size(egui::pos2(-4.0, 0.0), egui::vec2(600.0, 400.0)),
            1.0,
            [1920, 1080],
        );
        assert!(plan(&info, [1920, 1080], None).is_err());
        info.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0));
        info.clip_rect = info.viewport.shrink(2.0);
        assert!(plan(&info, [1920, 1080], None).is_err());
    }

    #[test]
    fn safe_limits_and_even_first_video_frame_are_enforced() {
        let sample = info(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(801.0, 601.0)),
            1.0,
            [1920, 1080],
        );
        assert_eq!(
            plan(&sample, [1920, 1080], None).unwrap().size.output,
            [800, 600]
        );
        assert!(plan(&sample, [1920, 1080], Some([4000, 2000])).is_err());
        let huge = info(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(8000.0, 8000.0)),
            1.0,
            [8192, 8192],
        );
        assert!(plan(&huge, [1920, 1080], None).is_err());
    }

    #[test]
    fn square_still_fits_the_pixel_budget_instead_of_failing() {
        let sample = info(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4096.0, 4096.0)),
            1.0,
            [4096, 4096],
        );
        let output = plan(&sample, [4096, 4096], None).unwrap().size.output;
        assert_eq!(output, [2880, 2880]);
    }
}
