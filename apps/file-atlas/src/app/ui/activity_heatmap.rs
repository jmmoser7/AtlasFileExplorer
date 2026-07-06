//! GitHub-style contribution heatmap for file activity by day.

use atlas_core::types::{date_string, day_index, month_short, ymd_from_secs, SECS_PER_DAY};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Sense, Ui, Vec2};

/// Sunday-aligned week grid: 7 rows (Sun–Sat) × N week columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityHeatmap {
    /// Inclusive first day (`day_index`).
    pub start_day: i64,
    /// Inclusive last day with data (may extend past last count for grid padding).
    pub end_day: i64,
    /// Count per calendar day, keyed by `day_index`.
    pub counts: std::collections::HashMap<i64, u32>,
    pub total_files: u32,
    pub max_count: u32,
}

impl ActivityHeatmap {
    pub fn empty() -> Self {
        Self {
            start_day: 0,
            end_day: 0,
            counts: std::collections::HashMap::new(),
            total_files: 0,
            max_count: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.total_files == 0
    }

    /// Build a heatmap from unix-second timestamps (one entry per file).
    pub fn from_timestamps(timestamps: impl IntoIterator<Item = i64>) -> Self {
        let mut counts: std::collections::HashMap<i64, u32> = std::collections::HashMap::new();
        let mut min_day = i64::MAX;
        let mut max_day = i64::MIN;
        let mut total = 0u32;

        for secs in timestamps {
            if secs <= 0 {
                continue;
            }
            let day = day_index(secs);
            min_day = min_day.min(day);
            max_day = max_day.max(day);
            *counts.entry(day).or_insert(0) += 1;
            total += 1;
        }

        if total == 0 {
            return Self::empty();
        }

        let start_day = sunday_on_or_before(min_day);
        let end_day = saturday_on_or_after(max_day);
        let max_count = counts.values().copied().max().unwrap_or(0);

        Self {
            start_day,
            end_day,
            counts,
            total_files: total,
            max_count,
        }
    }

    pub fn week_columns(&self) -> usize {
        if self.end_day < self.start_day {
            return 0;
        }
        ((self.end_day - self.start_day) / 7 + 1) as usize
    }

    pub fn count_at(&self, day: i64) -> u32 {
        self.counts.get(&day).copied().unwrap_or(0)
    }

    pub fn day_at(&self, week: usize, row: usize) -> i64 {
        self.start_day + week as i64 * 7 + row as i64
    }
}

/// GitHub-style green ramp (works on dark and light backgrounds).
fn heat_color(level: u8, dark: bool) -> Color32 {
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

fn level_for_count(count: u32, max: u32) -> u8 {
    if count == 0 {
        return 0;
    }
    if max <= 1 {
        return 1;
    }
    let ratio = count as f32 / max as f32;
    if ratio <= 0.25 {
        1
    } else if ratio <= 0.5 {
        2
    } else if ratio <= 0.75 {
        3
    } else {
        4
    }
}

/// Sunday on or before the given day index (GitHub row alignment).
fn sunday_on_or_before(day: i64) -> i64 {
    day - (day + 4).rem_euclid(7)
}

/// Saturday on or after the given day index.
fn saturday_on_or_after(day: i64) -> i64 {
    day + (6 - (day + 4).rem_euclid(7))
}

const CELL: f32 = 11.0;
const GAP: f32 = 3.0;
const DAY_LABEL_W: f32 = 14.0;
const MONTH_LABEL_H: f32 = 14.0;

pub fn draw_activity_heatmap(
    ui: &mut Ui,
    heatmap: &ActivityHeatmap,
    date_field_label: &str,
    source_label: &str,
    dark: bool,
    muted: Color32,
) {
    if heatmap.is_empty() {
        ui.label(
            RichText::new(format!("No {date_field_label} dates for {source_label}"))
                .small()
                .color(muted),
        );
        return;
    }

    let weeks = heatmap.week_columns();
    let grid_w = weeks as f32 * (CELL + GAP) - GAP;
    let grid_h = 7.0 * (CELL + GAP) - GAP;
    let legend_h = CELL + 6.0;
    let total_w = DAY_LABEL_W + grid_w;
    let total_h = MONTH_LABEL_H + grid_h + legend_h;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!(
                "{source_label} · {} files · {date_field_label}",
                heatmap.total_files
            ))
            .small()
            .color(muted),
        );
    });

    egui::ScrollArea::horizontal()
        .id_salt("activity_heatmap_scroll")
        .show(ui, |ui| {
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(total_w.max(ui.available_width()), total_h), Sense::hover());
            let painter = ui.painter_at(rect);
            let origin = rect.min + Vec2::new(DAY_LABEL_W, MONTH_LABEL_H);

            // Month labels along the top.
            draw_month_labels(&painter, heatmap, origin, grid_w, muted);

            // Day-of-week labels (Mon/Wed/Fri like GitHub).
            for &(row, label) in &[(1, "Mon"), (3, "Wed"), (5, "Fri")] {
                let y = origin.y + row as f32 * (CELL + GAP) + CELL * 0.5;
                painter.text(
                    Pos2::new(rect.left() + 2.0, y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(9.0),
                    muted,
                );
            }

            // Cells.
            for week in 0..weeks {
                for row in 0..7 {
                    let day = heatmap.day_at(week, row);
                    if day > heatmap.end_day {
                        continue;
                    }
                    let count = heatmap.count_at(day);
                    let level = level_for_count(count, heatmap.max_count);
                    let cell = Rect::from_min_size(
                        Pos2::new(
                            origin.x + week as f32 * (CELL + GAP),
                            origin.y + row as f32 * (CELL + GAP),
                        ),
                        Vec2::splat(CELL),
                    );
                    painter.rect_filled(cell, 2.0, heat_color(level, dark));

                    if count > 0 {
                        let secs = day * SECS_PER_DAY;
                        let tip = if count == 1 {
                            format!("1 file on {} ({date_field_label})", date_string(secs))
                        } else {
                            format!(
                                "{count} files on {} ({date_field_label})",
                                date_string(secs)
                            )
                        };
                        ui.interact(
                            cell,
                            ui.id().with("activity_cell").with(day),
                            Sense::hover(),
                        )
                        .on_hover_text(tip);
                    }
                }
            }

            // Legend.
            let legend_y = origin.y + grid_h + 6.0;
            let legend_x = origin.x;
            painter.text(
                Pos2::new(legend_x, legend_y),
                egui::Align2::LEFT_BOTTOM,
                "Less",
                FontId::proportional(9.0),
                muted,
            );
            for lvl in 0..5 {
                let lx = legend_x + 28.0 + lvl as f32 * (CELL + 2.0);
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(lx, legend_y - CELL), Vec2::splat(CELL)),
                    2.0,
                    heat_color(lvl, dark),
                );
            }
            painter.text(
                Pos2::new(legend_x + 28.0 + 5.0 * (CELL + 2.0) + 4.0, legend_y),
                egui::Align2::LEFT_BOTTOM,
                "More",
                FontId::proportional(9.0),
                muted,
            );

            resp.on_hover_text(format!(
                "Activity by {date_field_label} date · {weeks} weeks · scroll horizontally for long timelines"
            ));
        });
}

