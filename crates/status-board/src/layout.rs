//! Deterministic instrument layout. Coordinates are local to the frame
//! (origin top-left). Both interpreters consume [`StatusLayout::prims`].

use crate::model::{Snapshot, StatusQuery};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Fill {
        rect: Rect,
        rgba: [u8; 4],
    },
    Stroke {
        rect: Rect,
        rgba: [u8; 4],
        width: f32,
    },
    Bar {
        rect: Rect,
        frac: f32,
        rgba: [u8; 4],
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        rgba: [u8; 4],
        align: Align,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatusLayout {
    pub bounds: Size,
    pub prims: Vec<Prim>,
    pub caption: String,
}

/// Instrument palette — shared by painter and artifact (not chrome).
pub const INK: [u8; 4] = [216, 212, 200, 255];
pub const MUTED: [u8; 4] = [138, 145, 136, 255];
pub const TEAL: [u8; 4] = [61, 155, 143, 255];
pub const AMBER: [u8; 4] = [196, 154, 108, 255];
pub const CORAL: [u8; 4] = [196, 92, 74, 255];
pub const PANEL: [u8; 4] = [26, 33, 40, 255];
pub const LINE: [u8; 4] = [42, 51, 60, 255];

const PAD: f32 = 16.0;
const LOD_COMPACT: f32 = 220.0;
const LOD_DETAIL: f32 = 420.0;

pub fn layout_status(snap: &Snapshot, query: &StatusQuery, frame: Size) -> StatusLayout {
    let mut prims = Vec::new();
    let w = frame.w.max(1.0);
    let h = frame.h.max(1.0);
    let inner_w = (w - PAD * 2.0).max(1.0);
    let mut y = PAD;

    prims.push(Prim::Fill {
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        },
        rgba: [14, 17, 20, 255],
    });

    // Caption
    prims.push(Prim::Text {
        x: PAD,
        y,
        text: "STATUS BOARD".into(),
        size: 10.0,
        rgba: TEAL,
        align: Align::Left,
    });
    y += 16.0;
    prims.push(Prim::Text {
        x: PAD,
        y,
        text: format!("HEAD {} · {}", snap.meta.head_commit, snap.meta.generated),
        size: 11.0,
        rgba: MUTED,
        align: Align::Left,
    });
    y += 22.0;

    if query.show_overview {
        y = kpi_row(&mut prims, snap, PAD, y, inner_w);
        y += 12.0;
    }

    let show_bars = h >= LOD_COMPACT;
    if show_bars && query.show_phases && !snap.progress_scores.phase_scores.is_empty() {
        y = bar_block(
            &mut prims,
            "ROADMAP PHASES",
            &snap.progress_scores.phase_scores,
            PAD,
            y,
            inner_w,
        );
        y += 10.0;
    }
    if show_bars && query.show_waves && !snap.progress_scores.wave_scores.is_empty() {
        y = bar_block(
            &mut prims,
            "WORKPLAN WAVES",
            &snap.progress_scores.wave_scores,
            PAD,
            y,
            inner_w,
        );
        y += 10.0;
    }

    if h >= LOD_DETAIL && query.show_next {
        y = next_block(&mut prims, snap, PAD, y, inner_w);
        y += 10.0;
    }
    if h >= LOD_DETAIL && query.show_deviations {
        y = deviation_block(&mut prims, snap, PAD, y, inner_w);
    }

    let _ = y; // layout is top-down; leftover height is void
    StatusLayout {
        bounds: Size { w, h },
        prims,
        caption: format!(
            "{} · {} · roadmap {:.0}% · {} open deviations",
            snap.meta.head_commit,
            snap.meta.generated,
            snap.progress_scores.overall_roadmap * 100.0,
            snap.deviations.open
        ),
    }
}

fn kpi_row(prims: &mut Vec<Prim>, snap: &Snapshot, x0: f32, y: f32, inner_w: f32) -> f32 {
    let gap = 8.0;
    let cell_w = ((inner_w - gap * 3.0) / 4.0).max(40.0);
    let cell_h = 56.0;
    let items: [(&str, String, [u8; 4]); 4] = [
        (
            "ROADMAP",
            format!("{:.0}%", snap.progress_scores.overall_roadmap * 100.0),
            TEAL,
        ),
        (
            "WORKPLAN",
            format!(
                "{:.0}%",
                snap.progress_scores.overall_workplan_through_w2 * 100.0
            ),
            INK,
        ),
        ("OPEN DVs", snap.deviations.open.to_string(), CORAL),
        ("FORMAT", "v2".into(), TEAL),
    ];
    for (i, (label, value, color)) in items.iter().enumerate() {
        let x = x0 + i as f32 * (cell_w + gap);
        let rect = Rect {
            x,
            y,
            w: cell_w,
            h: cell_h,
        };
        prims.push(Prim::Fill { rect, rgba: PANEL });
        prims.push(Prim::Stroke {
            rect,
            rgba: LINE,
            width: 1.0,
        });
        prims.push(Prim::Text {
            x: x + 8.0,
            y: y + 8.0,
            text: (*label).into(),
            size: 9.0,
            rgba: MUTED,
            align: Align::Left,
        });
        prims.push(Prim::Text {
            x: x + 8.0,
            y: y + 26.0,
            text: value.clone(),
            size: 20.0,
            rgba: *color,
            align: Align::Left,
        });
    }
    y + cell_h
}

