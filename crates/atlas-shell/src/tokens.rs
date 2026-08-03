//! Canonical, live-reloadable design tokens for shared chrome.
//!
//! The checked-in source of truth is `../ui-tokens.toml`. Normal builds embed
//! that file. A build with the `ui-tuner` feature can replace the in-memory
//! values while the app runs and save them back to the TOML file.

use eframe::egui::Color32;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::{OnceLock, RwLock};

const EMBEDDED_TOKENS: &str = include_str!("../ui-tokens.toml");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct UiTokens {
    pub schema_version: u32,
    pub topbar: TopBarTokens,
    pub dock: DockTokens,
    pub home: HomeTokens,
    pub minimap: MinimapTokens,
    pub palette: PaletteTokens,
    pub readouts: ReadoutTokens,
    pub activity_heatmap: ActivityHeatmapTokens,
    pub theme: ThemeTokens,
}

impl Default for UiTokens {
    fn default() -> Self {
        Self {
            schema_version: 4,
            topbar: TopBarTokens::default(),
            dock: DockTokens::default(),
            home: HomeTokens::default(),
            minimap: MinimapTokens::default(),
            palette: PaletteTokens::default(),
            readouts: ReadoutTokens::default(),
            activity_heatmap: ActivityHeatmapTokens::default(),
            theme: ThemeTokens::default(),
        }
    }
}

/// The bottom readout bar that hosts the gear menu, the live counts, and the
/// activity timeline. Several unrelated readouts compete for the same few
/// vertical pixels here, so its padding and text size are dials rather than
/// constants — the balance between them is a judgement made by eye.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadoutTokens {
    /// Padding above the first row and below the last (px).
    pub pad_top: f32,
    pub pad_bottom: f32,
    /// Vertical gap between the metrics row and the timeline below it (px).
    pub row_gap: f32,
    /// Horizontal gap between items in the metrics row (px).
    pub item_gap: f32,
    /// Minimum height of the metrics row; `0` follows the text (px).
    pub row_height: f32,
    /// Point size for every label in the metrics row.
    pub text_size: f32,
    /// Draw the vertical separators flanking the gear menu.
    pub separators: bool,
}

impl Default for ReadoutTokens {
    fn default() -> Self {
        Self {
            pad_top: 3.0,
            pad_bottom: 3.0,
            row_gap: 4.0,
            item_gap: 8.0,
            row_height: 0.0,
            text_size: 12.0,
            separators: true,
        }
    }
}

impl ReadoutTokens {
    pub fn normalize(&mut self) {
        self.pad_top = self.pad_top.clamp(0.0, 24.0);
        self.pad_bottom = self.pad_bottom.clamp(0.0, 24.0);
        self.row_gap = self.row_gap.clamp(0.0, 24.0);
        self.item_gap = self.item_gap.clamp(0.0, 24.0);
        self.row_height = self.row_height.clamp(0.0, 48.0);
        self.text_size = self.text_size.clamp(7.0, 20.0);
    }

    pub fn round_for_storage(&mut self) {
        for value in [
            &mut self.pad_top,
            &mut self.pad_bottom,
            &mut self.row_gap,
            &mut self.item_gap,
            &mut self.row_height,
            &mut self.text_size,
        ] {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
    }
}

/// The unified activity timeline: one time axis shared by the contribution
/// graph and the range handles (see `TOOLBARS.md` and
/// `docs/keymap/specs/activity-timeline.md`).
///
/// Two morph curves carry the graph across zoom levels. `stagger_*` shears the
/// GitHub grid from week columns to each day's true time position; `expand_*`
/// then flattens the weekday staircase into a full-height bucket strip. Both
/// are spans in **days visible**, running from wide to narrow.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivityHeatmapTokens {
    /// Opacity multiplier for time outside the active selection (`1` = none).
    pub out_of_range_opacity: f32,
    /// Saturation multiplier for time outside the active selection.
    pub out_of_range_saturation: f32,
    /// Optional hairline on in-selection cells (`0` = off). Kept for tuning,
    /// not the primary selection cue.
    pub selected_stroke_width: f32,
    pub selected_stroke_opacity: f32,
    /// Day-cell edge and the gap between cells (px).
    pub cell: f32,
    pub cell_gap: f32,
    /// Weekday gutter beside the graph (px).
    pub day_label_width: f32,
    /// Handle rail, and the *minimum* height of the tick scale beneath it — the
    /// scale band grows past this whenever the label font or its gaps need more,
    /// since it is the control's only date readout (px).
    pub rail_height: f32,
    pub scale_height: f32,
    /// Padding around the whole control: above the info row, between that row
    /// and the graph, below the tick scale, and the inset from the right edge
    /// where the axis stops (the left inset is `day_label_width`).
    pub pad_top: f32,
    pub row_gap: f32,
    pub pad_bottom: f32,
    pub pad_right: f32,
    /// Info row: spacing between its items, its text size, the padding inside
    /// its buttons, a minimum row height (`0` follows the text), and the legend
    /// swatch size / gap.
    pub info_gap: f32,
    pub info_text: f32,
    pub info_button_pad_x: f32,
    pub info_button_pad_y: f32,
    pub info_row_height: f32,
    pub legend_cell: f32,
    pub legend_gap: f32,
    /// Point size of the weekday and tick labels.
    pub label_font: f32,
    /// The weekday label's inset from the left edge (px).
    pub weekday_label_dx: f32,
    /// Rail: vertical inset of the selection band inside the rail band, painted
    /// handle radius, square hit area per handle, and the floor width of the
    /// scrub grip so a tight window stays draggable (px).
    pub rail_inset: f32,
    pub handle_radius: f32,
    pub handle_hit: f32,
    pub grip_min_width: f32,
    /// Tick scale: drop from the top of the band to the tick row, tick length,
    /// and the gap from tick to label (px).
    pub scale_top_gap: f32,
    pub scale_tick_len: f32,
    pub scale_label_gap: f32,
    /// Days visible where the weekday columns begin to stagger, and where the
    /// stagger completes (each day then owns its own horizontal slot).
    pub stagger_begin_days: f32,
    pub stagger_full_days: f32,
    /// Days visible where the staircase begins flattening into a full-height
    /// strip, and where a single bucket owns the whole bar height.
    pub expand_begin_days: f32,
    pub expand_full_days: f32,
    /// Narrowest bucket the strip will draw before choosing a coarser grain.
    pub min_bucket_px: f32,
    /// Below this visible span (days), individual file timestamps are drawn as
    /// dashes inside the strip.
    pub file_tick_days: f32,
    pub file_tick_width: f32,
    /// Deepest zoom, in seconds of visible span.
    pub min_view_secs: f32,
    /// Ctrl+wheel zoom factor per notch, and plain-wheel pan as a fraction of
    /// the visible span per notch.
    pub zoom_per_notch: f32,
    pub pan_per_notch: f32,
    /// Wheel-down moves later in time; set true to flip it.
    pub pan_invert: bool,
    /// Seconds for the view to ease toward a new zoom target (`0` = instant).
    pub zoom_ease: f32,
}

