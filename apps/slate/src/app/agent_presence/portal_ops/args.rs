use eframe::egui;
use serde_json::Value;

pub(super) fn number(args: &Value, key: &str, default: usize, max: usize) -> Result<usize, String> {
    let n = args.get(key).map_or(Ok(default), |v| {
        v.as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| format!("{key} must be a nonnegative integer"))
    })?;
    if n > max {
        return Err(format!("{key} exceeds its bound"));
    }
    Ok(n)
}
pub(super) fn finite(args: &Value, key: &str) -> Result<f32, String> {
    let n = args
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("{key} must be a number"))?;
    if !n.is_finite() || !(-100_000.0..=100_000.0).contains(&n) {
        return Err(format!("{key} is outside the page bounds"));
    }
    Ok(n as f32)
}
pub(super) fn page_coord(args: &Value, key: &str) -> Result<f32, String> {
    let n = finite(args, key)?;
    if !(0.0..=8192.0).contains(&n) {
        return Err(format!(
            "{key} is outside the bounded page coordinate range"
        ));
    }
    Ok(n)
}
pub(super) fn web_key(key: &str) -> Option<egui::Key> {
    Some(match key {
        "Enter" => egui::Key::Enter,
        "Escape" => egui::Key::Escape,
        "Tab" => egui::Key::Tab,
        "Backspace" => egui::Key::Backspace,
        "Delete" => egui::Key::Delete,
        "Space" => egui::Key::Space,
        "ArrowUp" => egui::Key::ArrowUp,
        "ArrowDown" => egui::Key::ArrowDown,
        "ArrowLeft" => egui::Key::ArrowLeft,
        "ArrowRight" => egui::Key::ArrowRight,
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::super::recognizes;
    use super::*;
    use serde_json::json;

    #[test]
    fn portal_command_names_are_explicit_and_page_coordinates_are_bounded() {
        assert!(recognizes("portal.web.focus"));
        assert!(recognizes("portal.atlas.read_file"));
        assert!(recognizes("portal.web.click"));
        assert!(!recognizes("portal.web.evaluate"));
        assert_eq!(page_coord(&json!({"x":8192}), "x").unwrap(), 8192.0);
        assert!(page_coord(&json!({"x":8193}), "x").is_err());
        assert!(page_coord(&json!({"x":-1}), "x").is_err());
        assert!(web_key("F12").is_none());
        assert_eq!(web_key("Enter"), Some(egui::Key::Enter));
    }
}
