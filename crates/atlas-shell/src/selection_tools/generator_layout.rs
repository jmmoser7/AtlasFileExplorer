//! One measured world-space composition for attached generator content.
use super::*;

pub const INSET: f32 = 8.0;
pub const GAP: f32 = 2.0;
pub const HEADER_HEIGHT: f32 = 28.0;
pub const DIALOGUE_INSET: f32 = 16.0;
pub const DIALOGUE_GAP: f32 = 20.0;

/// One compact header datum, shared by images and every artifact generator.
#[derive(Clone, Copy, Debug)]
pub(in crate::selection_tools) struct DatumLayout {
    pub model: Rect,
    pub settings: Option<Rect>,
    pub count: Rect,
    pub play: Rect,
    pub history: Rect,
    pub feedback: Rect,
    pub fold: Rect,
    pub bottom: f32,
}

pub(in crate::selection_tools) fn datum_layout(
    ctx: &egui::Context,
    inner: Rect,
    zoom: f32,
    model_label: &str,
    _settings_label: Option<&str>,
) -> DatumLayout {
    let z = zoom.max(f32::EPSILON);
    let top = inner.top();
    let row_h = 24.0 * z;
    let play = Rect::from_min_size(
        Pos2::new(inner.right() - 20.0 * z, top + 2.0 * z),
        Vec2::splat(20.0 * z),
    );
    let count = Rect::from_min_size(
        Pos2::new(play.left() - 36.0 * z, top),
        Vec2::new(32.0 * z, row_h),
    );
    let fold = Rect::from_min_size(
        Pos2::new(count.left() - 22.0 * z, top + 2.4 * z),
        Vec2::new(14.0 * z, 11.2 * z),
    );
    let text = canvas_text::world_layout(
        ctx,
        model_label,
        canvas_scale::font(11.0, 1.0),
        f32::INFINITY,
        egui::Align::LEFT,
    );
    let available = ((fold.left() - inner.left()) / z - 21.0).max(0.0);
    let model_width = (text.width + 20.0).clamp(36.0, 120.0).min(available) * z;
    let model = Rect::from_min_size(inner.min, Vec2::new(model_width, row_h));
    let settings = Rect::from_min_size(
        Pos2::new(model.right() + z, top),
        Vec2::new(14.0 * z, row_h),
    );
    let feedback = Rect::from_min_size(
        Pos2::new(settings.right() + 6.0 * z, top + 6.0 * z),
        Vec2::splat(10.0 * z),
    );
    let history_left = (settings.right() + 24.0 * z).min(fold.left() - 6.0 * z);
    let history = Rect::from_min_max(
        Pos2::new(history_left, top),
        Pos2::new((fold.left() - 6.0 * z).max(history_left), top + row_h),
    );
    DatumLayout {
        model,
        settings: Some(settings),
        count,
        play,
        history,
        feedback,
        fold,
        bottom: top + row_h,
    }
}

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
                .map(|view| (view.entries, view.state.fold)),
        }
    }
    pub(in crate::selection_tools) fn image_history(mut self) -> Self {
        if let Some((_, fold)) = &mut self.activity {
            *fold = activity::CardFold::Partial;
        }
        self
    }
}