fn draw_month_labels(
    painter: &egui::Painter,
    heatmap: &ActivityHeatmap,
    origin: Pos2,
    _grid_w: f32,
    muted: Color32,
) {
    let weeks = heatmap.week_columns();
    let mut last_month: Option<u32> = None;
    for week in 0..weeks {
        let day = heatmap.day_at(week, 0);
        if day > heatmap.end_day {
            break;
        }
        let (_, m, _) = ymd_from_secs(day * SECS_PER_DAY);
        if last_month != Some(m) {
            let x = origin.x + week as f32 * (CELL + GAP);
            painter.text(
                Pos2::new(x, origin.y - 4.0),
                egui::Align2::LEFT_BOTTOM,
                month_short(m),
                FontId::proportional(9.0),
                muted,
            );
            last_month = Some(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_timestamps_yield_empty_heatmap() {
        let h = ActivityHeatmap::from_timestamps([]);
        assert!(h.is_empty());
    }

    #[test]
    fn counts_bucket_by_day() {
        let day = 20_000;
        let secs = day * SECS_PER_DAY + 3600;
        let h = ActivityHeatmap::from_timestamps([secs, secs + 100, (day + 1) * SECS_PER_DAY]);
        assert_eq!(h.total_files, 3);
        assert_eq!(h.count_at(day), 2);
        assert_eq!(h.count_at(day + 1), 1);
    }

    #[test]
    fn grid_aligns_to_sunday() {
        // 1970-01-01 is Thursday (day_index 0). Use one second past midnight:
        // timestamp 0 itself is rejected by the `secs <= 0` sentinel filter.
        let h = ActivityHeatmap::from_timestamps([1]);
        assert_eq!(h.start_day, -4); // preceding Sunday
        assert!(h.week_columns() >= 1);
    }

    #[test]
    fn multi_year_span_expands_columns() {
        let y1 = 365 * SECS_PER_DAY;
        let y2 = 800 * SECS_PER_DAY;
        let h = ActivityHeatmap::from_timestamps([y1, y2]);
        assert!(
            h.week_columns() > 52,
            "multi-year span should exceed one year of columns"
        );
    }

    #[test]
    fn level_scales_with_max() {
        assert_eq!(level_for_count(0, 10), 0);
        assert_eq!(level_for_count(1, 10), 1);
        assert_eq!(level_for_count(10, 10), 4);
    }
}