impl Default for ActivityHeatmapTokens {
    fn default() -> Self {
        Self {
            out_of_range_opacity: 0.38,
            out_of_range_saturation: 0.45,
            selected_stroke_width: 0.0,
            selected_stroke_opacity: 0.0,
            cell: 8.0,
            cell_gap: 2.0,
            day_label_width: 14.0,
            rail_height: 12.0,
            scale_height: 14.0,
            pad_top: 2.0,
            row_gap: 2.0,
            pad_bottom: 0.0,
            pad_right: 2.0,
            info_gap: 6.0,
            info_text: 10.0,
            info_button_pad_x: 4.0,
            info_button_pad_y: 1.0,
            info_row_height: 0.0,
            legend_cell: 8.0,
            legend_gap: 2.0,
            label_font: 9.0,
            weekday_label_dx: 1.0,
            rail_inset: 2.0,
            handle_radius: 1.8,
            handle_hit: 12.0,
            grip_min_width: 28.0,
            scale_top_gap: 3.0,
            scale_tick_len: 5.0,
            scale_label_gap: 1.0,
            stagger_begin_days: 31.0,
            stagger_full_days: 7.0,
            expand_begin_days: 7.0,
            expand_full_days: 1.0,
            min_bucket_px: 8.0,
            file_tick_days: 1.0,
            file_tick_width: 1.0,
            min_view_secs: 30.0,
            zoom_per_notch: 1.18,
            pan_per_notch: 0.12,
            pan_invert: false,
            zoom_ease: 0.12,
        }
    }
}

impl ActivityHeatmapTokens {
    pub fn normalize(&mut self) {
        self.out_of_range_opacity = self.out_of_range_opacity.clamp(0.05, 1.0);
        self.out_of_range_saturation = self.out_of_range_saturation.clamp(0.0, 1.0);
        self.selected_stroke_width = self.selected_stroke_width.clamp(0.0, 3.0);
        self.selected_stroke_opacity = self.selected_stroke_opacity.clamp(0.0, 1.0);
        self.cell = self.cell.clamp(4.0, 28.0);
        self.cell_gap = self.cell_gap.clamp(0.0, 8.0);
        self.day_label_width = self.day_label_width.clamp(0.0, 48.0);
        self.rail_height = self.rail_height.clamp(10.0, 48.0);
        self.scale_height = self.scale_height.clamp(12.0, 48.0);
        self.pad_top = self.pad_top.clamp(0.0, 24.0);
        self.row_gap = self.row_gap.clamp(0.0, 24.0);
        self.pad_bottom = self.pad_bottom.clamp(0.0, 24.0);
        self.pad_right = self.pad_right.clamp(0.0, 48.0);
        self.info_gap = self.info_gap.clamp(0.0, 24.0);
        self.info_text = self.info_text.clamp(7.0, 18.0);
        self.info_button_pad_x = self.info_button_pad_x.clamp(0.0, 12.0);
        self.info_button_pad_y = self.info_button_pad_y.clamp(0.0, 12.0);
        self.info_row_height = self.info_row_height.clamp(0.0, 48.0);
        self.legend_cell = self.legend_cell.clamp(4.0, 20.0);
        self.legend_gap = self.legend_gap.clamp(0.0, 8.0);
        self.label_font = self.label_font.clamp(6.0, 16.0);
        self.weekday_label_dx = self.weekday_label_dx.clamp(0.0, 16.0);
        // The band must leave a line of rail visible on both sides.
        self.rail_inset = self.rail_inset.clamp(0.0, self.rail_height * 0.4);
        self.handle_radius = self.handle_radius.clamp(0.0, 6.0);
        self.handle_hit = self.handle_hit.clamp(6.0, 32.0);
        self.grip_min_width = self.grip_min_width.clamp(8.0, 80.0);
        self.scale_top_gap = self.scale_top_gap.clamp(0.0, 16.0);
        self.scale_tick_len = self.scale_tick_len.clamp(0.0, 16.0);
        self.scale_label_gap = self.scale_label_gap.clamp(0.0, 16.0);
        // The morph curves must stay ordered wide → narrow, and the expansion
        // cannot start before the stagger has finished or cells would move on
        // two axes at once.
        self.stagger_full_days = self.stagger_full_days.clamp(1.0, 60.0);
        self.stagger_begin_days = self
            .stagger_begin_days
            .clamp(self.stagger_full_days + 1.0, 400.0);
        self.expand_full_days = self.expand_full_days.clamp(0.02, 7.0);
        self.expand_begin_days = self
            .expand_begin_days
            .clamp(self.expand_full_days + 0.1, self.stagger_begin_days);
        self.min_bucket_px = self.min_bucket_px.clamp(1.0, 40.0);
        self.file_tick_days = self.file_tick_days.clamp(0.01, 14.0);
        self.file_tick_width = self.file_tick_width.clamp(0.5, 4.0);
        self.min_view_secs = self.min_view_secs.clamp(5.0, 86_400.0);
        self.zoom_per_notch = self.zoom_per_notch.clamp(1.02, 2.0);
        self.pan_per_notch = self.pan_per_notch.clamp(0.01, 1.0);
        self.zoom_ease = self.zoom_ease.clamp(0.0, 0.6);
    }

    /// Height of the graph block above the rail: seven weekday rows.
    pub fn grid_height(&self) -> f32 {
        7.0 * (self.cell + self.cell_gap) - self.cell_gap
    }

