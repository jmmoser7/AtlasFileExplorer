//! One SVG path-data serializer for board clips and artifact paths.
use super::*;

pub(crate) fn path_data_d(path: &PathData, w: f32, h: f32) -> String {
    let mut d = String::new();
    let (x, y) = denorm_pt(path.start, w, h);
    d.push_str("M ");
    d.push_str(&fmt_px(x));
    d.push(' ');
    d.push_str(&fmt_px(y));
    for seg in &path.segs {
        match seg {
            PathSeg::Line { to } => {
                let (x, y) = denorm_pt(*to, w, h);
                d.push_str(" L ");
                d.push_str(&fmt_px(x));
                d.push(' ');
                d.push_str(&fmt_px(y));
            }
            PathSeg::Quad { ctrl, to } => {
                let (cx, cy) = denorm_pt(*ctrl, w, h);
                let (x, y) = denorm_pt(*to, w, h);
                d.push_str(" Q ");
                d.push_str(&fmt_px(cx));
                d.push(' ');
                d.push_str(&fmt_px(cy));
                d.push(' ');
                d.push_str(&fmt_px(x));
                d.push(' ');
                d.push_str(&fmt_px(y));
            }
            PathSeg::Cubic { c1, c2, to } => {
                let (c1x, c1y) = denorm_pt(*c1, w, h);
                let (c2x, c2y) = denorm_pt(*c2, w, h);
                let (x, y) = denorm_pt(*to, w, h);
                d.push_str(" C ");
                d.push_str(&fmt_px(c1x));
                d.push(' ');
                d.push_str(&fmt_px(c1y));
                d.push(' ');
                d.push_str(&fmt_px(c2x));
                d.push(' ');
                d.push_str(&fmt_px(c2y));
                d.push(' ');
                d.push_str(&fmt_px(x));
                d.push(' ');
                d.push_str(&fmt_px(y));
            }
        }
    }
    if path.closed {
        d.push_str(" Z");
    }
    for extra in &path.extra {
        let (x, y) = denorm_pt(extra.start, w, h);
        d.push_str(" M ");
        d.push_str(&fmt_px(x));
        d.push(' ');
        d.push_str(&fmt_px(y));
        for seg in &extra.segs {
            match seg {
                PathSeg::Line { to } => {
                    let (x, y) = denorm_pt(*to, w, h);
                    d.push_str(" L ");
                    d.push_str(&fmt_px(x));
                    d.push(' ');
                    d.push_str(&fmt_px(y));
                }
                PathSeg::Quad { ctrl, to } => {
                    let (cx, cy) = denorm_pt(*ctrl, w, h);
                    let (x, y) = denorm_pt(*to, w, h);
                    d.push_str(" Q ");
                    d.push_str(&fmt_px(cx));
                    d.push(' ');
                    d.push_str(&fmt_px(cy));
                    d.push(' ');
                    d.push_str(&fmt_px(x));
                    d.push(' ');
                    d.push_str(&fmt_px(y));
                }
                PathSeg::Cubic { c1, c2, to } => {
                    let (c1x, c1y) = denorm_pt(*c1, w, h);
                    let (c2x, c2y) = denorm_pt(*c2, w, h);
                    let (x, y) = denorm_pt(*to, w, h);
                    d.push_str(" C ");
                    d.push_str(&fmt_px(c1x));
                    d.push(' ');
                    d.push_str(&fmt_px(c1y));
                    d.push(' ');
                    d.push_str(&fmt_px(c2x));
                    d.push(' ');
                    d.push_str(&fmt_px(c2y));
                    d.push(' ');
                    d.push_str(&fmt_px(x));
                    d.push(' ');
                    d.push_str(&fmt_px(y));
                }
            }
        }
        if extra.closed {
            d.push_str(" Z");
        }
    }
    d
}
