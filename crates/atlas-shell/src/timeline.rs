//! The unified activity timeline.
//!
//! One time axis carries both the contribution graph and the range handles, so
//! a handle is always directly beneath the buckets it selects. Zoom is anchored
//! at the cursor and drives two morph curves (`atlas_core::timeline::morph`):
//!
//! * **stagger** shears the GitHub 7×N block from week columns into per-day
//!   slots — Sunday left through Saturday right, evenly spaced, weekday still
//!   stepping in y, which is what turns a vertical column into a staircase and
//!   makes a horizontal handle able to address a single day.
//! * **expansion** then flattens the staircase: distant days clip off the
//!   edges while the day under the cursor inflates to the full bar, revealing
//!   an adaptive bucket strip (6 h → hour → minute → second) and finally a
//!   dash per file.
//!
//! The buttons divide by what they address: **left selects time** (click a
//! bucket, or drag to sweep out a window) and **right moves the view** (drag to
//! pan), which is the same division the canvas uses.
//!
//! Feel constants live in [`ActivityHeatmapTokens`]; the pure math lives in
//! `atlas_core::timeline`. Interaction is documented in `TOOLBARS.md` and
//! `docs/keymap/specs/activity-timeline.md`.

use std::cell::RefCell;

use crate::sidebar::SidebarTheme;
use crate::tokens::{self, ActivityHeatmapTokens};
use atlas_core::timeline::{
    bucket_start, grain_for, heat_level, morph, sunday_on_or_before, weekday_row, ActivityIndex,
    TimePicks,
};
use atlas_core::types::{
    date_string, day_index, timeline_range_caption, timeline_tick_label, SECS_PER_DAY,
    SECS_PER_HOUR, SECS_PER_MINUTE,
};
use eframe::egui::{
    self, Color32, FontId, Id, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

/// Inset from each handle center where the scrub grip begins.
const GRIP_INSET: f32 = 8.0;
/// Pointer travel that separates a click (select one bucket) from a drag
/// (sweep out a new window).
const CLICK_SLOP: f32 = 3.0;
/// Scroll pixels per wheel notch.
const NOTCH_PX: f32 = 50.0;
/// A layer takes the pointer once it is legible, not once it dominates — the
/// fine-grained buckets must be aimable as soon as they are visible enough to
/// aim at, rather than after they win the crossfade.
const LAYER_HIT_ALPHA: f32 = 0.25;

thread_local! {
    /// Reused across frames so panning and deep zoom never allocate in the
    /// paint path (Art. II.2).
    static CELLS: RefCell<Vec<Cell>> = const { RefCell::new(Vec::new()) };
    static TICKS: RefCell<Vec<i64>> = const { RefCell::new(Vec::new()) };
}

/// What the timeline did this frame.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineAction {
    /// The selection changed — the caller must re-run its filters. Zoom and
    /// pan alone never set this.
    pub changed: bool,
}

/// Everything the timeline reads.
pub struct ActivityTimeline<'a> {
    pub id: Id,
    pub index: &'a ActivityIndex,
    /// Full selectable extent, day-aligned by the caller.
    pub span_lo: i64,
    pub span_hi: i64,
    pub theme: SidebarTheme,
    pub dark: bool,
    pub muted: Color32,
    /// "created" / "modified" — named in tooltips so the axis is never
    /// ambiguous about which date it is showing.
    pub field_label: &'a str,
    /// What the graph is counting ("canvas" / "filtered canvas" / "selection").
    /// Rides the info row rather than owning a line of its own.
    pub source_label: &'a str,
}

/// Everything the timeline writes. The window is the coarse selection; picks
/// are per-bucket exceptions inside it, seeded from the window the first time
/// one is toggled.
pub struct TimelineSelection<'a> {
    pub range_lo: &'a mut i64,
    pub range_hi: &'a mut i64,
    pub picks: &'a mut TimePicks,
}

/// One painted bucket, in either layer.
#[derive(Clone, Copy, Debug)]
struct Cell {
    rect: Rect,
    /// Half-open time span the cell stands for.
    lo: i64,
    hi: i64,
    count: u32,
    level: u8,
    /// Crossfade weight of the layer this cell belongs to.
    alpha: f32,
}

#[derive(Clone, Copy)]
struct View {
    lo: f64,
    hi: f64,
    /// Wheel gestures move the target; the view eases toward it so the morph
    /// reads as one continuous motion rather than a jump per notch.
    target_lo: f64,
    target_hi: f64,
    span_lo: i64,
    span_hi: i64,
}

/// Sticky drag kind, chosen on press so a gesture survives leaving its
/// original hit zone.
#[derive(Clone, Copy, PartialEq, Default)]
enum Drag {
    #[default]
    Lo,
    Hi,
    /// Left-drag on the band between the handles: slide the window itself.
    Grip,
    /// Right-drag: slide the view along the timeline.
    Pan,
    /// Left-drag across the graph or bare rail: a new window from `anchor_t`,
    /// which works even when both handles are scrolled off screen.
    Sub {
        anchor_t: i64,
        anchor_x: f32,
    },
}