    /// Set the graph's total height by solving for the row size. The seven-row
    /// structure is fixed, so this is the same dial as `cell` viewed from the
    /// other end — and it is the end that matters when the question is how many
    /// vertical pixels the readout bar can spare.
    pub fn set_grid_height(&mut self, height: f32) {
        self.cell = (height - 6.0 * self.cell_gap) / 7.0;
        self.cell = self.cell.clamp(4.0, 28.0);
    }

    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
        for value in [
            &mut self.out_of_range_opacity,
            &mut self.out_of_range_saturation,
            &mut self.selected_stroke_width,
            &mut self.selected_stroke_opacity,
            &mut self.cell,
            &mut self.cell_gap,
            &mut self.day_label_width,
            &mut self.rail_height,
            &mut self.scale_height,
            &mut self.pad_top,
            &mut self.row_gap,
            &mut self.pad_bottom,
            &mut self.pad_right,
            &mut self.info_gap,
            &mut self.info_text,
            &mut self.info_button_pad_x,
            &mut self.info_button_pad_y,
            &mut self.info_row_height,
            &mut self.legend_cell,
            &mut self.legend_gap,
            &mut self.label_font,
            &mut self.weekday_label_dx,
            &mut self.rail_inset,
            &mut self.handle_radius,
            &mut self.handle_hit,
            &mut self.grip_min_width,
            &mut self.scale_top_gap,
            &mut self.scale_tick_len,
            &mut self.scale_label_gap,
            &mut self.stagger_begin_days,
            &mut self.stagger_full_days,
            &mut self.expand_begin_days,
            &mut self.expand_full_days,
            &mut self.min_bucket_px,
            &mut self.file_tick_days,
            &mut self.file_tick_width,
            &mut self.min_view_secs,
            &mut self.zoom_per_notch,
            &mut self.pan_per_notch,
            &mut self.zoom_ease,
        ] {
            round3(value);
        }
    }
}

/// Canvas minimap overlay geometry (see `minimap.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MinimapTokens {
    /// Panel width (px); height follows the content aspect ratio.
    pub width: f32,
    /// Height clamp so extreme aspect ratios stay usable (px).
    pub min_height: f32,
    pub max_height: f32,
    /// Inset from the canvas' lower-right corner (px).
    pub margin: f32,
    /// Superellipse exponent of the panel outline (higher = squarer).
    pub squircle_exponent: f32,
}

impl Default for MinimapTokens {
    fn default() -> Self {
        Self {
            width: 220.0,
            min_height: 90.0,
            max_height: 260.0,
            margin: 14.0,
            squircle_exponent: 6.0,
        }
    }
}

impl MinimapTokens {
    pub fn normalize(&mut self) {
        self.width = self.width.clamp(120.0, 480.0);
        self.min_height = self.min_height.clamp(48.0, 400.0);
        self.max_height = self.max_height.max(self.min_height);
        self.margin = self.margin.clamp(0.0, 80.0);
        self.squircle_exponent = self.squircle_exponent.clamp(2.0, 12.0);
    }

    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
        for value in [
            &mut self.width,
            &mut self.min_height,
            &mut self.max_height,
            &mut self.margin,
            &mut self.squircle_exponent,
        ] {
            round3(value);
        }
    }
}

/// Canvas command-palette popup geometry (see `palette.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PaletteTokens {
    /// Popup width (px).
    pub width: f32,
    pub corner_radius: f32,
    /// Height of each result row (px).
    pub row_height: f32,
    /// Maximum result rows shown at once.
    pub max_rows: usize,
    /// Row label / query text size (pt).
    pub text_size: f32,
}

impl Default for PaletteTokens {
    fn default() -> Self {
        Self {
            width: 300.0,
            corner_radius: 10.0,
            row_height: 26.0,
            max_rows: 8,
            text_size: 12.5,
        }
    }
}

impl PaletteTokens {
    pub fn normalize(&mut self) {
        self.width = self.width.clamp(180.0, 600.0);
        self.corner_radius = self.corner_radius.clamp(0.0, 24.0);
        self.row_height = self.row_height.clamp(18.0, 48.0);
        self.max_rows = self.max_rows.clamp(3, 16);
        self.text_size = self.text_size.clamp(9.0, 18.0);
    }

    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
        for value in [
            &mut self.width,
            &mut self.corner_radius,
            &mut self.row_height,
            &mut self.text_size,
        ] {
            round3(value);
        }
    }
}

/// Cover Flow home shelf geometry and motion (see `home.rs`).
///
/// Layout is sigmoidal: `x(o) = side_step·o + center_bulge·tanh(o/bulge_width)`
/// opens a wide gap around the focused cover and packs side covers tightly.
/// Rotation and depth saturate with their own widths.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HomeTokens {
    /// Cover size as a fraction of the canvas height (covers are square).
    pub cover_frac: f32,
    /// Cover size clamp (px).
    pub cover_min: f32,
    pub cover_max: f32,
    /// Vertical center of the rack as a fraction of the canvas height.
    pub center_y_frac: f32,
    /// Asymptotic side-cover spacing (× cover size).
    pub side_step_frac: f32,
    /// Extra gap pushed outward around the focused cover (× cover size).
    pub center_bulge_frac: f32,
    /// Sigmoid width of the center gap — smaller = sharper falloff.
    pub bulge_width: f32,
    /// Saturating side-cover rotation (degrees; negative flips inward).
    pub angle_max_deg: f32,
    /// Sigmoid width of the rotation ramp — smaller = flips sooner.
    pub angle_width: f32,
    /// Corner fillet radius as a fraction of the card size (rounded corners).
    /// (Field name kept for saved-token compatibility.)
    pub corner_bevel_frac: f32,
    /// Ambient-occlusion halo reach behind each card (px).
    pub ao_size: f32,
    /// Ambient-occlusion strength (0 = off).
    pub ao_strength: f32,
    /// Saturating side-cover depth push-back (px).
    pub depth_max: f32,
    /// Sigmoid width of the depth ramp.
    pub depth_width: f32,
    /// Perspective focal length (px).
    pub focal: f32,
    /// Free-inertia velocity damping (1/s).
    pub friction: f32,
    /// Detent spring stiffness (1/s²).
    pub spring_stiffness: f32,
    /// Detent spring damping (1/s).
    pub spring_damping: f32,
    /// Below this |velocity| inertia hands over to the detent spring.
    pub snap_velocity: f32,
    /// Scroll pixels per album step.
    pub wheel_px_per_album: f32,
}

impl Default for HomeTokens {
    fn default() -> Self {
        Self {
            cover_frac: 0.48,
            cover_min: 180.0,
            cover_max: 340.0,
            center_y_frac: 0.46,
            side_step_frac: 0.16,
            center_bulge_frac: 0.52,
            bulge_width: 0.6,
            angle_max_deg: 62.0,
            angle_width: 0.55,
            corner_bevel_frac: 0.045,
            ao_size: 26.0,
            ao_strength: 0.55,
            depth_max: 90.0,
            depth_width: 0.9,
            focal: 900.0,
            friction: 4.2,
            spring_stiffness: 64.0,
            spring_damping: 15.0,
            snap_velocity: 0.9,
            wheel_px_per_album: 60.0,
        }
    }
}

