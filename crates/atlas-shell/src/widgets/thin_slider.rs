//! Thin sidebar sliders and their optional user-editable value domains.
use super::*;
use std::ops::RangeInclusive;

/// Session-local range policy. The original minimum remains a hard floor.
#[derive(Clone, Debug)]
pub struct SliderDomain {
    bounds: RangeInclusive<usize>,
    floor: usize,
}

impl SliderDomain {
    pub fn new(bounds: RangeInclusive<usize>) -> Self {
        assert!(!bounds.is_empty(), "slider domain must be ordered");
        Self {
            floor: *bounds.start(),
            bounds,
        }
    }

    pub fn maximum(&self) -> usize {
        *self.bounds.end()
    }

    pub fn clamp(&self, value: usize) -> usize {
        value.clamp(*self.bounds.start(), *self.bounds.end())
    }

    fn parse_bounds(&self, min: &str, max: &str) -> Option<RangeInclusive<usize>> {
        // Integer parsing rejects NaN, infinity, negatives, fractions and overflow.
        let min = min.trim().parse::<usize>().ok()?;
        let max = max.trim().parse::<usize>().ok()?;
        (min >= self.floor && min <= max).then_some(min..=max)
    }

    fn apply(&mut self, min: &str, max: &str) -> Option<bool> {
        let next = self.parse_bounds(min, max)?;
        let changed = self.bounds != next;
        self.bounds = next;
        Some(changed)
    }

    fn menu(&mut self, response: &egui::Response) -> bool {
        let id = response.id.with("slider_domain");
        let ctx = response.ctx.clone();
        if response.secondary_clicked() {
            ctx.data_mut(|data| {
                data.insert_temp(
                    id,
                    [
                        self.bounds.start().to_string(),
                        self.bounds.end().to_string(),
                    ],
                )
            });
        }
        let mut changed = false;
        response.context_menu(|ui| {
            let dark = ui.visuals().dark_mode;
            crate::menu::prepare(ui, dark);
            let mut draft = ctx
                .data(|data| data.get_temp::<[String; 2]>(id))
                .unwrap_or_else(|| {
                    [
                        self.bounds.start().to_string(),
                        self.bounds.end().to_string(),
                    ]
                });
            ui.label("Slider range");
            for (label, text) in ["Minimum", "Maximum"].into_iter().zip(&mut draft) {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(text).desired_width(80.0));
                });
            }
            let valid = self.parse_bounds(&draft[0], &draft[1]).is_some();
            if !valid {
                ui.label(format!(
                    "Use whole numbers: minimum at least {}, maximum at least minimum.",
                    self.floor
                ));
            }
            if ui.add_enabled(valid, egui::Button::new("Apply")).clicked() {
                changed = self.apply(&draft[0], &draft[1]).unwrap_or(false);
                ctx.data_mut(|data| data.remove::<[String; 2]>(id));
                ui.close_menu();
            } else {
                ctx.data_mut(|data| data.insert_temp(id, draft));
            }
        });
        changed
    }
}

/// Fixed callers keep their existing behavior; opted-in owners retain edits.
pub enum SidebarSliderRange<'a> {
    Fixed(RangeInclusive<usize>),
    Editable(&'a mut SliderDomain),
}

impl From<RangeInclusive<usize>> for SidebarSliderRange<'_> {
    fn from(value: RangeInclusive<usize>) -> Self {
        Self::Fixed(value)
    }
}

impl<'a> From<&'a mut SliderDomain> for SidebarSliderRange<'a> {
    fn from(value: &'a mut SliderDomain) -> Self {
        Self::Editable(value)
    }
}

impl SidebarSliderRange<'_> {
    fn bounds(&self) -> RangeInclusive<usize> {
        match self {
            Self::Fixed(bounds) => bounds.clone(),
            Self::Editable(domain) => domain.bounds.clone(),
        }
    }
}

fn thin_slider_label_row(ui: &mut Ui, label: &str, value_text: &str, sub_color: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small().color(sub_color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value_text).small().color(sub_color));
        });
    });
}

