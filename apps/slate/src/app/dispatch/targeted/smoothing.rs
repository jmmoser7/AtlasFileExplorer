//! Explicit scoped smoothing through the ordinary Smooth result owner.
use super::*;
use crate::app::board_smooth::{prepare_smooth, SmoothGesture};
use eframe::egui::Pos2;

pub(super) fn prepare(
    app: &SlateApp,
    ids: &[NodeId],
    args: &Value,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>, SmoothGesture), String> {
    prepare::validate_keys(args, schema::arguments("board.smooth.stroke")?)?;
    if !(1..=64).contains(&ids.len()) {
        return Err("Smooth needs 1–64 explicit stroke targets".into());
    }
    let points = args["points"]
        .as_array()
        .filter(|points| (2..=4096).contains(&points.len()))
        .ok_or("Smooth needs 2–4096 sweep points")?
        .iter()
        .map(|point| {
            let pair = point
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or("Each smooth point must be [x,y]")?;
            let coordinate = |value: &Value| {
                value
                    .as_f64()
                    .filter(|value| value.is_finite() && value.abs() <= 1.0e9)
                    .map(|value| value as f32)
                    .ok_or("Smooth coordinates must be finite and inside board bounds")
            };
            Ok(Pos2::new(coordinate(&pair[0])?, coordinate(&pair[1])?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let option = |name: &str, default: f32| -> Result<f32, String> {
        args.get(name).map_or(Ok(default), |value| {
            value
                .as_f64()
                .filter(|v| v.is_finite())
                .map(|v| v as f32)
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("{name} must be finite"))
        })
    };
    let gesture = SmoothGesture {
        points,
        width: option("width", app.smooth_width)?,
        strength: option("strength", app.smooth_strength.clamp(0.05, 1.0))?,
    };
    let before = ids
        .iter()
        .map(|id| {
            app.doc()
                .scene
                .node(*id)
                .cloned()
                .ok_or_else(|| format!("Smooth target {} is missing", id.0))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let after = prepare_smooth(
        &before,
        &gesture.points,
        gesture.width,
        gesture.strength,
        app.tab().cam.z,
    )?;
    if after.is_empty() {
        return Err("The smooth path did not change any target ink".into());
    }
    let cmds = after
        .into_iter()
        .map(|after| {
            let before = before
                .iter()
                .find(|node| node.id == after.id)
                .expect("the shared smoother preserves target identities")
                .clone();
            SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            }
        })
        .collect();
    Ok((cmds, ids.to_vec(), gesture))
}
