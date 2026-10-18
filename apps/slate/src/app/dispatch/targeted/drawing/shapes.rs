//! Recipe-backed parametric results for witnessed rectangle gestures.
use super::*;

pub(super) fn supports(kind: &str) -> bool {
    matches!(kind, "rect" | "ellipse" | "polygon")
}

pub(super) fn draw(
    app: &SlateApp,
    scene: &mut slate_doc::Scene,
    kind: &str,
    input: &Value,
    points: &[Pos2],
    closed: bool,
) -> Result<slate_doc::Node, String> {
    if points.len() != 2 || closed || input.get("softness").is_some() {
        return Err(
            "Parametric shapes need two corners and do not accept closed or softness".into(),
        );
    }
    let sides = input
        .get("sides")
        .map(|value| {
            value
                .as_u64()
                .filter(|n| (3..=128).contains(n))
                .map(|n| n as u8)
                .ok_or("Polygon sides must be an integer from 3 to 128")
        })
        .transpose()?;
    if kind != "polygon" && sides.is_some() {
        return Err("sides is available for polygon only".into());
    }
    let rect = WorldRect::new(
        points[0].x.min(points[1].x),
        points[0].y.min(points[1].y),
        (points[0].x - points[1].x).abs(),
        (points[0].y - points[1].y).abs(),
    );
    if !super::valid_rect(rect) {
        return Err("Shape corners must enclose a positive bounded rectangle".into());
    }
    let width = super::optional_number(input, "width")?;
    if width.is_some_and(|v| !(0.01..=10_000.0).contains(&v)) {
        return Err("Stroke width must be between 0.01 and 10000".into());
    }
    let color = input.get("color").map(super::rgba).transpose()?;
    let opacity = super::optional_number(input, "opacity")?;
    if opacity.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
        return Err("Opacity must be between 0 and 1".into());
    }
    board_path::parametric_shape_node(app, scene, kind, rect, sides, width, color, opacity)
}