fn bar_block(
    prims: &mut Vec<Prim>,
    title: &str,
    scores: &std::collections::BTreeMap<String, f64>,
    x0: f32,
    mut y: f32,
    inner_w: f32,
) -> f32 {
    prims.push(Prim::Text {
        x: x0,
        y,
        text: title.into(),
        size: 10.0,
        rgba: MUTED,
        align: Align::Left,
    });
    y += 16.0;
    let label_w = 92.0;
    let pct_w = 36.0;
    let bar_w = (inner_w - label_w - pct_w - 12.0).max(20.0);
    for (name, frac) in scores {
        let f = (*frac as f32).clamp(0.0, 1.0);
        prims.push(Prim::Text {
            x: x0,
            y: y + 1.0,
            text: name.replace('_', " "),
            size: 11.0,
            rgba: MUTED,
            align: Align::Left,
        });
        let bar = Rect {
            x: x0 + label_w,
            y: y + 3.0,
            w: bar_w,
            h: 8.0,
        };
        prims.push(Prim::Fill {
            rect: bar,
            rgba: [14, 17, 20, 255],
        });
        prims.push(Prim::Stroke {
            rect: bar,
            rgba: LINE,
            width: 1.0,
        });
        prims.push(Prim::Bar {
            rect: bar,
            frac: f,
            rgba: bar_color(f),
        });
        prims.push(Prim::Text {
            x: x0 + inner_w,
            y: y + 1.0,
            text: format!("{:.0}%", f * 100.0),
            size: 11.0,
            rgba: INK,
            align: Align::Right,
        });
        y += 16.0;
    }
    y
}

fn next_block(prims: &mut Vec<Prim>, snap: &Snapshot, x0: f32, mut y: f32, inner_w: f32) -> f32 {
    prims.push(Prim::Text {
        x: x0,
        y,
        text: "NEXT".into(),
        size: 10.0,
        rgba: MUTED,
        align: Align::Left,
    });
    y += 16.0;
    for (i, action) in snap
        .capabilities_inventory
        .next_critical
        .iter()
        .take(4)
        .enumerate()
    {
        prims.push(Prim::Text {
            x: x0,
            y,
            text: format!("{}. {}", i + 1, action.item),
            size: 12.0,
            rgba: INK,
            align: Align::Left,
        });
        y += 16.0;
        if !action.why.is_empty() && inner_w > 200.0 {
            prims.push(Prim::Text {
                x: x0 + 16.0,
                y,
                text: truncate(&action.why, 72),
                size: 10.0,
                rgba: MUTED,
                align: Align::Left,
            });
            y += 14.0;
        }
    }
    y
}

fn deviation_block(
    prims: &mut Vec<Prim>,
    snap: &Snapshot,
    x0: f32,
    mut y: f32,
    _inner_w: f32,
) -> f32 {
    prims.push(Prim::Text {
        x: x0,
        y,
        text: format!(
            "DEVIATIONS · {} open · {} closed",
            snap.deviations.open, snap.deviations.closed
        ),
        size: 10.0,
        rgba: MUTED,
        align: Align::Left,
    });
    y += 16.0;
    for row in snap
        .deviations
        .rows
        .iter()
        .filter(|r| r.status == "open")
        .take(6)
    {
        prims.push(Prim::Text {
            x: x0,
            y,
            text: format!(
                "{}  Art.{}  {}",
                row.id,
                row.article,
                truncate(&row.summary, 56)
            ),
            size: 11.0,
            rgba: CORAL,
            align: Align::Left,
        });
        y += 15.0;
    }
    y
}

fn bar_color(frac: f32) -> [u8; 4] {
    if frac >= 0.85 {
        TEAL
    } else if frac >= 0.35 {
        AMBER
    } else {
        CORAL
    }
}

fn truncate(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}
