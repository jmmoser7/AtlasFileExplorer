//! Native review of the production image surface, stroke and selection geometry.
use super::*;
use vector_ink::kurbo::{RoundedRect, Shape as _};

pub(super) fn review(ui: &mut egui::Ui, theme: Palette) {
    canvas_text::text(
        ui.painter(),
        Pos2::new(35.0, 14.0),
        Align2::LEFT_CENTER,
        "Synthetic R8 notch / R6 marker · production silhouette",
        egui::FontId::proportional(12.0),
        theme.ink,
    );
    for (i, (expanded, width, title)) in [
        (
            false,
            0.0,
            "Collapsed · smooth shoulders · outside selection",
        ),
        (false, 6.0, "Collapsed · authored stroke inside selection"),
        (
            true,
            6.0,
            "Expanded · continuous stroke around image and editor",
        ),
        (true, 0.0, "Expanded · selection clears the whole assembly"),
    ]
    .into_iter()
    .enumerate()
    {
        let origin = Pos2::new(
            35.25 + (i % 2) as f32 * 575.0,
            65.25 + (i / 2) as f32 * 350.0,
        );
        let host = Rect::from_min_size(origin, Vec2::new(550.0, 205.0));
        // Explicit fixture sites, not the app's WireHost/visibility adapter.
        let ports = [
            (host.left(), 30.0, Vec2::X, Color32::from_rgb(26, 185, 196)),
            (host.left(), 72.0, Vec2::X, Color32::from_gray(80)),
            (
                host.left(),
                114.0,
                Vec2::X,
                Color32::from_rgb(255, 241, 164),
            ),
            (
                host.right(),
                30.0,
                -Vec2::X,
                Color32::from_rgb(35, 133, 255),
            ),
            (
                host.right(),
                72.0,
                -Vec2::X,
                Color32::from_rgb(255, 148, 41),
            ),
        ]
        .map(|(x, y, inward, color)| (Pos2::new(x, host.top() + y), inward, color));
        let notches: Vec<_> = ports
            .iter()
            .map(|&(center, inward, _)| chrome::MediaPortNotch {
                center,
                inward,
                radius: 6.0,
            })
            .collect();
        canvas_text::text(
            ui.painter(),
            origin - Vec2::new(0.0, 25.0),
            Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(14.0),
            theme.ink,
        );
        let mut layout =
            chrome::image_presentation_layout(host, 1.0, chrome::ImagePanelKind::Filters);
        layout.blister = chrome::blister_layout(
            host,
            1.0,
            7,
            if expanded {
                chrome::BlisterPresentation::Overlay
            } else {
                chrome::BlisterPresentation::Collapsed
            },
        );
        let path = RoundedRect::new(
            host.left() as f64,
            host.top() as f64,
            host.right() as f64,
            host.bottom() as f64,
            20.0,
        )
        .to_path(0.1);
        let outline: Vec<_> = vector_ink::flatten_contours(&path, 0.1)[0]
            .iter()
            .copied()
            .map(Pos2::from)
            .collect();
        let surface = chrome::media_host_geometry(
            ui.ctx(),
            Id::new(("blister-review-surface", i)),
            &outline,
            &notches,
            layout.blister.cutout,
            1.0,
        );
        let geometry = chrome::image_frame_geometry(
            ui.ctx(),
            Id::new(("blister-review", i)),
            &surface.outline[0],
            expanded.then_some(layout.overall),
            true,
            1.0,
            width,
            1.5,
        );
        let start = chrome::media_host_shape_start(ui.painter());
        ui.painter()
            .rect_filled(host, 20.0, Color32::from_rgb(204, 220, 235));
        // Diagonals make edge coverage and accidental image shading visible.
        let image = ui.painter().with_clip_rect(host.shrink(22.0));
        for j in 0..12 {
            let x = host.left() + j as f32 * 48.0;
            image.line_segment(
                [
                    Pos2::new(x, host.top()),
                    Pos2::new(x - 100.0, host.bottom()),
                ],
                egui::Stroke::new(7.0_f32, Color32::from_rgb(128, 148, 164)),
            );
        }
        chrome::clip_media_host_shapes(
            ui.painter(),
            Id::new(("blister-review-clip", i)),
            start,
            surface,
        );
        // Mark the expected disc size after clipping. No backing masks and no
        // app wire routing, reveal, hit-testing or role policy are reproduced.
        for (center, inward, color) in ports {
            ui.painter().line_segment(
                [center - inward * 20.0, center],
                egui::Stroke::new(2.0_f32, Color32::from_gray(160)),
            );
            ui.painter().circle_filled(center, 6.0, color);
        }
        if expanded {
            chrome::paint_blister_corner_backing(
                ui.painter(),
                Id::new(("review-corners", i)),
                host,
                20.0,
                false,
                theme,
            );
            chrome::paint_image_blister(ui.painter(), &layout, 1.0, theme, 0.0);
            canvas_text::text(
                ui.painter(),
                layout.main.center(),
                Align2::CENTER_CENTER,
                "Editor controls",
                egui::FontId::proportional(14.0),
                theme.ink.gamma_multiply(0.6),
            );
        }
        for (j, icon) in [
            Icon::Fill,
            Icon::Ellipse,
            Icon::Corners,
            Icon::Sliders,
            Icon::Brush,
            Icon::Media,
            Icon::Agent,
        ]
        .into_iter()
        .enumerate()
        {
            layout
                .blister
                .paint_icon(ui.painter(), j, icon, theme.ink, theme);
        }
        if width > 0.0 {
            for ring in &geometry.body {
                ui.painter().add(egui::Shape::closed_line(
                    ring.clone(),
                    egui::Stroke::new(width, Color32::from_rgb(225, 229, 40)),
                ));
            }
        }
        for ring in &geometry.selection {
            ui.painter().add(egui::Shape::closed_line(
                ring.clone(),
                egui::Stroke::new(1.5_f32, Color32::from_rgb(83, 174, 255)),
            ));
        }
    }
}