pub fn thin_sidebar_slider<'a>(
    ui: &mut Ui,
    value: &mut usize,
    range: impl Into<SidebarSliderRange<'a>>,
    label: &str,
    unit: &str,
    hover: &str,
    sub_color: Color32,
) -> bool {
    let mut domain = range.into();
    let range = domain.bounds();
    let (lo, hi) = (*range.start(), *range.end());
    let mut domain_changed = false;
    let before = *value;
    *value = (*value).clamp(lo, hi);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = THIN_SLIDER_LABEL_GAP;
        let mut frac = if hi > lo {
            (*value - lo) as f32 / (hi - lo) as f32
        } else {
            0.0
        };
        let editing = slider_value_editing(ui.ctx(), ui.id().with((label, "thin")));
        let rail = thin_slider_rail(ui, &mut frac, hover, editing);
        // A handle click keeps the value until the field commits.
        if rail.changed && !rail.handle_click {
            *value = lo
                .saturating_add((frac * (hi - lo) as f32).round() as usize)
                .min(hi);
        }
        let field_id = ui.id().with((label, "thin"));
        let field = slider_value_field(
            ui,
            field_id,
            rail.handle + Vec2::new(0.0, -16.0),
            1.0,
            *value as f32,
            lo as f32..=hi as f32,
            unit,
            rail.handle_click,
            thin_field_chrome(ui, sub_color),
        );
        if let Some(v) = field.committed {
            *value = v.round().clamp(lo as f32, hi as f32) as usize;
        }
        if !field.open {
            thin_slider_label_row(ui, label, &format!("{} {}", *value, unit), sub_color);
        } else {
            thin_slider_label_row(ui, label, "", sub_color);
        }
        if let SidebarSliderRange::Editable(domain) = &mut domain {
            domain_changed = domain.menu(&rail.response);
        }
    });
    if let SidebarSliderRange::Editable(domain) = domain {
        *value = domain.clamp(*value);
    }
    *value != before || domain_changed
}

/// Signed variant of [`thin_sidebar_slider`] for ranges spanning zero
/// (e.g. hue rotation in degrees). Same rail, grip, and label layout.
pub fn thin_sidebar_slider_i32(
    ui: &mut Ui,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    label: &str,
    unit: &str,
    hover: &str,
    sub_color: Color32,
) -> bool {
    let (lo, hi) = (*range.start(), *range.end());
    let before = *value;
    *value = (*value).clamp(lo, hi);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = THIN_SLIDER_LABEL_GAP;
        let mut frac = if hi > lo {
            (*value - lo) as f32 / (hi - lo) as f32
        } else {
            0.0
        };
        let editing = slider_value_editing(ui.ctx(), ui.id().with((label, "thin")));
        let rail = thin_slider_rail(ui, &mut frac, hover, editing);
        if !rail.handle_click && rail.changed {
            *value = lo + (frac * (hi - lo) as f32).round() as i32;
        }
        let field = slider_value_field(
            ui,
            ui.id().with((label, "thin")),
            rail.handle + Vec2::new(0.0, -16.0),
            1.0,
            *value as f32,
            lo as f32..=hi as f32,
            unit,
            rail.handle_click,
            thin_field_chrome(ui, sub_color),
        );
        if let Some(v) = field.committed {
            *value = v.round().clamp(lo as f32, hi as f32) as i32;
        }
        if !field.open {
            thin_slider_label_row(ui, label, &format!("{} {}", *value, unit), sub_color);
        } else {
            thin_slider_label_row(ui, label, "", sub_color);
        }
    });
    *value != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_domain_input_never_changes_the_retained_policy() {
        let mut domain = SliderDomain::new(40..=300);
        for (min, max) in [
            ("NaN", "500"),
            ("40", "inf"),
            ("-1", "500"),
            ("40.5", "500"),
            ("39", "500"),
            ("500", "400"),
            ("40", "999999999999999999999999999999999999999"),
        ] {
            assert_eq!(domain.apply(min, max), None, "{min}..={max}");
            assert_eq!(domain.bounds, 40..=300);
        }
    }

    #[test]
    fn a_valid_extended_domain_is_retained_and_a_narrower_one_clamps_the_value() {
        let mut domain = SliderDomain::new(40..=300);
        assert_eq!(domain.apply("80", "750"), Some(true));
        assert_eq!(domain.clamp(600), 600);
        assert_eq!(domain.clamp(20), 80);
        assert_eq!(domain.apply("80", "100"), Some(true));
        assert_eq!(domain.clamp(600), 100);
        assert_eq!(domain.apply("80", "100"), Some(false));
    }
}