/// Rectangles and height share the same measurement, before the camera transform.
#[derive(Clone, Copy, Debug)]
pub struct GeneratorLayout {
    pub height: f32,
    pub(in crate::selection_tools) history: Option<Rect>,
    pub(super) summary: Option<Rect>,
    pub(super) feedback: Rect,
    pub(super) fold: Rect,
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
        datum: DatumLayout,
        content: GeneratorContent<'_>,
    ) -> Self {
        let dialogue = content.activity.is_some();
        let left = if dialogue { DIALOGUE_INSET } else { INSET };
        let inner_width = (width - left * 2.0).max(1.0);
        let mut y = if dialogue {
            HEADER_HEIGHT + 8.0
        } else {
            datum.bottom + GAP
        };
        // The history always remains on the datum. Its own renderer paginates.
        let history =
            (content.history_count > 0 && datum.history.width() > 0.0).then_some(datum.history);
        let summary = content
            .status
            .filter(|_| content.activity.is_none())
            .map(|status| {
                let text = canvas_text::world_layout(
                    ctx,
                    status,
                    canvas_scale::font(11.0, 1.0),
                    inner_width,
                    egui::Align::LEFT,
                );
                let r = Rect::from_min_size(
                    Pos2::new(left, y),
                    Vec2::new(inner_width, text.height.max(16.0)),
                );
                y = r.bottom() + GAP;
                r
            });
        let approval = content.approval.map(|text| {
            let r = Rect::from_min_size(
                Pos2::new(left, y),
                Vec2::new(inner_width, approval::height(ctx, text, inner_width)),
            );
            y = r.bottom() + GAP;
            r
        });
        let activity = content.activity.and_then(|(entries, fold)| {
            let h = activity::height(ctx, entries, inner_width, fold);
            if h <= 0.0 {
                return None;
            }
            let r = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner_width, h));
            y = r.bottom() + DIALOGUE_GAP;
            Some(r)
        });
        let prompt_height = if dialogue {
            (content.prompt_lines.clamp(1, 5) as f32 * 20.0).max(20.0)
        } else {
            (16.0 + content.prompt_lines.clamp(1, 8) as f32 * 16.0).max(GENERATOR_PROMPT_MIN_HEIGHT)
        };
        let prompt = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner_width, prompt_height));
        Self {
            height: prompt.bottom() + if dialogue { 18.0 } else { INSET },
            history,
            summary,
            feedback: datum.feedback,
            fold: datum.fold,
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
            feedback: transform * self.feedback,
            fold: transform * self.fold,
            approval: self.approval.map(|r| transform * r),
            activity: self.activity.map(|r| transform * r),
            prompt,
        }
    }
}

pub(in crate::selection_tools) fn inner(width: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(INSET, 2.0),
        Vec2::new((width - INSET * 2.0).max(1.0), 0.0),
    )
}

#[cfg(test)]
mod image_dialogue_tests {
    use super::*;
    #[test]
    fn empty_image_dialogue_has_interactive_composer_without_invented_transcript() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let entries: [activity::ActivityEntry<'_>; 0] = [];
            let mut state = activity::ActivityState::default();
            let chrome = GeneratorChrome {
                activity: Some(activity::ActivityView {
                    entries: &entries,
                    state: &mut state,
                    running: false,
                }),
                ..Default::default()
            };
            let layout = crate::selection_tools::image_agent_layout(
                ctx,
                420.0,
                GeneratorContent::new(1, 0, None, None, &chrome),
            );
            assert_eq!(layout.prompt.height(), 20.0);
            assert!(layout.activity.is_none());
            let plain = crate::selection_tools::image_agent_layout(
                ctx,
                420.0,
                GeneratorContent::new(1, 0, None, None, &GeneratorChrome::default()),
            );
            assert_eq!(plain.prompt.height(), 32.0);
        });
    }
    #[test]
    fn image_history_is_visible_and_capped220_without_changing_shared_fold_state() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let text = "Actual public message. ".repeat(200);
            let entries = [activity::ActivityEntry {
                id: "real-request",
                label: "You",
                text: &text,
            }];
            let mut state = activity::ActivityState::default();
            let chrome = GeneratorChrome {
                activity: Some(activity::ActivityView {
                    entries: &entries,
                    state: &mut state,
                    running: false,
                }),
                ..Default::default()
            };
            let layout = crate::selection_tools::image_agent_layout(
                ctx,
                420.0,
                GeneratorContent::new(1, 0, None, None, &chrome),
            );
            assert_eq!(layout.activity.unwrap().height(), 220.0);
            assert_eq!(state.fold, activity::CardFold::Collapsed);
        });
    }
}