impl HomeTokens {
    pub fn normalize(&mut self) {
        self.cover_frac = self.cover_frac.clamp(0.15, 0.85);
        self.cover_min = self.cover_min.clamp(60.0, 500.0);
        self.cover_max = self.cover_max.max(self.cover_min);
        self.center_y_frac = self.center_y_frac.clamp(0.2, 0.75);
        self.side_step_frac = self.side_step_frac.clamp(0.02, 0.8);
        self.center_bulge_frac = self.center_bulge_frac.clamp(0.0, 1.5);
        self.bulge_width = self.bulge_width.clamp(0.1, 3.0);
        self.angle_max_deg = self.angle_max_deg.clamp(-85.0, 85.0);
        self.angle_width = self.angle_width.clamp(0.1, 3.0);
        self.corner_bevel_frac = self.corner_bevel_frac.clamp(0.0, 0.2);
        self.ao_size = self.ao_size.clamp(0.0, 120.0);
        self.ao_strength = self.ao_strength.clamp(0.0, 1.0);
        self.depth_max = self.depth_max.clamp(0.0, 600.0);
        self.depth_width = self.depth_width.clamp(0.1, 4.0);
        self.focal = self.focal.clamp(200.0, 4000.0);
        self.friction = self.friction.clamp(0.2, 20.0);
        self.spring_stiffness = self.spring_stiffness.clamp(4.0, 400.0);
        self.spring_damping = self.spring_damping.clamp(1.0, 60.0);
        self.snap_velocity = self.snap_velocity.clamp(0.05, 5.0);
        self.wheel_px_per_album = self.wheel_px_per_album.clamp(10.0, 400.0);
    }

    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
        for value in [
            &mut self.cover_frac,
            &mut self.cover_min,
            &mut self.cover_max,
            &mut self.center_y_frac,
            &mut self.side_step_frac,
            &mut self.center_bulge_frac,
            &mut self.bulge_width,
            &mut self.angle_max_deg,
            &mut self.angle_width,
            &mut self.corner_bevel_frac,
            &mut self.ao_size,
            &mut self.ao_strength,
            &mut self.depth_max,
            &mut self.depth_width,
            &mut self.focal,
            &mut self.friction,
            &mut self.spring_stiffness,
            &mut self.spring_damping,
            &mut self.snap_velocity,
            &mut self.wheel_px_per_album,
        ] {
            round3(value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DockTokens {
    pub icon_size: f32,
    pub icon_gap: f32,
    pub icon_text_size: f32,
    pub label_text_size: f32,
    pub squircle_exponent: f32,
    pub popover_width: f32,
    pub popover_max_height: f32,
    pub popover_gap: f32,
    pub popover_padding: f32,
    pub popover_corner_radius: f32,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_opacity: f32,
    pub close_delay: f32,
    pub left_margin: f32,
    pub bottom_margin: f32,
    /// Gap between stacked open popovers.
    pub stack_gap: f32,
    /// Distance from the icon strip toward the canvas for the partition line.
    pub partition_gap: f32,
    /// How far the partition extends past the icon strip (along its axis).
    pub partition_extend: f32,
    /// Stroke thickness at the partition midpoint.
    pub partition_max_thickness: f32,
    /// Stroke thickness at the partition ends.
    pub partition_min_thickness: f32,
    pub partition_opacity: f32,
    pub tracer_width: f32,
    pub tracer_opacity: f32,
    pub tracer_corner_radius: f32,
    /// Hover hit band around a popover border that reveals the tracer.
    pub tracer_border_hit: f32,
    /// Seconds before a Dashboard hover chip expands to show `description`.
    pub dashboard_describe_delay: f32,
    /// Seconds to fade Dashboard description in the label chip.
    pub describe_fade_duration: f32,
    /// Seconds for hover preview / pinned panel ease-in.
    pub panel_open_duration: f32,
    /// Gap between icon top and label chip / preview anchor.
    pub hover_chip_gap: f32,
    pub light: DockThemeTokens,
    pub dark: DockThemeTokens,
}

impl Default for DockTokens {
    fn default() -> Self {
        Self {
            icon_size: 34.0,
            icon_gap: 10.0,
            icon_text_size: 14.0,
            label_text_size: 11.0,
            squircle_exponent: 4.0,
            popover_width: 260.0,
            popover_max_height: 520.0,
            popover_gap: 10.0,
            popover_padding: 8.0,
            popover_corner_radius: 10.0,
            shadow_offset_x: 0.0,
            shadow_offset_y: 6.0,
            shadow_blur: 20.0,
            shadow_spread: 1.0,
            shadow_opacity: 0.26,
            // Grace window for the pointer to travel icon → preview panel
            // (and across brief canvas excursions) before hovers retire.
            close_delay: 0.45,
            left_margin: 10.0,
            bottom_margin: 14.0,
            stack_gap: 8.0,
            partition_gap: 8.0,
            partition_extend: 48.0,
            partition_max_thickness: 2.4,
            partition_min_thickness: 0.4,
            partition_opacity: 0.45,
            tracer_width: 1.4,
            tracer_opacity: 0.55,
            tracer_corner_radius: 8.0,
            tracer_border_hit: 10.0,
            dashboard_describe_delay: 0.55,
            describe_fade_duration: 0.28,
            panel_open_duration: 0.18,
            hover_chip_gap: 6.0,
            light: DockThemeTokens::light(),
            dark: DockThemeTokens::dark(),
        }
    }
}

impl DockTokens {
    pub fn normalize(&mut self) {
        self.icon_size = self.icon_size.max(18.0);
        self.icon_gap = self.icon_gap.max(0.0);
        self.squircle_exponent = self.squircle_exponent.clamp(2.0, 8.0);
        self.popover_width = self.popover_width.max(140.0);
        self.popover_max_height = self.popover_max_height.max(120.0);
        self.popover_padding = self.popover_padding.max(0.0);
        self.popover_corner_radius = self.popover_corner_radius.max(0.0);
        self.shadow_opacity = self.shadow_opacity.clamp(0.0, 1.0);
        self.close_delay = self.close_delay.clamp(0.0, 2.0);
        self.stack_gap = self.stack_gap.max(0.0);
        self.partition_gap = self.partition_gap.max(0.0);
        self.partition_extend = self.partition_extend.max(0.0);
        self.partition_max_thickness = self.partition_max_thickness.max(0.0);
        self.partition_min_thickness = self
            .partition_min_thickness
            .clamp(0.0, self.partition_max_thickness);
        self.partition_opacity = self.partition_opacity.clamp(0.0, 1.0);
        self.tracer_width = self.tracer_width.max(0.0);
        self.tracer_opacity = self.tracer_opacity.clamp(0.0, 1.0);
        self.tracer_corner_radius = self.tracer_corner_radius.max(0.0);
        self.tracer_border_hit = self.tracer_border_hit.max(2.0);
        self.dashboard_describe_delay = self.dashboard_describe_delay.clamp(0.0, 2.0);
        self.describe_fade_duration = self.describe_fade_duration.clamp(0.05, 1.0);
        self.panel_open_duration = self.panel_open_duration.clamp(0.05, 0.8);
        self.hover_chip_gap = self.hover_chip_gap.clamp(2.0, 24.0);
    }

    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
        for value in [
            &mut self.icon_size,
            &mut self.icon_gap,
            &mut self.icon_text_size,
            &mut self.label_text_size,
            &mut self.squircle_exponent,
            &mut self.popover_width,
            &mut self.popover_max_height,
            &mut self.popover_gap,
            &mut self.popover_padding,
            &mut self.popover_corner_radius,
            &mut self.shadow_offset_x,
            &mut self.shadow_offset_y,
            &mut self.shadow_blur,
            &mut self.shadow_spread,
            &mut self.shadow_opacity,
            &mut self.close_delay,
            &mut self.left_margin,
            &mut self.bottom_margin,
            &mut self.stack_gap,
            &mut self.partition_gap,
            &mut self.partition_extend,
            &mut self.partition_max_thickness,
            &mut self.partition_min_thickness,
            &mut self.partition_opacity,
            &mut self.tracer_width,
            &mut self.tracer_opacity,
            &mut self.tracer_corner_radius,
            &mut self.tracer_border_hit,
            &mut self.dashboard_describe_delay,
            &mut self.describe_fade_duration,
            &mut self.panel_open_duration,
            &mut self.hover_chip_gap,
        ] {
            round3(value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DockThemeTokens {
    pub icon_fill: [u8; 4],
    pub icon_hover: [u8; 4],
    pub icon_active: [u8; 4],
    pub popover_fill: [u8; 4],
    pub border: [u8; 4],
    pub text: [u8; 4],
    pub muted_text: [u8; 4],
}

impl DockThemeTokens {
    fn light() -> Self {
        Self {
            // Hover/active stay close to fill — dock.rs mixes them in lightly.
            icon_fill: [248, 249, 250, 238],
            icon_hover: [240, 242, 245, 242],
            icon_active: [232, 240, 244, 242],
            popover_fill: [248, 249, 250, 248],
            border: [215, 220, 226, 255],
            text: [24, 25, 27, 255],
            muted_text: [112, 116, 122, 255],
        }
    }

    fn dark() -> Self {
        Self {
            icon_fill: [18, 21, 25, 238],
            icon_hover: [24, 28, 33, 242],
            icon_active: [20, 32, 36, 242],
            popover_fill: [18, 20, 22, 248],
            border: [54, 60, 66, 255],
            text: [235, 238, 241, 255],
            muted_text: [145, 150, 156, 255],
        }
    }

    pub fn icon_fill_color(&self) -> Color32 {
        rgba(self.icon_fill)
    }
    pub fn icon_hover_color(&self) -> Color32 {
        rgba(self.icon_hover)
    }
    pub fn icon_active_color(&self) -> Color32 {
        rgba(self.icon_active)
    }
    pub fn popover_fill_color(&self) -> Color32 {
        rgba(self.popover_fill)
    }
    pub fn border_color(&self) -> Color32 {
        rgba(self.border)
    }
    pub fn text_color(&self) -> Color32 {
        rgba(self.text)
    }
    pub fn muted_text_color(&self) -> Color32 {
        rgba(self.muted_text)
    }
}

impl Default for DockThemeTokens {
    fn default() -> Self {
        Self::dark()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TopBarTokens {
    pub height: f32,
    pub tab_top_inset: f32,
    pub tab_top_radius: f32,
    pub tab_shoulder_radius: f32,
    pub tab_horizontal_padding: f32,
    pub tab_close_width: f32,
    pub tab_title_chars: usize,
    pub tab_text_size: f32,
    pub tab_min_width: f32,
    pub tab_max_width: f32,
    pub plus_hit_width: f32,
    pub plus_radius: f32,
    pub plus_text_size: f32,
    pub icon_zone_width: f32,
    pub icon_size: f32,
    pub window_button_width: f32,
    pub glow_outer_width: f32,
    pub glow_outer_opacity: f32,
    pub glow_middle_width: f32,
    pub glow_middle_opacity: f32,
    pub glow_core_width: f32,
    pub glow_core_opacity: f32,
    pub inner_highlight_opacity: f32,
    pub portal: PortalMenuTokens,
    pub light: TopBarThemeTokens,
    pub dark: TopBarThemeTokens,
}

impl Default for TopBarTokens {
    fn default() -> Self {
        Self {
            height: 30.0,
            tab_top_inset: 4.0,
            tab_top_radius: 5.0,
            tab_shoulder_radius: 7.0,
            tab_horizontal_padding: 10.0,
            tab_close_width: 15.0,
            tab_title_chars: 36,
            tab_text_size: 12.0,
            tab_min_width: 108.0,
            tab_max_width: 280.0,
            plus_hit_width: 26.0,
            plus_radius: 8.0,
            plus_text_size: 13.0,
            icon_zone_width: 28.0,
            icon_size: 18.0,
            window_button_width: 40.0,
            glow_outer_width: 4.5,
            glow_outer_opacity: 0.10,
            glow_middle_width: 2.4,
            glow_middle_opacity: 0.28,
            glow_core_width: 1.0,
            glow_core_opacity: 0.88,
            inner_highlight_opacity: 0.12,
            portal: PortalMenuTokens::default(),
            light: TopBarThemeTokens::light(),
            dark: TopBarThemeTokens::dark(),
        }
    }
}

impl TopBarTokens {
    /// Keep hand-edited or live-edited values inside safe rendering bounds.
    pub fn normalize(&mut self) {
        self.height = self.height.max(1.0);
        self.tab_top_inset = self.tab_top_inset.clamp(0.0, (self.height - 1.0).max(0.0));
        self.tab_top_radius = self.tab_top_radius.max(0.5);
        self.tab_shoulder_radius = self.tab_shoulder_radius.max(0.5);
        if self.tab_min_width > self.tab_max_width {
            std::mem::swap(&mut self.tab_min_width, &mut self.tab_max_width);
        }
        for opacity in [
            &mut self.glow_outer_opacity,
            &mut self.glow_middle_opacity,
            &mut self.glow_core_opacity,
            &mut self.inner_highlight_opacity,
            &mut self.light.active_top_mix,
            &mut self.light.divider_strength,
            &mut self.light.accent_white_mix,
            &mut self.dark.active_top_mix,
            &mut self.dark.divider_strength,
            &mut self.dark.accent_white_mix,
            &mut self.portal.shadow_opacity,
        ] {
            *opacity = opacity.clamp(0.0, 1.0);
        }
        self.portal.width = self.portal.width.max(120.0);
        self.portal.submenu_width = self.portal.submenu_width.max(120.0);
        self.portal.row_height = self.portal.row_height.max(18.0);
        self.portal.panel_padding = self.portal.panel_padding.max(0.0);
        self.portal.corner_radius = self.portal.corner_radius.max(0.0);
        self.portal.panel_offset_x = self.portal.panel_offset_x.clamp(-100.0, 100.0);
        self.portal.close_delay = self.portal.close_delay.clamp(0.0, 2.0);
    }

    /// Keep the checked-in TOML readable after slider edits.
    pub fn round_for_storage(&mut self) {
        fn round3(value: &mut f32) {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }

        for value in [
            &mut self.height,
            &mut self.tab_top_inset,
            &mut self.tab_top_radius,
            &mut self.tab_shoulder_radius,
            &mut self.tab_horizontal_padding,
            &mut self.tab_close_width,
            &mut self.tab_text_size,
            &mut self.tab_min_width,
            &mut self.tab_max_width,
            &mut self.plus_hit_width,
            &mut self.plus_radius,
            &mut self.plus_text_size,
            &mut self.icon_zone_width,
            &mut self.icon_size,
            &mut self.window_button_width,
            &mut self.glow_outer_width,
            &mut self.glow_outer_opacity,
            &mut self.glow_middle_width,
            &mut self.glow_middle_opacity,
            &mut self.glow_core_width,
            &mut self.glow_core_opacity,
            &mut self.inner_highlight_opacity,
            &mut self.light.active_top_mix,
            &mut self.light.divider_strength,
            &mut self.light.accent_white_mix,
            &mut self.dark.active_top_mix,
            &mut self.dark.divider_strength,
            &mut self.dark.accent_white_mix,
            &mut self.portal.width,
            &mut self.portal.submenu_width,
            &mut self.portal.row_height,
            &mut self.portal.panel_padding,
            &mut self.portal.corner_radius,
            &mut self.portal.panel_offset_x,
            &mut self.portal.panel_gap,
            &mut self.portal.submenu_gap,
            &mut self.portal.header_text_size,
            &mut self.portal.row_text_size,
            &mut self.portal.shortcut_text_size,
            &mut self.portal.chevron_text_size,
            &mut self.portal.separator_gap,
            &mut self.portal.shadow_offset_x,
            &mut self.portal.shadow_offset_y,
            &mut self.portal.shadow_blur,
            &mut self.portal.shadow_spread,
            &mut self.portal.shadow_opacity,
            &mut self.portal.close_delay,
        ] {
            round3(value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PortalMenuTokens {
    pub width: f32,
    pub submenu_width: f32,
    pub row_height: f32,
    pub panel_padding: f32,
    pub corner_radius: f32,
    pub panel_offset_x: f32,
    pub panel_gap: f32,
    pub submenu_gap: f32,
    pub header_text_size: f32,
    pub row_text_size: f32,
    pub shortcut_text_size: f32,
    pub chevron_text_size: f32,
    pub separator_gap: f32,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
    pub shadow_opacity: f32,
    pub close_delay: f32,
    pub light: PortalMenuThemeTokens,
    pub dark: PortalMenuThemeTokens,
}

impl Default for PortalMenuTokens {
    fn default() -> Self {
        Self {
            width: 220.0,
            submenu_width: 250.0,
            row_height: 30.0,
            panel_padding: 10.0,
            corner_radius: 12.0,
            panel_offset_x: 5.0,
            panel_gap: 5.0,
            submenu_gap: 6.0,
            header_text_size: 12.5,
            row_text_size: 12.0,
            shortcut_text_size: 11.0,
            chevron_text_size: 15.0,
            separator_gap: 7.0,
            shadow_offset_x: 0.0,
            shadow_offset_y: 5.0,
            shadow_blur: 18.0,
            shadow_spread: 1.0,
            shadow_opacity: 0.28,
            close_delay: 0.18,
            light: PortalMenuThemeTokens::light(),
            dark: PortalMenuThemeTokens::dark(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PortalMenuThemeTokens {
    pub fill: [u8; 4],
    pub border: [u8; 4],
    pub hover: [u8; 4],
    pub text: [u8; 4],
    pub muted_text: [u8; 4],
}

impl PortalMenuThemeTokens {
    fn light() -> Self {
        Self {
            fill: [248, 249, 250, 250],
            border: [218, 221, 225, 255],
            hover: [231, 233, 236, 255],
            text: [24, 25, 27, 255],
            muted_text: [112, 116, 122, 255],
        }
    }

    fn dark() -> Self {
        Self {
            fill: [18, 20, 22, 250],
            border: [54, 57, 61, 255],
            hover: [39, 40, 43, 255],
            text: [239, 236, 226, 255],
            muted_text: [169, 166, 158, 255],
        }
    }

    pub fn fill_color(&self) -> Color32 {
        rgba(self.fill)
    }

    pub fn border_color(&self) -> Color32 {
        rgba(self.border)
    }

    pub fn hover_color(&self) -> Color32 {
        rgba(self.hover)
    }

    pub fn text_color(&self) -> Color32 {
        rgba(self.text)
    }

    pub fn muted_text_color(&self) -> Color32 {
        rgba(self.muted_text)
    }
}

impl Default for PortalMenuThemeTokens {
    fn default() -> Self {
        Self::dark()
    }
}

fn rgba(value: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(value[0], value[1], value[2], value[3])
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TopBarThemeTokens {
    pub bar: [u8; 4],
    pub bar_top: [u8; 4],
    pub inactive: [u8; 4],
    pub inactive_hover: [u8; 4],
    pub active_top_mix: f32,
    pub divider_strength: f32,
    pub accent_white_mix: f32,
}

impl TopBarThemeTokens {
    fn light() -> Self {
        Self {
            bar: [0xe4, 0xe7, 0xeb, 0xff],
            bar_top: [0xec, 0xee, 0xf2, 0xff],
            inactive: [0xd2, 0xd6, 0xdc, 0xff],
            inactive_hover: [0xea, 0xec, 0xf0, 0xff],
            active_top_mix: 0.08,
            divider_strength: 0.55,
            accent_white_mix: 0.20,
        }
    }

    fn dark() -> Self {
        Self {
            bar: [0x12, 0x16, 0x1c, 0xff],
            bar_top: [0x19, 0x1f, 0x27, 0xff],
            inactive: [0x19, 0x1e, 0x26, 0xff],
            inactive_hover: [0x21, 0x28, 0x32, 0xff],
            active_top_mix: 0.10,
            divider_strength: 0.65,
            accent_white_mix: 0.42,
        }
    }

    pub fn bar_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.bar[0], self.bar[1], self.bar[2], self.bar[3])
    }

    pub fn bar_top_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(
            self.bar_top[0],
            self.bar_top[1],
            self.bar_top[2],
            self.bar_top[3],
        )
    }

    pub fn inactive_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(
            self.inactive[0],
            self.inactive[1],
            self.inactive[2],
            self.inactive[3],
        )
    }

    pub fn inactive_hover_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(
            self.inactive_hover[0],
            self.inactive_hover[1],
            self.inactive_hover[2],
            self.inactive_hover[3],
        )
    }
}

impl Default for TopBarThemeTokens {
    fn default() -> Self {
        Self::dark()
    }
}

/// A colour token, written as `"#rrggbb"` (or `"#rrggbbaa"`) in TOML.
///
/// Stored unpacked rather than as a `String` because [`current`] clones the
/// whole token set on every frame; a colour token must not allocate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hex(Color32);

impl Hex {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(Color32::from_rgb(r, g, b))
    }

    pub fn color(self) -> Color32 {
        self.0
    }

    /// Parses `#rrggbb` / `#rrggbbaa`, with or without the leading `#`.
    pub fn parse(text: &str) -> Option<Self> {
        let body = text.strip_prefix('#').unwrap_or(text);
        if !body.is_ascii() {
            return None;
        }
        let byte = |at: usize| u8::from_str_radix(&body[at..at + 2], 16).ok();
        match body.len() {
            6 => Some(Self(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))),
            8 => Some(Self(Color32::from_rgba_unmultiplied(
                byte(0)?,
                byte(2)?,
                byte(4)?,
                byte(6)?,
            ))),
            _ => None,
        }
    }
}

impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.0.to_srgba_unmultiplied();
        if a == u8::MAX {
            write!(f, "#{r:02x}{g:02x}{b:02x}")
        } else {
            write!(f, "#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }
}

impl Serialize for Hex {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("expected #rrggbb, got {text:?}")))
    }
}

/// The built-in themes. Named `theme` rather than `palette` because
/// `[palette]` already carries the command-palette popup's geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeTokens {
    pub light: ThemeSlots,
    pub dark: ThemeSlots,
}

impl Default for ThemeTokens {
    fn default() -> Self {
        Self {
            light: ThemeSlots::light(),
            dark: ThemeSlots::dark(),
        }
    }
}

/// One theme's semantic colour slots — the data behind
/// [`crate::theme::Palette`].
///
/// The first fourteen are the slots both apps paint chrome and canvas with.
/// The last five are the surfaces egui fills itself, held here so that
/// `Visuals` can be *derived* from a theme instead of being hand-synchronised
/// beside it; without them a theme could only ever be half-applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSlots {
    /// Paint these slots over egui's dark widget base rather than its light one.
    pub dark_base: bool,
    pub bg: Hex,
    pub grid_dot: Hex,
    pub card: Hex,
    pub card_hover: Hex,
    pub border: Hex,
    pub border_strong: Hex,
    pub ink: Hex,
    pub sub: Hex,
    pub line: Hex,
    pub accent: Hex,
    pub portal: Hex,
    pub thumb_bg: Hex,
    pub select: Hex,
    pub staged: Hex,
    /// `Visuals::panel_fill`.
    pub panel: Hex,
    /// `Visuals::window_fill`.
    pub window: Hex,
    /// `Visuals::extreme_bg_color`.
    pub extreme_bg: Hex,
    /// `Visuals::selection.bg_fill` — egui's own selection highlight, which is
    /// not the canvas selection colour [`ThemeSlots::select`].
    pub select_fill: Hex,
    /// `Visuals::selection.stroke.color`.
    pub select_stroke: Hex,
}

impl ThemeSlots {
    /// Every colour slot name, in declaration order. Kept in step with
    /// [`ThemeSlots::slot_mut`] by `slot_names_all_resolve`.
    pub const SLOTS: &'static [&'static str] = &[
        "bg",
        "grid_dot",
        "card",
        "card_hover",
        "border",
        "border_strong",
        "ink",
        "sub",
        "line",
        "accent",
        "portal",
        "thumb_bg",
        "select",
        "staged",
        "panel",
        "window",
        "extreme_bg",
        "select_fill",
        "select_stroke",
    ];

    pub fn light() -> Self {
        Self {
            dark_base: false,
            bg: Hex::rgb(0xf6, 0xf7, 0xf8),
            grid_dot: Hex::rgb(0xdf, 0xe3, 0xe7),
            card: Hex::rgb(0xff, 0xff, 0xff),
            card_hover: Hex::rgb(0xfb, 0xfc, 0xfd),
            border: Hex::rgb(0xdf, 0xe3, 0xe8),
            border_strong: Hex::rgb(0xc7, 0xcd, 0xd4),
            ink: Hex::rgb(0x1b, 0x1e, 0x22),
            sub: Hex::rgb(0x87, 0x8e, 0x96),
            line: Hex::rgb(0xcb, 0xd1, 0xd8),
            accent: Hex::rgb(0x0f, 0x76, 0x6e),
            portal: Hex::rgb(0x8b, 0x5c, 0xf6),
            thumb_bg: Hex::rgb(0xee, 0xf0, 0xf2),
            select: Hex::rgb(0x1f, 0x6f, 0xb2),
            staged: Hex::rgb(0xc4, 0x84, 0x1d),
            panel: Hex::rgb(0xf8, 0xf9, 0xfb),
            window: Hex::rgb(0xff, 0xff, 0xff),
            extreme_bg: Hex::rgb(0xee, 0xf0, 0xf2),
            select_fill: Hex::rgb(0xd7, 0xe8, 0xff),
            select_stroke: Hex::rgb(0x1f, 0x6f, 0xb2),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark_base: true,
            bg: Hex::rgb(0x0e, 0x10, 0x13),
            grid_dot: Hex::rgb(0x23, 0x27, 0x2d),
            card: Hex::rgb(0x1c, 0x20, 0x26),
            card_hover: Hex::rgb(0x24, 0x29, 0x31),
            border: Hex::rgb(0x33, 0x39, 0x41),
            border_strong: Hex::rgb(0x4a, 0x52, 0x5c),
            ink: Hex::rgb(0xdd, 0xe2, 0xe8),
            sub: Hex::rgb(0x87, 0x8e, 0x96),
            line: Hex::rgb(0x3a, 0x41, 0x4a),
            accent: Hex::rgb(0x2d, 0xd4, 0xbf),
            portal: Hex::rgb(0xa7, 0x8b, 0xfa),
            thumb_bg: Hex::rgb(0x15, 0x18, 0x1c),
            select: Hex::rgb(0x6f, 0xb7, 0xff),
            staged: Hex::rgb(0xe0, 0xa8, 0x3c),
            panel: Hex::rgb(0x14, 0x16, 0x1a),
            window: Hex::rgb(0x1a, 0x1d, 0x23),
            extreme_bg: Hex::rgb(0x0e, 0x10, 0x13),
            select_fill: Hex::rgb(0x2b, 0x5c, 0x8a),
            // egui's own dark default, which `dark_visuals` used to inherit
            // silently while the light theme overrode it.
            select_stroke: Hex::rgb(0xc0, 0xde, 0xff),
        }
    }

    /// The slot named `key`, for loaders that read themes as loose tables.
    pub fn slot_mut(&mut self, key: &str) -> Option<&mut Hex> {
        Some(match key {
            "bg" => &mut self.bg,
            "grid_dot" => &mut self.grid_dot,
            "card" => &mut self.card,
            "card_hover" => &mut self.card_hover,
            "border" => &mut self.border,
            "border_strong" => &mut self.border_strong,
            "ink" => &mut self.ink,
            "sub" => &mut self.sub,
            "line" => &mut self.line,
            "accent" => &mut self.accent,
            "portal" => &mut self.portal,
            "thumb_bg" => &mut self.thumb_bg,
            "select" => &mut self.select,
            "staged" => &mut self.staged,
            "panel" => &mut self.panel,
            "window" => &mut self.window,
            "extreme_bg" => &mut self.extreme_bg,
            "select_fill" => &mut self.select_fill,
            "select_stroke" => &mut self.select_stroke,
            _ => return None,
        })
    }
}

impl Default for ThemeSlots {
    /// Dark, so that a partial user theme falls back to a legible surface
    /// rather than to whatever egui would otherwise leave behind.
    fn default() -> Self {
        Self::dark()
    }
}

fn parse_embedded() -> UiTokens {
    let mut tokens = toml::from_str(EMBEDDED_TOKENS).unwrap_or_else(|error| {
        eprintln!("invalid atlas-shell/ui-tokens.toml ({error}); using factory defaults");
        UiTokens::default()
    });
    tokens.topbar.normalize();
    tokens.dock.normalize();
    tokens.home.normalize();
    tokens.minimap.normalize();
    tokens.palette.normalize();
    tokens.readouts.normalize();
    tokens.activity_heatmap.normalize();
    tokens
}

fn store() -> &'static RwLock<UiTokens> {
    static STORE: OnceLock<RwLock<UiTokens>> = OnceLock::new();
    STORE.get_or_init(|| RwLock::new(parse_embedded()))
}

/// Current tokens, including unsaved changes made by the live tuner.
pub fn current() -> UiTokens {
    store().read().expect("UI token lock poisoned").clone()
}

/// Replace live tokens. Used by the feature-gated UI tuner.
pub fn replace(mut tokens: UiTokens) {
    tokens.topbar.normalize();
    tokens.dock.normalize();
    tokens.home.normalize();
    tokens.minimap.normalize();
    tokens.palette.normalize();
    tokens.readouts.normalize();
    tokens.activity_heatmap.normalize();
    *store().write().expect("UI token lock poisoned") = tokens;
}

/// Values embedded from the checked-in token file when this build was made.
pub fn embedded() -> UiTokens {
    parse_embedded()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_tokens_parse() {
        let tokens: UiTokens = toml::from_str(EMBEDDED_TOKENS).unwrap();
        assert!(tokens.topbar.height > 0.0);
        assert!(tokens.topbar.tab_max_width >= tokens.topbar.tab_min_width);
        assert!(tokens.dock.popover_width > 0.0);
        assert!(tokens.theme.dark.dark_base);
        assert!(!tokens.theme.light.dark_base);
    }

    #[test]
    fn slot_names_all_resolve() {
        let mut slots = ThemeSlots::light();
        for name in ThemeSlots::SLOTS {
            assert!(slots.slot_mut(name).is_some(), "{name} has no slot");
        }
        assert!(slots.slot_mut("dark_base").is_none());
        assert!(slots.slot_mut("not_a_slot").is_none());
    }

    #[test]
    fn hex_round_trips_through_toml() {
        let slots = ThemeSlots::dark();
        let text = toml::to_string(&slots).unwrap();
        assert!(text.contains("bg = \"#0e1013\""), "{text}");
        assert_eq!(toml::from_str::<ThemeSlots>(&text).unwrap(), slots);
    }

    #[test]
    fn hex_accepts_alpha_and_rejects_nonsense() {
        assert_eq!(Hex::parse("#0e1013"), Some(Hex::rgb(0x0e, 0x10, 0x13)));
        assert_eq!(Hex::parse("0e1013"), Some(Hex::rgb(0x0e, 0x10, 0x13)));
        assert_eq!(
            Hex::parse("#0e101380").map(Hex::color),
            Some(Color32::from_rgba_unmultiplied(0x0e, 0x10, 0x13, 0x80))
        );
        // "éé1013" is eight *bytes*, so it reaches the fixed-width slicing.
        for bad in [
            "",
            "#",
            "#fff",
            "#gggggg",
            "#0e1013ff00",
            "rebeccapurple",
            "#éé1013",
        ] {
            assert!(Hex::parse(bad).is_none(), "{bad} parsed");
        }
    }
}
