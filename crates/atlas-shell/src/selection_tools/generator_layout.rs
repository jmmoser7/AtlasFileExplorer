//! One measured world-space composition for attached generator content.
use super::*;

pub const INSET: f32 = 12.0;
pub const GAP: f32 = 6.0;
const SUMMARY_HEIGHT: f32 = 24.0;

/// Visible content only; lifecycle and permission decisions remain with the app.
pub struct GeneratorContent<'a> {
    pub prompt_lines: usize,
    pub history_count: usize,
    pub status: Option<&'a str>,
    pub approval: Option<&'a str>,
    activity: Option<(&'a dyn activity::ActivityEntries, activity::CardFold)>,
}

impl<'a> GeneratorContent<'a> {
    pub fn new(
        prompt_lines: usize,
        history_count: usize,
        status: Option<&'a str>,
        approval: Option<&'a str>,
        chrome: &'a GeneratorChrome<'_>,
    ) -> Self {
        Self {
            prompt_lines,
            history_count,
            status: status.filter(|text| !text.trim().is_empty()),
            approval,
            activity: chrome
                .activity
                .as_ref()
                .filter(|view| !view.entries.is_empty())
                .map(|view| (view.entries, view.state.fold)),
        }
    }
}

/// Rectangles and height share the same measurement, before the camera transform.
#[derive(Clone, Copy, Debug)]
pub struct GeneratorLayout {
    pub height: f32,
    pub(super) history: Option<Rect>,
    pub(super) summary: Option<Rect>,
    pub(super) approval: Option<Rect>,
    pub(super) activity: Option<Rect>,
    pub(super) prompt: Rect,
}

impl GeneratorLayout {
    /// App adapters request textures only for chips the shared row will paint.
    pub fn visible_history_range(
        &self,
        count: usize,
        selected: Option<usize>,
    ) -> std::ops::Range<usize> {
        self.history
            .map_or(0..0, |row| history::visible_range(row, count, selected))
    }

    pub(in crate::selection_tools) fn measure(
        ctx: &egui::Context,
        width: f32,
        datum_bottom: f32,
        history_lane: Rect,
        content: GeneratorContent<'_>,
    ) -> Self {
        let left = INSET;
        let inner_width = (width - INSET * 2.0).max(1.0);
        let mut y = datum_bottom + GAP;
        let mut row = |height: f32| {
            let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner_width, height));
            y += height + GAP;
            rect
        };
        let history = if content.history_count == 0 {
            None
        } else if history::fits(history_lane.width(), content.history_count) {
            Some(history_lane)
        } else {
            Some(row(history::HEIGHT))
        };
        let summary =
            (content.status.is_some() || content.activity.is_some()).then(|| row(SUMMARY_HEIGHT));
        let approval = content
            .approval
            .map(|text| row(approval::height(ctx, text, inner_width)));
        let activity = content.activity.and_then(|(entries, fold)| {
            let height = activity::height(ctx, entries, inner_width, fold);
            (height > 0.0).then(|| row(height))
        });
        let prompt_height = (16.0 + content.prompt_lines.clamp(1, 8) as f32 * 16.0)
            .max(GENERATOR_PROMPT_MIN_HEIGHT);
        let prompt = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner_width, prompt_height));
        Self {
            height: prompt.bottom() + INSET,
            history,
            summary,
            approval,
            activity,
            prompt,
        }
    }

    pub(in crate::selection_tools) fn at(self, rect: Rect, zoom: f32) -> Self {
        let transform = egui::emath::TSTransform::new(rect.min.to_vec2(), zoom);
        let mut prompt = transform * self.prompt;
        // The caller may offer extra space (for example a manually enlarged host).
        // It belongs to the prompt, without changing its fixed world-width wrap.
        prompt.max.y = rect.bottom() - INSET * zoom;
        Self {
            height: self.height * zoom,
            history: self.history.map(|r| transform * r),
            summary: self.summary.map(|r| transform * r),
            approval: self.approval.map(|r| transform * r),
            activity: self.activity.map(|r| transform * r),
            prompt,
        }
    }
}

pub(in crate::selection_tools) fn inner(width: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(INSET, INSET),
        Vec2::new((width - INSET * 2.0).max(1.0), 0.0),
    )
}