impl ActivityTimeline<'_> {
    /// Total height the control needs, so callers can reserve space before the
    /// data arrives and the readout never changes height mid-scan. `label_row`
    /// is the line height of the tick labels (see [`scale_band`]).
    pub fn height(tokens: &ActivityHeatmapTokens, label_row: f32) -> f32 {
        tokens.grid_height() + tokens.rail_height + scale_band(tokens, label_row)
    }

    pub fn show(&self, ui: &mut Ui, sel: TimelineSelection<'_>) -> TimelineAction {
        let tk = tokens::current().activity_heatmap;
        let mut action = TimelineAction::default();
        if self.span_hi <= self.span_lo {
            ui.label(
                RichText::new(format!("No {} dates yet", self.field_label))
                    .small()
                    .color(self.muted),
            );
            return action;
        }

        let TimelineSelection {
            range_lo,
            range_hi,
            picks,
        } = sel;

        ui.add_space(tk.pad_top);
        self.info_row(ui, &tk, range_lo, range_hi, picks, &mut action);
        ui.add_space(tk.row_gap);

        let width = ui.available_width().max(64.0);
        let label_row = ui
            .ctx()
            .fonts(|f| f.row_height(&FontId::proportional(tk.label_font)));
        let scale_h = scale_band(&tk, label_row);
        let (block, _) = ui.allocate_exact_size(
            Vec2::new(width, Self::height(&tk, label_row)),
            Sense::hover(),
        );
        let axis = Rect::from_min_max(
            Pos2::new(block.left() + tk.day_label_width, block.top()),
            Pos2::new(
                (block.right() - tk.pad_right).max(block.left() + 8.0),
                block.bottom(),
            ),
        );
        let grid = Rect::from_min_size(
            Pos2::new(axis.left(), block.top()),
            Vec2::new(axis.width(), tk.grid_height()),
        );
        let rail = Rect::from_min_size(
            Pos2::new(axis.left(), grid.bottom()),
            Vec2::new(axis.width(), tk.rail_height),
        );

        let mut view = self.read_view(ui, &tk);
        let span_secs = (self.span_hi - self.span_lo).max(1) as f64;

        let resp = ui.interact(block, self.id.with("timeline"), Sense::click_and_drag());
        self.wheel(ui, &tk, &resp, axis, &mut view, span_secs);
        if resp.double_clicked() {
            view.target_lo = self.span_lo as f64;
            view.target_hi = self.span_hi as f64;
        }
        self.ease(ui, &tk, &mut view);
        clamp_view(
            &mut view,
            self.span_lo,
            self.span_hi,
            tk.min_view_secs as f64,
        );

        let visible = (view.hi - view.lo).max(1.0);
        let visible_days = visible / SECS_PER_DAY as f64;
        let week_px = (7 * SECS_PER_DAY) as f64 / visible * axis.width() as f64;
        let day_px = (week_px / 7.0) as f32;
        let footprint = tk.cell + tk.cell_gap;
        // The stagger starts on whichever comes first: the user's day threshold
        // or the point where week columns outgrow a cell — past that the grid
        // would show isolated squares in a sea of gap (D23: key on pixels).
        let stagger = morph(
            visible_days,
            tk.stagger_begin_days as f64,
            tk.stagger_full_days as f64,
        )
        .max(morph(week_px, footprint as f64, 7.0 * footprint as f64));
        let expand = morph(
            visible_days,
            tk.expand_begin_days as f64,
            tk.expand_full_days as f64,
        );

        let grain = grain_for(visible, axis.width(), tk.min_bucket_px).min(SECS_PER_DAY);
        let filter_active =
            *range_lo > self.span_lo || *range_hi < self.span_hi || !picks.is_empty();

        CELLS.with(|store| {
            let mut cells = store.borrow_mut();
            self.build_cells(
                &tk, grid, &view, stagger, expand, week_px, day_px, grain, &mut cells,
            );
            self.paint_graph(
                ui,
                &tk,
                block,
                grid,
                &view,
                &cells,
                expand,
                visible_days,
                *range_lo,
                *range_hi,
                picks,
                filter_active,
            );
            self.pick_gestures(
                ui,
                &tk,
                &resp,
                &cells,
                grain,
                range_lo,
                range_hi,
                picks,
                &mut action,
            );
        });

        self.rail_gestures(
            ui,
            &tk,
            block,
            axis,
            rail,
            grain,
            &mut view,
            range_lo,
            range_hi,
            picks,
            &mut action,
        );
        self.paint_rail(ui, &tk, axis, rail, &view, *range_lo, *range_hi, picks);
        if self.reset_button(ui, block, rail, filter_active) {
            *range_lo = self.span_lo;
            *range_hi = self.span_hi;
            picks.clear();
            action.changed = true;
        }
        // The end labels are centered on ticks at the axis ends, so half of each
        // hangs past the axis: into the weekday gutter on the left and the right
        // inset on the right. Clip to the panel rather than to the block so a
        // date reads whole instead of losing its last digits.
        let scale_top = block.bottom() - scale_h;
        let scale_clip = Rect::from_x_y_ranges(
            ui.clip_rect().x_range(),
            scale_top..=block.bottom().max(scale_top + 1.0),
        );
        draw_scale(
            &ui.painter_at(scale_clip),
            &tk,
            axis,
            scale_top,
            &view,
            tick_step_secs(visible, axis.width()),
            self.theme,
        );

        ui.data_mut(|d| d.insert_temp(self.id.with("view"), view));
        ui.add_space(tk.pad_bottom);
        action
    }

    /// The one line of text this control gets: legend, the reset affordance, and
    /// what is being counted over what window. Everything the timeline has to
    /// say lives here, above the axis — a second caption row underneath was
    /// mostly whitespace, and the axis is the thing worth the vertical space.
    fn info_row(
        &self,
        ui: &mut Ui,
        tk: &ActivityHeatmapTokens,
        range_lo: &mut i64,
        range_hi: &mut i64,
        picks: &mut TimePicks,
        action: &mut TimelineAction,
    ) {
        let cropped = *range_lo > self.span_lo || *range_hi < self.span_hi;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = tk.info_gap;
            ui.spacing_mut().button_padding = Vec2::new(tk.info_button_pad_x, tk.info_button_pad_y);
            if tk.info_row_height > 0.0 {
                ui.set_min_height(tk.info_row_height);
            }
            let font = FontId::proportional(tk.info_text);
            // Legend leads, so the ramp reads next to the button that clears
            // what the ramp is muting.
            let (swatch, gap) = (tk.legend_cell, tk.legend_gap);
            let (lr, _) =
                ui.allocate_exact_size(Vec2::new(5.0 * (swatch + gap), swatch), Sense::hover());
            let painter = ui.painter_at(lr);
            for level in 0..5u8 {
                let x = lr.left() + level as f32 * (swatch + gap);
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x, lr.top()), Vec2::splat(swatch)),
                    2.0,
                    heat_color(level, self.dark),
                );
            }
            let clear = (cropped || !picks.is_empty())
                && ui
                    .button(RichText::new("clear selection").font(font.clone()))
                    .on_hover_text("Reset to the whole timeline (Esc)")
                    .clicked();
            if clear {
                *range_lo = self.span_lo;
                *range_hi = self.span_hi;
                picks.clear();
                action.changed = true;
            }
            let mut text = format!(
                "{} files · {} · {} · {}",
                group_digits(self.index.total() as u64),
                self.source_label,
                self.field_label,
                timeline_range_caption(*range_lo, *range_hi, caption_snap(*range_hi - *range_lo))
            );
            if !picks.is_empty() {
                text.push_str(&format!(" · {} picked spans", picks.len()));
            }
            ui.label(RichText::new(text).font(font).color(self.muted));
        });
    }

    fn read_view(&self, ui: &Ui, tk: &ActivityHeatmapTokens) -> View {
        let fresh = View {
            lo: self.span_lo as f64,
            hi: self.span_hi as f64,
            target_lo: self.span_lo as f64,
            target_hi: self.span_hi as f64,
            span_lo: self.span_lo,
            span_hi: self.span_hi,
        };
        let mut view = ui
            .data(|d| d.get_temp::<View>(self.id.with("view")))
            .unwrap_or(fresh);
        // A span that only grew (a scan landing) keeps the user's view; a
        // different folder starts fitted.
        if view.span_lo > self.span_hi || view.span_hi < self.span_lo {
            view = fresh;
        }
        view.span_lo = self.span_lo;
        view.span_hi = self.span_hi;
        clamp_view(
            &mut view,
            self.span_lo,
            self.span_hi,
            tk.min_view_secs as f64,
        );
        view
    }

    /// Plain wheel pans; Ctrl+wheel zooms around the cursor.
    fn wheel(
        &self,
        ui: &mut Ui,
        tk: &ActivityHeatmapTokens,
        resp: &egui::Response,
        axis: Rect,
        view: &mut View,
        span_secs: f64,
    ) {
        if !resp.hovered() {
            return;
        }
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        // Consume the scroll so the enclosing panel does not also move.
        let (scroll, zoom) = ui.input_mut(|i| {
            let s = i.smooth_scroll_delta.y + i.raw_scroll_delta.y;
            i.smooth_scroll_delta = Vec2::ZERO;
            i.raw_scroll_delta = Vec2::ZERO;
            (s, i.zoom_delta())
        });
        let width = (view.target_hi - view.target_lo).max(1.0);
        let notches = scroll / NOTCH_PX;

        if ctrl {
            // egui folds Ctrl+wheel into `zoom_delta` on some platforms and
            // leaves it in the scroll delta on others; honour either.
            let factor = if (zoom - 1.0).abs() > 0.001 {
                1.0 / zoom as f64
            } else if notches.abs() > 0.0 {
                (tk.zoom_per_notch as f64).powf(-notches as f64)
            } else {
                return;
            };
            let anchor = time_at_x(
                ui.input(|i| i.pointer.hover_pos())
                    .unwrap_or(axis.center())
                    .x,
                axis,
                view.target_lo,
                view.target_hi,
            );
            let new_w = (width * factor).clamp(tk.min_view_secs as f64, span_secs);
            let rel = ((anchor - view.target_lo) / width).clamp(0.0, 1.0);
            view.target_lo = anchor - rel * new_w;
            view.target_hi = view.target_lo + new_w;
        } else if notches.abs() > 0.0 {
            let dir = if tk.pan_invert { 1.0 } else { -1.0 };
            let dt = dir * notches as f64 * width * tk.pan_per_notch as f64;
            view.target_lo += dt;
            view.target_hi += dt;
        } else {
            return;
        }
        clamp_target(view, self.span_lo, self.span_hi, tk.min_view_secs as f64);
        ui.ctx().request_repaint();
    }

    fn ease(&self, ui: &Ui, tk: &ActivityHeatmapTokens, view: &mut View) {
        if tk.zoom_ease <= 0.0 {
            view.lo = view.target_lo;
            view.hi = view.target_hi;
            return;
        }
        let dt = ui.input(|i| i.stable_dt).clamp(0.0, 0.1) as f64;
        let k = 1.0 - (-dt / tk.zoom_ease as f64).exp();
        view.lo += (view.target_lo - view.lo) * k;
        view.hi += (view.target_hi - view.hi) * k;
        let settled =
            (view.target_lo - view.lo).abs() < 1.0 && (view.target_hi - view.hi).abs() < 1.0;
        if settled {
            view.lo = view.target_lo;
            view.hi = view.target_hi;
        } else {
            ui.ctx().request_repaint();
        }
    }

    /// Geometry pass: both layers, positioned by time so the axis is shared.
    #[allow(clippy::too_many_arguments)]
    fn build_cells(
        &self,
        tk: &ActivityHeatmapTokens,
        grid: Rect,
        view: &View,
        stagger: f32,
        expand: f32,
        week_px: f64,
        day_px: f32,
        grain: i64,
        out: &mut Vec<Cell>,
    ) {
        out.clear();
        let gap = tk.cell_gap;
        let cell_h = bar_height(tk, grid, expand);

        // --- weekday grid, fading out as the strip takes over ---
        let grid_alpha = 1.0 - expand;
        if grid_alpha > 0.01 {
            let day_max = self.index.max_bucket(SECS_PER_DAY);
            let week_max = self.index.max_bucket(7 * SECS_PER_DAY);
            let first = sunday_on_or_before(day_index(view.lo.floor() as i64));
            let last = day_index(view.hi.ceil() as i64);
            let mut week = first;
            while week <= last {
                let week_lo = week * SECS_PER_DAY;
                let week_x = time_to_x(week_lo as f64, grid, view.lo, view.hi);
                if week_px < 2.0 {
                    // Below two pixels a column cannot show seven rows
                    // honestly, so the week is drawn as one aggregate bar.
                    let count = self.index.count_in(week_lo, week_lo + 7 * SECS_PER_DAY);
                    out.push(Cell {
                        rect: Rect::from_min_size(
                            Pos2::new(week_x, grid.top()),
                            Vec2::new((week_px as f32).max(1.0), grid.height()),
                        ),
                        lo: week_lo,
                        hi: week_lo + 7 * SECS_PER_DAY,
                        count,
                        level: heat_level(count, week_max),
                        alpha: grid_alpha,
                    });
                } else {
                    for row in 0..7i64 {
                        let day = week + row;
                        let day_lo = day * SECS_PER_DAY;
                        let day_hi = day_lo + SECS_PER_DAY;
                        let day_x = time_to_x(day_lo as f64, grid, view.lo, view.hi);
                        let x = lerp(week_x, day_x, stagger);
                        let w = cell_width(week_px as f32, day_px, gap, stagger);
                        if x + w < grid.left() || x > grid.right() {
                            continue;
                        }
                        let y = bar_top(tk, grid, row as usize, expand);
                        let count = self.index.count_in(day_lo, day_hi);
                        out.push(Cell {
                            rect: Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, cell_h)),
                            lo: day_lo,
                            hi: day_hi,
                            count,
                            level: heat_level(count, day_max),
                            alpha: grid_alpha,
                        });
                    }
                }
                week += 7;
            }
        }

        // --- adaptive strip, fading in ---
        if expand > 0.01 && grain < SECS_PER_DAY {
            let max = self.index.max_bucket(grain);
            let mut t = bucket_start(view.lo.floor() as i64, grain);
            let end = view.hi.ceil() as i64;
            while t <= end {
                let x0 = time_to_x(t as f64, grid, view.lo, view.hi);
                let x1 = time_to_x((t + grain) as f64, grid, view.lo, view.hi);
                let count = self.index.count_in(t, t + grain);
                // Concentric with the day it is carving up — same band, same
                // height — so the finer buckets resolve out of that day's cell
                // instead of fading in somewhere else and sliding into place.
                let y = bar_top(tk, grid, weekday_row(day_index(t)), expand);
                out.push(Cell {
                    rect: Rect::from_min_max(
                        Pos2::new(x0, y),
                        Pos2::new((x1 - 1.0).max(x0 + 1.0), y + cell_h),
                    ),
                    lo: t,
                    hi: t + grain,
                    count,
                    level: heat_level(count, max),
                    alpha: expand,
                });
                t += grain;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_graph(
        &self,
        ui: &Ui,
        tk: &ActivityHeatmapTokens,
        block: Rect,
        grid: Rect,
        view: &View,
        cells: &[Cell],
        expand: f32,
        visible_days: f64,
        range_lo: i64,
        range_hi: i64,
        picks: &TimePicks,
        filter_active: bool,
    ) {
        let painter = ui.painter_at(block);
        let label_fade = 1.0 - expand;

        // Only the weekday gutter labels the graph itself. There is no month
        // strip above it: it named the same dates the tick scale already names,
        // and the two disagreed by design — months snapped to week columns while
        // the scale snaps to its own step — so the axis had two captions that
        // never lined up. The scale under the rail is the single source of truth.
        if label_fade > 0.01 {
            let faded = fade(self.muted, label_fade);
            for &(row, label) in &[(1usize, "Mon"), (3, "Wed"), (5, "Fri")] {
                let y = grid.top() + row as f32 * (tk.cell + tk.cell_gap) + tk.cell * 0.5;
                painter.text(
                    Pos2::new(block.left() + tk.weekday_label_dx, y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(tk.label_font),
                    faded,
                );
            }
        }

        for cell in cells {
            let selected =
                !filter_active || in_selection(cell.lo, cell.hi, range_lo, range_hi, picks);
            let base = heat_color(cell.level, self.dark);
            let fill = if selected {
                base
            } else {
                mute_heat(base, tk.out_of_range_opacity, tk.out_of_range_saturation)
            };
            painter.rect_filled(cell.rect, 2.0, fade(fill, cell.alpha));
            if selected && filter_active && tk.selected_stroke_width > 0.0 {
                let alpha = tk.selected_stroke_opacity.clamp(0.0, 1.0) * cell.alpha;
                if alpha > 0.0 {
                    painter.rect_stroke(
                        cell.rect.expand(0.5),
                        2.0,
                        Stroke::new(tk.selected_stroke_width, fade(self.theme.ink, alpha)),
                        StrokeKind::Outside,
                    );
                }
            }
        }

        // One dash per file, once a bucket is short enough that individual
        // timestamps are the interesting resolution.
        let tick_alpha = morph(
            visible_days,
            (tk.file_tick_days * 3.0) as f64,
            tk.file_tick_days as f64,
        );
        if tick_alpha > 0.01 {
            TICKS.with(|store| {
                let mut ticks = store.borrow_mut();
                self.index
                    .stamps_in(view.lo.floor() as i64, view.hi.ceil() as i64, &mut ticks);
                if ticks.len() > 4_000 {
                    return;
                }
                let color = fade(
                    if self.dark {
                        Color32::from_rgb(0xd8, 0xff, 0xe4)
                    } else {
                        Color32::from_rgb(0x0b, 0x3a, 0x1c)
                    },
                    tick_alpha,
                );
                let dim = fade(color, tk.out_of_range_opacity);
                let h = bar_height(tk, grid, expand);
                for &t in ticks.iter() {
                    let x = time_to_x(t as f64, grid, view.lo, view.hi);
                    // Inside the band of its own day, like the buckets around it.
                    let top = bar_top(tk, grid, weekday_row(day_index(t)), expand) + 2.0;
                    let lit = !filter_active || in_selection(t, t + 1, range_lo, range_hi, picks);
                    painter.line_segment(
                        [
                            Pos2::new(x, top),
                            Pos2::new(x, (top + h - 4.0).max(top + 1.0)),
                        ],
                        Stroke::new(tk.file_tick_width, if lit { color } else { dim }),
                    );
                }
            });
        }
    }

    /// Clicks on the graph: plain sets the window to one bucket, Shift extends
    /// from the anchor, Ctrl toggles a bucket inside the window.
    #[allow(clippy::too_many_arguments)]
    fn pick_gestures(
        &self,
        ui: &mut Ui,
        tk: &ActivityHeatmapTokens,
        resp: &egui::Response,
        cells: &[Cell],
        grain: i64,
        range_lo: &mut i64,
        range_hi: &mut i64,
        picks: &mut TimePicks,
        action: &mut TimelineAction,
    ) {
        let Some(pointer) = ui.input(|i| i.pointer.latest_pos()) else {
            return;
        };
        let Some(cell) = hit_cell(cells, pointer) else {
            return;
        };

        if resp.hovered() {
            let ring = tk.selected_stroke_width.max(1.0);
            ui.painter_at(resp.rect).rect_stroke(
                cell.rect.expand(0.5),
                2.0,
                Stroke::new(ring, fade(self.theme.ink, 0.55 * cell.alpha)),
                StrokeKind::Outside,
            );
            let when = if cell.hi - cell.lo >= SECS_PER_DAY {
                date_string(cell.lo)
            } else {
                format!(
                    "{} {}–{}",
                    date_string(cell.lo),
                    timeline_tick_label(cell.lo, grain),
                    timeline_tick_label(cell.hi, grain)
                )
            };
            let files = match cell.count {
                1 => "1 file".to_string(),
                n => format!("{} files", group_digits(n as u64)),
            };
            egui::show_tooltip_at_pointer(
                ui.ctx(),
                resp.layer_id,
                self.id.with("tip"),
                |ui: &mut Ui| {
                    ui.label(format!("{files} · {when} ({})", self.field_label));
                    ui.label(
                        RichText::new(
                            "click select · drag window · shift range · ctrl toggle\n\
                             right-drag or wheel pan · ctrl+wheel zoom",
                        )
                        .small()
                        .weak(),
                    );
                },
            );
        }

        if !resp.clicked() {
            return;
        }
        let (shift, ctrl) =
            ui.input(|i| (i.modifiers.shift, i.modifiers.ctrl || i.modifiers.command));
        let anchor_id = self.id.with("anchor");
        if ctrl {
            // A plain drag already handled defining the window; a Ctrl-click
            // punches or restores this bucket. Seeding from the window is what
            // makes the first Ctrl-click read as "deselect this one".
            if picks.is_empty() {
                picks.insert(*range_lo, range_hi.saturating_add(1));
            }
            picks.toggle(cell.lo, cell.hi);
            if picks.is_empty() {
                *range_lo = self.span_lo;
                *range_hi = self.span_hi;
            }
        } else if shift {
            let anchor = ui
                .ctx()
                .data(|d| d.get_temp::<i64>(anchor_id))
                .unwrap_or(cell.lo);
            let (a, b) = if anchor <= cell.lo {
                (anchor, cell.hi)
            } else {
                (cell.lo, anchor + (cell.hi - cell.lo))
            };
            picks.clear();
            *range_lo = a.clamp(self.span_lo, self.span_hi);
            *range_hi = (b - 1).clamp(self.span_lo, self.span_hi);
        } else {
            picks.clear();
            ui.ctx().data_mut(|d| d.insert_temp(anchor_id, cell.lo));
            *range_lo = cell.lo.clamp(self.span_lo, self.span_hi);
            *range_hi = (cell.hi - 1).clamp(self.span_lo, self.span_hi);
        }
        action.changed = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn rail_gestures(
        &self,
        ui: &mut Ui,
        tk: &ActivityHeatmapTokens,
        block: Rect,
        axis: Rect,
        rail: Rect,
        grain: i64,
        view: &mut View,
        range_lo: &mut i64,
        range_hi: &mut i64,
        picks: &mut TimePicks,
        action: &mut TimelineAction,
    ) {
        let drag_id = self.id.with("drag");
        let primary_down = ui.input(|i| i.pointer.primary_down());
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        let secondary_down = ui.input(|i| i.pointer.secondary_down());
        let secondary_pressed = ui.input(|i| i.pointer.secondary_pressed());
        let pointer = ui.input(|i| i.pointer.latest_pos());

        let lo = (*range_lo).min(*range_hi);
        let hi = (*range_lo).max(*range_hi);
        let gx0 = time_to_x(lo as f64, axis, view.lo, view.hi);
        let gx1 = time_to_x(hi as f64, axis, view.lo, view.hi);
        let mid = (rail.top() + rail.bottom()) * 0.5;
        let handle_rects = [gx0, gx1].map(|x| {
            Rect::from_center_size(
                Pos2::new(x, mid),
                Vec2::new(tk.handle_hit, tk.handle_hit.max(rail.height())),
            )
        });
        let grip = {
            let wide = gx1 - gx0 > GRIP_INSET * 2.0 + 4.0;
            let (l, r) = if wide {
                (gx0 + GRIP_INSET, gx1 - GRIP_INSET)
            } else {
                let c = (gx0 + gx1) * 0.5;
                (c - tk.grip_min_width * 0.5, c + tk.grip_min_width * 0.5)
            };
            Rect::from_min_max(
                Pos2::new(l.max(axis.left()), rail.top() - 2.0),
                Pos2::new(r.min(axis.right()), rail.bottom() + 2.0),
            )
        };

        // The window can only be slid when it is actually cropped; while it
        // covers the whole span the grip has nothing to move, so the drag is
        // better spent defining a window.
        let movable = hi > lo && (lo > self.span_lo || hi < self.span_hi);

        let mut drag = ui.data(|d| d.get_temp::<Drag>(drag_id));
        // A gesture ends when the button that began it is released, not when any
        // button is: the other one may go down mid-drag.
        let held = match drag {
            Some(Drag::Pan) => secondary_down,
            Some(_) => primary_down,
            None => false,
        };
        if !held {
            drag = None;
        }
        if drag.is_none() {
            let start = pointer.filter(|p| block.contains(*p));
            drag = if secondary_pressed {
                start.map(|_| Drag::Pan)
            } else if pressed {
                start.map(|p| match press_target(p, &handle_rects, grip, movable) {
                    PressTarget::Lo => Drag::Lo,
                    PressTarget::Hi => Drag::Hi,
                    PressTarget::Grip => Drag::Grip,
                    PressTarget::Window => Drag::Sub {
                        anchor_t: time_at_x(p.x, axis, view.lo, view.hi).round() as i64,
                        anchor_x: p.x,
                    },
                })
            } else {
                None
            };
        }
        ui.data_mut(|d| match drag {
            Some(kind) => d.insert_temp(drag_id, kind),
            None => {
                d.remove_temp::<Drag>(drag_id);
            }
        });

        let dragging = primary_down;
        match drag {
            Some(Drag::Lo) | Some(Drag::Hi) => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                if let (true, Some(p)) = (dragging, pointer) {
                    let t = time_at_x(p.x, axis, view.lo, view.hi).round() as i64;
                    // Snap to the grain on screen: the low edge takes the
                    // bucket start, the high edge its last second, so dragging
                    // over a day includes that day.
                    if drag == Some(Drag::Lo) {
                        *range_lo = bucket_start(t, grain);
                    } else {
                        *range_hi = bucket_start(t, grain) + grain - 1;
                    }
                    if *range_lo > *range_hi {
                        std::mem::swap(range_lo, range_hi);
                    }
                    picks.clear();
                    action.changed = true;
                }
            }
            Some(Drag::Grip) => {
                ui.ctx().set_cursor_icon(if dragging {
                    egui::CursorIcon::Grabbing
                } else {
                    egui::CursorIcon::Grab
                });
                if dragging {
                    let dx = ui.input(|i| i.pointer.delta().x);
                    if dx.abs() > 0.0 {
                        let dt =
                            (dx as f64 / axis.width() as f64 * (view.hi - view.lo)).round() as i64;
                        let window = hi - lo;
                        let mut new_lo = lo + dt;
                        new_lo =
                            new_lo.clamp(self.span_lo, (self.span_hi - window).max(self.span_lo));
                        *range_lo = new_lo;
                        *range_hi = (new_lo + window).min(self.span_hi);
                        picks.clear();
                        action.changed = true;
                    }
                }
            }
            Some(Drag::Pan) => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                if secondary_down {
                    let dx = ui.input(|i| i.pointer.delta().x);
                    if dx.abs() > 0.0 {
                        let dt = dx as f64 / axis.width() as f64 * (view.hi - view.lo);
                        view.target_lo -= dt;
                        view.target_hi -= dt;
                        view.lo -= dt;
                        view.hi -= dt;
                        clamp_target(view, self.span_lo, self.span_hi, tk.min_view_secs as f64);
                    }
                }
            }
            Some(Drag::Sub { anchor_t, anchor_x }) => {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                if let Some(p) = pointer {
                    let now = time_at_x(p.x, axis, view.lo, view.hi).round() as i64;
                    if (p.x - anchor_x).abs() > CLICK_SLOP {
                        let (a, b) = (anchor_t.min(now), anchor_t.max(now));
                        *range_lo = bucket_start(a, grain).clamp(self.span_lo, self.span_hi);
                        *range_hi =
                            (bucket_start(b, grain) + grain - 1).clamp(self.span_lo, self.span_hi);
                        picks.clear();
                        action.changed = true;
                        // Preview band while the drag is live, over the graph as
                        // well as the rail: the graph is what the eye is on, and
                        // the rail is repainted after this.
                        ui.painter_at(block).rect_filled(
                            Rect::from_x_y_ranges(
                                anchor_x.min(p.x)..=anchor_x.max(p.x),
                                block.top()..=rail.bottom(),
                            ),
                            0.0,
                            fade(self.theme.ink, 0.12),
                        );
                    }
                }
            }
            None => {
                if let Some(p) = pointer {
                    if handle_rects.iter().any(|r| r.contains(p)) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    } else if grip.contains(p) && movable {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_rail(
        &self,
        ui: &Ui,
        tk: &ActivityHeatmapTokens,
        axis: Rect,
        rail: Rect,
        view: &View,
        range_lo: i64,
        range_hi: i64,
        picks: &TimePicks,
    ) {
        let painter = ui.painter_at(rail.expand2(Vec2::new(tk.day_label_width, 2.0)));
        let mid = (rail.top() + rail.bottom()) * 0.5;
        let x0 = time_to_x(range_lo as f64, axis, view.lo, view.hi);
        let x1 = time_to_x(range_hi as f64, axis, view.lo, view.hi);
        let band_y = rail.top() + tk.rail_inset..=rail.bottom() - tk.rail_inset;
        let band =
            Rect::from_x_y_ranges(x0.max(axis.left())..=x1.min(axis.right()), band_y.clone());
        if band.width() > 0.0 {
            painter.rect_filled(band, 0.0, fade(self.theme.ink, 0.16));
        }
        // Picked spans read darker than the window they sit in, so a punched
        // hole is visible on the rail as well as in the graph.
        for (lo, hi) in picks.intervals() {
            let a = time_to_x(lo as f64, axis, view.lo, view.hi).max(axis.left());
            let b = time_to_x(hi as f64, axis, view.lo, view.hi).min(axis.right());
            if b > a {
                painter.rect_filled(
                    Rect::from_x_y_ranges(a..=b, band_y.clone()),
                    0.0,
                    fade(self.theme.ink, 0.3),
                );
            }
        }
        painter.hline(
            axis.x_range(),
            mid,
            Stroke::new(1.5_f32, self.theme.border.gamma_multiply(0.9)),
        );
        for x in [x0, x1] {
            painter.circle_filled(Pos2::new(x, mid), tk.handle_radius, self.theme.sub);
        }
    }

    /// Small reset target in the gutter, left of the rail — deliberately on the
    /// axis-free side so it cannot shift the shared mapping.
    fn reset_button(&self, ui: &mut Ui, block: Rect, rail: Rect, active: bool) -> bool {
        let size = (rail.height() - 2.0).clamp(9.0, 14.0);
        let rect = Rect::from_center_size(
            Pos2::new(
                block.left() + size * 0.5 + 1.0,
                (rail.top() + rail.bottom()) * 0.5,
            ),
            Vec2::splat(size),
        );
        let resp = ui
            .interact(rect, self.id.with("reset"), Sense::click())
            .on_hover_text("Reset selection to the whole timeline (Esc)");
        let painter = ui.painter_at(block);
        let color = if !active {
            fade(self.muted, 0.5)
        } else if resp.hovered() {
            self.theme.ink
        } else {
            self.theme.sub
        };
        painter.circle_stroke(rect.center(), size * 0.34, Stroke::new(1.2_f32, color));
        // Break the ring and add a tick: a reset glyph legible at 12 px.
        painter.rect_filled(
            Rect::from_center_size(
                Pos2::new(rect.center().x + size * 0.30, rect.center().y - size * 0.22),
                Vec2::splat(size * 0.30),
            ),
            0.0,
            self.theme.card,
        );
        painter.line_segment(
            [
                Pos2::new(rect.center().x + size * 0.16, rect.center().y - size * 0.34),
                Pos2::new(rect.center().x + size * 0.40, rect.center().y - size * 0.10),
            ],
            Stroke::new(1.2_f32, color),
        );
        active && resp.clicked()
    }
}

/// A bucket is lit when it falls in the window and, once picks exist, in a
/// picked span. Picks are exceptions inside the window, never a second window.
fn in_selection(lo: i64, hi: i64, range_lo: i64, range_hi: i64, picks: &TimePicks) -> bool {
    if hi <= range_lo || lo > range_hi {
        return false;
    }
    if picks.is_empty() {
        return true;
    }
    picks.overlaps(lo, hi)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Height of the tick scale band. `scale_height` is a floor, not the value: the
/// band always grows to hold the tick row plus a whole line of label text, so
/// raising the label size or either gap can never clip the dates — they are the
/// control's only date readout now.
fn scale_band(tk: &ActivityHeatmapTokens, label_row: f32) -> f32 {
    (tk.scale_top_gap + tk.scale_tick_len + tk.scale_label_gap + label_row).max(tk.scale_height)
}

/// What a left press starts.
#[derive(Clone, Copy, PartialEq, Debug)]
enum PressTarget {
    Lo,
    Hi,
    Grip,
    Window,
}

/// Route a left press. Order is handle → grip → window: the grip spans the band
/// between the handles and therefore overlaps them, so the handles have to win or
/// a cropped window could not be re-edged.
///
/// `movable` is false while the window covers the whole span, where sliding it is
/// a no-op — that drag is better spent sweeping out a new window, which is also
/// what makes the rail useful before anything has been selected.
fn press_target(p: Pos2, handles: &[Rect; 2], grip: Rect, movable: bool) -> PressTarget {
    if handles[0].contains(p) {
        PressTarget::Lo
    } else if handles[1].contains(p) {
        PressTarget::Hi
    } else if movable && grip.contains(p) {
        PressTarget::Grip
    } else {
        PressTarget::Window
    }
}

/// The bucket under the pointer, searched frontmost first so the finer layer
/// wins as soon as it is legible.
///
/// Empty intervals are inert. A bucket that received no files is real structure
/// worth painting, but it is not worth reporting: at depth most of a day is
/// empty, so hovering them would answer "nothing here" over and over and turn a
/// slow pan into a stutter of tooltips. Skipping them also lets the pointer fall
/// through to the coarser bucket underneath, which does have something to say.
fn hit_cell(cells: &[Cell], p: Pos2) -> Option<&Cell> {
    cells
        .iter()
        .rev()
        .filter(|c| c.alpha >= LAYER_HIT_ALPHA && c.count > 0)
        .find(|c| c.rect.expand(1.0).contains(p))
}

/// Precision the range readout should use, taken from the length of the window
/// it describes rather than from the zoom: a multi-day window wants dates, a
/// window of minutes wants clock times, and neither should flicker while the
/// view moves around underneath it.
fn caption_snap(span: i64) -> i64 {
    match span.max(1) {
        s if s >= 2 * SECS_PER_DAY => SECS_PER_DAY,
        s if s >= 4 * SECS_PER_HOUR => SECS_PER_HOUR,
        s if s >= 10 * SECS_PER_MINUTE => SECS_PER_MINUTE,
        _ => 1,
    }
}

/// Top edge of the bucket band for one weekday row. The rows converge on the top
/// of the bar as the expansion completes, so every layer that places itself with
/// this — a day cell and the finer buckets inside that day alike — stays
/// concentric with the shape it is replacing through the whole crossfade.
fn bar_top(tk: &ActivityHeatmapTokens, grid: Rect, row: usize, expand: f32) -> f32 {
    lerp(
        grid.top() + row as f32 * (tk.cell + tk.cell_gap),
        grid.top(),
        expand,
    )
}

/// Height of a bucket at this stage of the expansion — one row of the grid
/// while the weekday block still reads as a grid, the whole bar once a single
/// day owns the axis. Every layer that stands for a bucket (day cells, the finer
/// strip taking over from them, the per-file dashes inside it) grows on this one
/// curve, so the crossfade reads as one shape opening up rather than a
/// full-height overlay landing on top of something still growing.
fn bar_height(tk: &ActivityHeatmapTokens, grid: Rect, expand: f32) -> f32 {
    lerp(tk.cell, grid.height(), expand)
}

/// A cell always fills the slot it owns — the week column while the block reads
/// as a grid, its own day once the staircase separates them — so a spare span on
/// a wide panel packs the bar instead of scattering squares across it.
///
/// Nothing caps this at a square. A square cap could only ever bind once a week
/// column outgrew `cell + gap`, which is exactly where the stagger begins, so it
/// never shaped the grid form it was meant to protect; all it did was starve the
/// morph, freezing the cells while the gaps kept growing. The square look at
/// rest is preserved by the stagger onset itself, not by clamping width.
fn cell_width(week_px: f32, day_px: f32, gap: f32, stagger: f32) -> f32 {
    lerp(slot_fill(week_px, gap), slot_fill(day_px, gap), stagger)
}

/// A slot's width less its gap, where the gap gives way rather than eating the
/// slot: at a four-pixel pitch a fixed three-pixel gap is almost all gap, which
/// is what made deep zoom-outs read as scattered dots instead of dense history.
fn slot_fill(pitch: f32, gap: f32) -> f32 {
    (pitch - gap.min(pitch * 0.25)).max(1.0)
}

fn fade(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (color.a() as f32 * alpha.clamp(0.0, 1.0)).round() as u8,
    )
}

/// GitHub's green ramp (reads on both themes).
pub fn heat_color(level: u8, dark: bool) -> Color32 {
    if dark {
        match level {
            0 => Color32::from_rgb(0x16, 0x1b, 0x22),
            1 => Color32::from_rgb(0x0e, 0x44, 0x29),
            2 => Color32::from_rgb(0x00, 0x6d, 0x32),
            3 => Color32::from_rgb(0x26, 0xa6, 0x41),
            _ => Color32::from_rgb(0x39, 0xd3, 0x53),
        }
    } else {
        match level {
            0 => Color32::from_rgb(0xeb, 0xed, 0xf0),
            1 => Color32::from_rgb(0x9b, 0xe9, 0xa8),
            2 => Color32::from_rgb(0x40, 0xc4, 0x63),
            3 => Color32::from_rgb(0x30, 0xa1, 0x4e),
            _ => Color32::from_rgb(0x21, 0x6e, 0x39),
        }
    }
}

/// Desaturate toward luminance, then scale opacity — the mute that carries the
/// selection instead of a heavy border.
pub fn mute_heat(color: Color32, opacity: f32, saturation: f32) -> Color32 {
    let (r, g, b) = (color.r() as f32, color.g() as f32, color.b() as f32);
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let sat = saturation.clamp(0.0, 1.0);
    let mix = |c: f32| (c * sat + lum * (1.0 - sat)).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(
        mix(r),
        mix(g),
        mix(b),
        (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn clamp_view(view: &mut View, span_lo: i64, span_hi: i64, min_view: f64) {
    let span_w = (span_hi - span_lo).max(1) as f64;
    let w = (view.hi - view.lo).clamp(min_view.min(span_w), span_w);
    if w >= span_w {
        view.lo = span_lo as f64;
        view.hi = span_hi as f64;
    } else {
        if view.lo < span_lo as f64 {
            view.lo = span_lo as f64;
        }
        view.hi = view.lo + w;
        if view.hi > span_hi as f64 {
            view.hi = span_hi as f64;
            view.lo = view.hi - w;
        }
    }
    clamp_target(view, span_lo, span_hi, min_view);
}

fn clamp_target(view: &mut View, span_lo: i64, span_hi: i64, min_view: f64) {
    let span_w = (span_hi - span_lo).max(1) as f64;
    let w = (view.target_hi - view.target_lo).clamp(min_view.min(span_w), span_w);
    if w >= span_w {
        view.target_lo = span_lo as f64;
        view.target_hi = span_hi as f64;
        return;
    }
    if view.target_lo < span_lo as f64 {
        view.target_lo = span_lo as f64;
    }
    view.target_hi = view.target_lo + w;
    if view.target_hi > span_hi as f64 {
        view.target_hi = span_hi as f64;
        view.target_lo = view.target_hi - w;
    }
}

fn tick_step_secs(visible: f64, width: f32) -> i64 {
    let target = (visible * (56.0 / width.max(1.0) as f64)).max(1.0);
    const STEPS: [i64; 15] = [
        1,
        5,
        15,
        SECS_PER_MINUTE,
        5 * SECS_PER_MINUTE,
        15 * SECS_PER_MINUTE,
        SECS_PER_HOUR,
        6 * SECS_PER_HOUR,
        12 * SECS_PER_HOUR,
        SECS_PER_DAY,
        7 * SECS_PER_DAY,
        30 * SECS_PER_DAY,
        90 * SECS_PER_DAY,
        365 * SECS_PER_DAY,
        5 * 365 * SECS_PER_DAY,
    ];
    STEPS
        .iter()
        .copied()
        .find(|&s| (s as f64) >= target)
        .unwrap_or(5 * 365 * SECS_PER_DAY)
}

fn align_tick_start(view_lo: f64, step: i64) -> i64 {
    let t = view_lo.floor() as i64;
    if step >= SECS_PER_DAY {
        let day = bucket_start(t, SECS_PER_DAY);
        return day - day.rem_euclid(step);
    }
    bucket_start(t, step)
}

fn label_min_spacing(step: i64) -> f32 {
    if step >= 365 * SECS_PER_DAY {
        34.0
    } else if step >= 28 * SECS_PER_DAY {
        40.0
    } else if step >= SECS_PER_DAY {
        32.0
    } else if step >= SECS_PER_MINUTE {
        30.0
    } else {
        38.0
    }
}

fn draw_scale(
    painter: &egui::Painter,
    tk: &ActivityHeatmapTokens,
    axis: Rect,
    top: f32,
    view: &View,
    step: i64,
    theme: SidebarTheme,
) {
    let baseline = top + tk.scale_top_gap;
    let stroke = Stroke::new(1.0_f32, theme.sub.gamma_multiply(0.85));
    let min_gap = label_min_spacing(step);
    let mut t = align_tick_start(view.lo, step);
    let mut last_label = f32::MIN;
    while (t as f64) <= view.hi + step as f64 {
        let x = time_to_x(t as f64, axis, view.lo, view.hi);
        if x >= axis.left() - 2.0 && x <= axis.right() + 2.0 {
            painter.line_segment(
                [
                    Pos2::new(x, baseline),
                    Pos2::new(x, baseline + tk.scale_tick_len),
                ],
                stroke,
            );
            if x - last_label > min_gap {
                painter.text(
                    Pos2::new(x, baseline + tk.scale_tick_len + tk.scale_label_gap),
                    egui::Align2::CENTER_TOP,
                    timeline_tick_label(t, step),
                    FontId::proportional(tk.label_font),
                    theme.sub,
                );
                last_label = x;
            }
        }
        t += step;
    }
}

fn time_to_x(t: f64, axis: Rect, view_lo: f64, view_hi: f64) -> f32 {
    let w = (view_hi - view_lo).max(1.0);
    axis.left() + ((t - view_lo) / w * axis.width() as f64) as f32
}

fn time_at_x(x: f32, axis: Rect, view_lo: f64, view_hi: f64) -> f64 {
    if axis.width() <= 0.0 {
        return view_lo;
    }
    let f = ((x - axis.left()) / axis.width()) as f64;
    view_lo + f * (view_hi - view_lo).max(1.0)
}

fn group_digits(n: u64) -> String {
    crate::widgets::group_digits(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::types::day_start;

    fn axis() -> Rect {
        Rect::from_min_size(Pos2::new(20.0, 0.0), Vec2::new(700.0, 100.0))
    }

    #[test]
    fn the_axis_maps_time_to_x_and_back() {
        let (lo, hi) = (1_000_000.0, 1_000_000.0 + SECS_PER_DAY as f64);
        let a = axis();
        assert!((time_to_x(lo, a, lo, hi) - a.left()).abs() < 0.01);
        assert!((time_to_x(hi, a, lo, hi) - a.right()).abs() < 0.01);
        let mid = time_to_x(lo + 0.5 * SECS_PER_DAY as f64, a, lo, hi);
        assert!((mid - a.center().x).abs() < 0.5);
        // Round-trip: the handles and the graph must agree exactly, since one
        // axis serving both is the whole point.
        let t = lo + 12_345.0;
        let back = time_at_x(time_to_x(t, a, lo, hi), a, lo, hi);
        assert!((back - t).abs() < 2.0, "round-tripped {t} to {back}");
    }

    #[test]
    fn selection_lights_the_window_then_the_picks() {
        let picks = TimePicks::new();
        assert!(in_selection(100, 200, 0, 1_000, &picks));
        assert!(!in_selection(2_000, 2_100, 0, 1_000, &picks));
        let mut picks = TimePicks::new();
        picks.insert(0, 1_001);
        picks.remove(100, 200);
        assert!(!in_selection(100, 200, 0, 1_000, &picks), "punched hole");
        assert!(in_selection(300, 400, 0, 1_000, &picks));
    }

    #[test]
    fn cells_fill_their_slot_at_every_zoom() {
        let gap = 3.0;
        // Fully staggered, every day owns its slot however wide that becomes.
        assert_eq!(cell_width(700.0, 100.0, gap, 1.0), 97.0);
        for day_px in [16.0_f32, 40.0, 100.0, 900.0] {
            assert_eq!(cell_width(day_px * 7.0, day_px, gap, 1.0), day_px - gap);
        }
        // Grid form fills its week column, so a short span on a wide panel packs
        // the bar. Nothing caps this at a square; the regression was a `cell`
        // ceiling that grew the gaps instead of the cells.
        assert_eq!(cell_width(80.0, 11.4, gap, 0.0), 77.0);
        let mid = cell_width(80.0, 11.4, gap, 0.5);
        assert!(mid > 8.4 && mid < 77.0, "eases with the stagger, got {mid}");
        // The duty cycle holds at any pitch: a fixed gap must not eat the slot
        // when the whole slot is a few pixels wide.
        for pitch in [1.0_f32, 2.5, 4.0, 12.0, 60.0, 400.0] {
            let w = cell_width(pitch, pitch / 7.0, gap, 0.0);
            assert!(
                w >= (pitch * 0.7).min(pitch) && w <= pitch,
                "pitch {pitch} left {w}"
            );
        }
    }

    #[test]
    fn both_layers_grow_on_one_curve() {
        let tk = ActivityHeatmapTokens::default();
        let grid = Rect::from_min_size(Pos2::ZERO, Vec2::new(700.0, 98.0));
        assert_eq!(bar_height(&tk, grid, 0.0), tk.cell, "one row while a grid");
        assert_eq!(bar_height(&tk, grid, 1.0), grid.height(), "the whole bar");
        let half = bar_height(&tk, grid, 0.5);
        assert!(half > tk.cell && half < grid.height(), "grows, got {half}");
    }

    /// Left selects time, right moves the view. Within the left button, the
    /// handles have to out-rank the grip they sit inside, and a window covering
    /// the whole span must yield the drag to selection rather than swallow it
    /// sliding nowhere.
    #[test]
    fn a_left_press_routes_handles_before_grip_and_grip_only_when_movable() {
        // A rail spanning y 100..116, with its two handles at x 100 and 300.
        let handles = [
            Rect::from_center_size(Pos2::new(100.0, 108.0), Vec2::splat(14.0)),
            Rect::from_center_size(Pos2::new(300.0, 108.0), Vec2::splat(14.0)),
        ];
        // The grip spans the band between them, overlapping both handles.
        let grip = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 116.0));

        let on_lo = handles[0].center();
        let on_hi = handles[1].center();
        let between = Pos2::new(200.0, 108.0);
        let outside = Pos2::new(360.0, 108.0);

        assert_eq!(press_target(on_lo, &handles, grip, true), PressTarget::Lo);
        assert_eq!(press_target(on_hi, &handles, grip, true), PressTarget::Hi);
        assert_eq!(
            press_target(between, &handles, grip, true),
            PressTarget::Grip,
            "a cropped window slides"
        );
        assert_eq!(
            press_target(between, &handles, grip, false),
            PressTarget::Window,
            "a full-span window has nowhere to slide, so the drag selects"
        );
        assert_eq!(
            press_target(outside, &handles, grip, true),
            PressTarget::Window,
            "bare rail sweeps out a window"
        );
        // Anywhere over the graph, well above the rail, is always a window drag.
        assert_eq!(
            press_target(Pos2::new(200.0, 20.0), &handles, grip, true),
            PressTarget::Window
        );
    }

    #[test]
    fn a_finer_bucket_stays_concentric_with_its_day() {
        let tk = ActivityHeatmapTokens::default();
        let grid = Rect::from_min_size(Pos2::new(0.0, 40.0), Vec2::new(700.0, 98.0));
        // A Tuesday: row 2 of the block, and a bucket inside it.
        let day = day_index(1_700_000_000);
        let row = weekday_row(day);
        assert_eq!(row, 2, "fixture must be mid-week to be a real test");
        for expand in [0.0_f32, 0.15, 0.5, 0.85, 1.0] {
            let day_band = bar_top(&tk, grid, row, expand);
            let bucket = bar_top(&tk, grid, weekday_row(day), expand);
            assert_eq!(day_band, bucket, "same band at expand {expand}");
            // Equal tops and equal heights is what makes the centers agree.
            let h = bar_height(&tk, grid, expand);
            assert!(day_band + h <= grid.bottom() + 0.01, "stays in the bar");
        }
        // The rows only converge on the top of the bar at the end.
        assert!(bar_top(&tk, grid, 6, 0.0) > bar_top(&tk, grid, 0, 0.0));
        assert_eq!(bar_top(&tk, grid, 6, 1.0), bar_top(&tk, grid, 0, 1.0));
    }

    #[test]
    fn empty_buckets_are_inert_and_let_the_pointer_through() {
        let cell = |x: f32, w: f32, count: u32, alpha: f32| Cell {
            rect: Rect::from_min_size(Pos2::new(x, 0.0), Vec2::new(w, 20.0)),
            lo: 0,
            hi: SECS_PER_DAY,
            count,
            level: 1,
            alpha,
        };
        // A populated day, half covered by finer buckets: one empty, one not.
        let cells = vec![
            cell(0.0, 100.0, 9, 0.6),
            cell(0.0, 50.0, 0, 0.6),
            cell(50.0, 50.0, 4, 0.6),
        ];
        let over_empty = hit_cell(&cells, Pos2::new(25.0, 10.0)).expect("falls through");
        assert_eq!(over_empty.count, 9, "the day underneath answers instead");
        let over_full = hit_cell(&cells, Pos2::new(75.0, 10.0)).expect("hit");
        assert_eq!(over_full.count, 4, "frontmost legible layer wins");
        // Nothing at all when every bucket under the pointer is empty.
        assert!(hit_cell(&[cell(0.0, 100.0, 0, 1.0)], Pos2::new(50.0, 10.0)).is_none());
        // A layer only just fading in does not steal the pointer yet.
        let ghost = vec![cell(0.0, 100.0, 9, 1.0), cell(0.0, 100.0, 4, 0.05)];
        assert_eq!(hit_cell(&ghost, Pos2::new(50.0, 10.0)).unwrap().count, 9);
    }

    #[test]
    fn tick_steps_reach_seconds_and_years() {
        assert_eq!(tick_step_secs(60.0, 700.0), 5);
        // Labels need ~56 px, so a day resolves to six-hour ticks.
        assert_eq!(
            tick_step_secs(SECS_PER_DAY as f64, 700.0),
            6 * SECS_PER_HOUR
        );
        assert_eq!(
            tick_step_secs(3.0 * SECS_PER_HOUR as f64, 700.0),
            15 * SECS_PER_MINUTE
        );
        assert!(tick_step_secs(4_000.0 * SECS_PER_DAY as f64, 700.0) >= 365 * SECS_PER_DAY);
    }

    #[test]
    fn ticks_align_to_bucket_boundaries() {
        let noon = day_start(20_000) + 12 * SECS_PER_HOUR;
        assert_eq!(
            align_tick_start(noon as f64, SECS_PER_HOUR),
            noon,
            "noon is already on the hour"
        );
        assert_eq!(
            align_tick_start((noon + 61) as f64, SECS_PER_MINUTE),
            noon + SECS_PER_MINUTE
        );
        let aligned = align_tick_start(noon as f64, 7 * SECS_PER_DAY);
        assert_eq!(aligned.rem_euclid(7 * SECS_PER_DAY), 0);
    }

    #[test]
    fn clamping_never_leaves_the_span_or_inverts() {
        let mut view = View {
            lo: -5_000.0,
            hi: 900_000.0,
            target_lo: -5_000.0,
            target_hi: 900_000.0,
            span_lo: 0,
            span_hi: 100_000,
        };
        clamp_view(&mut view, 0, 100_000, 30.0);
        assert_eq!((view.lo, view.hi), (0.0, 100_000.0));
        // A window narrower than the floor widens back to it.
        view.lo = 10.0;
        view.hi = 12.0;
        clamp_view(&mut view, 0, 100_000, 30.0);
        assert!(view.hi - view.lo >= 30.0);
        assert!(view.lo >= 0.0 && view.hi <= 100_000.0);
    }

    #[test]
    fn mute_reduces_saturation_and_opacity() {
        let muted = mute_heat(Color32::from_rgb(0x39, 0xd3, 0x53), 0.4, 0.0);
        assert_eq!(muted.a(), (0.4_f32 * 255.0).round() as u8);
        assert_eq!(muted.r(), muted.g());
        assert_eq!(muted.g(), muted.b());
    }
}
