//! Full-access presentation only. The app owns grants and provider policy.
use super::*;

pub const FULL_ACCESS_HINT: &str = "Full access: run commands and edit files without asking";

#[derive(Clone, Copy)]
pub struct AccessView {
    /// Machine-wide default for subsequent dashboard requests.
    pub enabled: bool,
    /// Actual authority of the displayed request/builder, including restrictions.
    pub effective: bool,
    pub interactive: bool,
}

pub fn ink(enabled: bool, theme: Palette) -> Color32 {
    if enabled {
        theme.danger
    } else {
        theme.ink
    }
}

pub fn model_hint(view: AccessView) -> Option<&'static str> {
    if view.enabled && !view.effective {
        Some("Full access is on globally; this run remains restricted.")
    } else if view.effective {
        Some(FULL_ACCESS_HINT)
    } else {
        None
    }
}

/// Existing chat ellipsis and generator model menus share this warning row.
pub fn menu_row(enabled: bool) -> crate::menu::Row<'static> {
    let row = crate::menu::Row::new(crate::menu::MenuIcon::Lock, "Full access");
    if enabled {
        row.checked(true).danger()
    } else {
        row
    }
}

pub fn label_hint(response: egui::Response, enabled: bool) -> egui::Response {
    if enabled {
        object_hover_text(response, FULL_ACCESS_HINT)
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_permission_color_and_hint_do_not_confuse_the_global_default_with_this_run() {
        let theme = Palette::dark();
        let restricted = AccessView {
            enabled: true,
            effective: false,
            interactive: true,
        };
        assert_eq!(ink(restricted.effective, theme), theme.ink);
        assert!(model_hint(restricted)
            .unwrap()
            .contains("remains restricted"));
        assert!(
            menu_row(restricted.enabled).danger,
            "the global grant is still explicit"
        );
        let granted = AccessView {
            enabled: false,
            effective: true,
            interactive: true,
        };
        assert_eq!(ink(granted.effective, theme), theme.danger);
        assert_eq!(model_hint(granted), Some(FULL_ACCESS_HINT));
        assert!(
            !menu_row(granted.enabled).danger,
            "conversation grants do not enable the global default"
        );
    }
}
