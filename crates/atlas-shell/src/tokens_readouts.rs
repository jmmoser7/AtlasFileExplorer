use serde::{Deserialize, Serialize};

/// The bottom readout bar that hosts the gear menu, the live counts, and the
/// activity timeline. Several unrelated readouts compete for the same few
/// vertical pixels here, so its padding and text size are dials rather than
/// constants — the balance between them is a judgement made by eye.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadoutTokens {
    pub pad_top: f32,
    pub pad_bottom: f32,
    pub row_gap: f32,
    pub item_gap: f32,
    pub row_height: f32,
    pub text_size: f32,
    pub separators: bool,
    pub chevron_size: f32,
    pub chevron_hit: f32,
    pub chevron_inset_x: f32,
    pub chevron_inset_y: f32,
    pub chevron_stroke: f32,
    pub chevron_idle_opacity: f32,
    pub chevron_hover_opacity: f32,
    pub chevron_hover_fill: f32,
    pub chevron_emboss: f32,
    /// `0` uses [`super::DockTokens::icon_size`].
    pub suggestion_size: f32,
    pub suggestion_gap_from_chevron: f32,
    pub suggestion_inset_x: f32,
}

impl Default for ReadoutTokens {
    fn default() -> Self {
        Self {
            pad_top: 3.0,
            pad_bottom: 3.0,
            row_gap: 4.0,
            item_gap: 8.0,
            row_height: 0.0,
            text_size: 12.0,
            separators: true,
            chevron_size: 4.0,
            chevron_hit: 8.0,
            chevron_inset_x: 2.5,
            chevron_inset_y: 1.5,
            chevron_stroke: 0.58,
            chevron_idle_opacity: 0.38,
            chevron_hover_opacity: 0.92,
            chevron_hover_fill: 0.10,
            chevron_emboss: 0.22,
            suggestion_size: 0.0,
            suggestion_gap_from_chevron: 10.0,
            suggestion_inset_x: 12.0,
        }
    }
}

impl ReadoutTokens {
    pub fn normalize(&mut self) {
        self.pad_top = self.pad_top.clamp(0.0, 24.0);
        self.pad_bottom = self.pad_bottom.clamp(0.0, 24.0);
        self.row_gap = self.row_gap.clamp(0.0, 24.0);
        self.item_gap = self.item_gap.clamp(0.0, 24.0);
        self.row_height = self.row_height.clamp(0.0, 48.0);
        self.text_size = self.text_size.clamp(7.0, 20.0);
        self.chevron_size = self.chevron_size.clamp(2.0, 16.0);
        self.chevron_hit = self.chevron_hit.clamp(6.0, 28.0);
        self.chevron_inset_x = self.chevron_inset_x.clamp(0.0, 24.0);
        self.chevron_inset_y = self.chevron_inset_y.clamp(0.0, 16.0);
        self.chevron_stroke = self.chevron_stroke.clamp(0.4, 2.4);
        self.chevron_idle_opacity = self.chevron_idle_opacity.clamp(0.08, 1.0);
        self.chevron_hover_opacity = self.chevron_hover_opacity.clamp(0.2, 1.0);
        self.chevron_hover_fill = self.chevron_hover_fill.clamp(0.0, 0.4);
        self.chevron_emboss = self.chevron_emboss.clamp(0.0, 0.6);
        self.suggestion_size = self.suggestion_size.clamp(0.0, 64.0);
        self.suggestion_gap_from_chevron = self.suggestion_gap_from_chevron.clamp(2.0, 32.0);
        self.suggestion_inset_x = self.suggestion_inset_x.clamp(0.0, 48.0);
    }

    pub fn round_for_storage(&mut self) {
        for value in [
            &mut self.pad_top,
            &mut self.pad_bottom,
            &mut self.row_gap,
            &mut self.item_gap,
            &mut self.row_height,
            &mut self.text_size,
            &mut self.chevron_size,
            &mut self.chevron_hit,
            &mut self.chevron_inset_x,
            &mut self.chevron_inset_y,
            &mut self.chevron_stroke,
            &mut self.chevron_idle_opacity,
            &mut self.chevron_hover_opacity,
            &mut self.chevron_hover_fill,
            &mut self.chevron_emboss,
            &mut self.suggestion_size,
            &mut self.suggestion_gap_from_chevron,
            &mut self.suggestion_inset_x,
        ] {
            *value = (*value * 1_000.0).round() / 1_000.0;
        }
    }
}
