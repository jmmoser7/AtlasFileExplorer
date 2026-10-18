//! Attribute reconciliation at merged curve endpoints.
use super::*;
use vector_ink::JoinSource;

/// Carry the geometry join's vertex trace, averaging the two endpoint
/// attributes when geometry merged them into one corner.
pub fn joined_vertex_styles(
    first: &[VertexStyle],
    second: &[VertexStyle],
    trace: &[JoinSource],
) -> Vec<VertexStyle> {
    let mut out: Vec<_> = trace
        .iter()
        .map(|source| match *source {
            JoinSource::First(i) => first[i],
            JoinSource::Second(i) => second[i],
        })
        .collect();
    let missing = (0..second.len()).find(|i| !trace.contains(&JoinSource::Second(*i)));
    if let Some(missing) = missing {
        if let Some(seam) = trace
            .iter()
            .rposition(|source| matches!(source, JoinSource::First(_)))
        {
            out[seam] = average_vertex_style(out[seam], second[missing]);
        }
    }
    out
}

pub fn average_vertex_style(a: VertexStyle, b: VertexStyle) -> VertexStyle {
    VertexStyle {
        tip: StrokeSpan {
            width: (a.tip.width + b.tip.width) * 0.5,
            color: Rgba(std::array::from_fn(|i| {
                (u16::from(a.tip.color.0[i]) + u16::from(b.tip.color.0[i])).div_ceil(2) as u8
            })),
            softness: (a.tip.softness + b.tip.softness) * 0.5,
            texture: a.tip.texture,
        },
        corner: match (a.corner, b.corner) {
            (Some(a), Some(b)) => Some((a + b) * 0.5),
            (a, b) => a.or(b),
        },
    }
}

/// Write `styles`, one per vertex, onto `path` stroked with `stroke`: a hard
/// stroke's width becomes its widest tip and equal tips set the stroke
/// itself ([`set_grip_tips`]). Corner overrides are kept only where they
/// apply, on a straight-segment polyline. Nothing changes when `styles`
/// does not fit the path.
pub fn apply_vertex_styles(path: &mut PathData, stroke: &mut Stroke, styles: &[VertexStyle]) {
    if styles.is_empty() || styles.len() != vertex_count(path) {
        return;
    }
    let tips: Vec<StrokeSpan> = styles.iter().map(|s| s.tip).collect();
    if stroke.paints_as_stamp() {
        path.tips = tips;
    } else {
        write_tips(path, stroke, tips);
    }
    path.corner_amounts.clear();
    if crate::geom::path_is_line_polyline(path) && styles.iter().any(|s| s.corner.is_some()) {
        path.corner_amounts = styles.iter().map(|s| s.corner).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merged_join_averages_width_color_and_opacity_with_reversed_sources() {
        let a = VertexStyle {
            tip: StrokeSpan {
                width: 2.0,
                color: Rgba([255, 0, 0, 51]),
                ..StrokeSpan::of(&Stroke::default())
            },
            corner: None,
        };
        let b = VertexStyle {
            tip: StrokeSpan {
                width: 10.0,
                color: Rgba([0, 0, 255, 255]),
                ..a.tip
            },
            corner: None,
        };
        let out = joined_vertex_styles(
            &[a, a],
            &[b, b],
            &[
                JoinSource::First(1),
                JoinSource::First(0),
                JoinSource::Second(0),
            ],
        );
        assert_eq!(out[1].tip.width, 6.0);
        assert_eq!(out[1].tip.color, Rgba([128, 0, 128, 153]));
        assert_eq!(out[0], a);
        assert_eq!(out[2], b);
    }
}
