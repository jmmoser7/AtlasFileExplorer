//! Chat-card metrics: handle dots, composer height, and fold.

use super::*;

/// Designed radius of a chat card's gray handle dots.
const HANDLE_DOT: f32 = 3.5;

/// The handle dot of a chat card: the output grip, the context and
/// changed-document handles, and the crosstalk ports. It scales with the
/// board (P0.9) and swells a little under the pointer.
pub(crate) fn paint_handle_dot(
    painter: &egui::Painter,
    at: Pos2,
    z: f32,
    near: bool,
    color: Color32,
) {
    let grow = if near { 1.12 } else { 1.0 };
    painter.circle_filled(at, canvas_scale::px(HANDLE_DOT, z) * grow, color);
}

pub(crate) fn paint_agent_spinner(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    time: f32,
    ink: Color32,
) {
    for i in 0..8 {
        let angle = time * 5.0 + i as f32 * std::f32::consts::TAU / 8.0;
        let p = center + egui::vec2(angle.cos(), angle.sin()) * radius;
        let alpha = 36 + i * 26;
        painter.circle_filled(p, radius * 0.18, ink.gamma_multiply(alpha as f32 / 255.0));
    }
}

/// A pill laid over agent media: `fill` plus the theme's hairline outline.
pub(crate) fn paint_overlay_pill(
    painter: &egui::Painter,
    rect: Rect,
    radius: f32,
    fill: Color32,
    ink: &OverlayInk,
    z: f32,
) {
    painter.rect_filled(rect, radius, fill);
    if ink.border.a() > 0 {
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(canvas_scale::px(ink.border_width, z), ink.border),
            egui::StrokeKind::Inside,
        );
    }
}

/// One line of chat-card type in world units — `size`, extra `tracking`, cut
/// with `…` past `max_w` — painted at board zoom `z`.
pub(super) fn tracked_label(
    ctx: &egui::Context,
    text: &str,
    size: f32,
    tracking: f32,
    max_w: f32,
    z: f32,
) -> canvas_text::Scaled {
    let spec = canvas_text::WorldSpec {
        tracking,
        break_anywhere: false,
        ..canvas_text::WorldSpec::label(FontId::proportional(size), max_w)
    };
    let layout = canvas_text::world_layout_spec(ctx, text, &spec);
    canvas_text::world_text(ctx, &layout, z)
}

/// Title band above the message being typed, in world units.
pub(super) const COMPOSER_TOP: f32 = 32.0;
/// Padding under the message, in world units.
pub(super) const COMPOSER_BOTTOM: f32 = 14.0;
/// Gap between a transcript and the composer, in world units.
pub(super) const COMPOSER_GAP: f32 = 8.0;
/// Extra room so a measured transcript is not clipped by spacing.
const TRANSCRIPT_SLACK: f32 = 4.0;
/// About 1.3 lines under the last line of a sent message.
pub(super) const CARD_TEXT_PAD: f32 = 18.0;
/// Top of a sent card's text, in world units.
pub(super) const SUMMARY_TEXT_TOP: f32 = 28.0;
/// Lines a collapsed card keeps.
pub(super) const COLLAPSED_ROWS: usize = 3;
/// Card text size, in world units.
const CARD_TEXT_PX: f32 = 13.0;
/// Stop's bare square (the output circle's diameter) and press reach on a
/// streaming card's output circle, in world units.
pub(super) const STOP_SIDE: f32 = 7.0;
pub(super) const STOP_REACH: f32 = 8.0;
/// Width a chooser list always keeps for its scroll bar, in world units, so
/// the bar appearing never narrows the rows.
pub(super) const PICK_BAR_RESERVE: f32 = 8.0;

/// Chooser columns, fixed by row count so rows never restack: the project
/// list is one column; conversations take one up to 3 rows, two up to 8,
/// otherwise three.
pub(super) fn agent_pick_columns(projects: bool, rows: usize) -> usize {
    match rows {
        _ if projects => 1,
        0..=3 => 1,
        4..=8 => 2,
        _ => 3,
    }
}

/// On-screen center of a chat card's top output circle.
pub(super) fn output_circle_center(card: Rect, z: f32) -> Pos2 {
    card.right_top()
        + egui::vec2(
            -slate_doc::agent_chat::PORT_INSET,
            slate_doc::agent_chat::RAIL_INSET,
        ) * z
}

/// A person's resize of a chat card is authored size, recorded in the same
/// patch as the rect. It also opens a collapsed card. Drafts and bundles still fit.
pub(crate) fn record_agent_resize(node: &mut Node) -> bool {
    let size = [node.rect.w, node.rect.h];
    let NodeKind::Portal(p) = &mut node.kind else {
        return false;
    };
    let Some(a) = p.agent.as_mut() else {
        return false;
    };
    if a.provider.is_empty()
        || a.view != atlas_ai::agent::PortalView::Chat
        || a.chat.draft
        || (a.chat.train && !a.chat.bundled.is_empty())
    {
        return false;
    }
    a.chat.size = Some(size);
    a.chat.collapsed = false;
    a.chat.partial = false;
    true
}

/// `streaming` is the display-only open of a collapsed card ([`AgentRuntime::stream_open`]).
pub(super) fn card_fold(
    chat: &slate_doc::agent_chat::ChatView,
    streaming: bool,
) -> train_ux::CardFold {
    if chat.collapsed && streaming {
        train_ux::CardFold::Partial
    } else if chat.collapsed {
        train_ux::CardFold::Collapsed
    } else if chat.partial {
        train_ux::CardFold::Partial
    } else {
        train_ux::CardFold::Open
    }
}

pub(super) fn paint_pick_button(
    ui: &egui::Ui,
    rect: Rect,
    label: &str,
    z: f32,
    radius: f32,
    id: Id,
    palette: &atlas_shell::theme::Palette,
) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    let fill = if resp.hovered() {
        palette.link_hover
    } else {
        palette.link
    };
    ui.painter().rect_filled(rect, radius, fill);
    let on_link = if palette.dark_mode {
        palette.ink
    } else {
        palette.window
    };
    let laid = tracked_label(ui.ctx(), label, 14.0, 0.15, rect.width() / z - 24.0, z);
    laid.paint_anchored(ui.painter(), rect.center(), Align2::CENTER_CENTER, on_link);
    resp.clicked()
}

/// "Responding" and its token readout, shaped in world units once per count,
/// so the wave repaints and the board zooms without shaping or allocating.
#[derive(Default)]
pub(crate) struct RespondingLabel {
    pub(super) wave: Option<std::sync::Arc<canvas_text::WorldLayout>>,
    pub(super) count_key: Option<(Option<u64>, usize)>,
    pub(super) count: Option<std::sync::Arc<canvas_text::WorldLayout>>,
}

/// The opacity wave runs glyph by glyph over one cached galley: each glyph
/// is the same galley clipped to its advance, tinted by its phase. `size` is
/// the world type size; `z` the board zoom.
pub(super) fn paint_responding(
    ui: &mut egui::Ui,
    label: &mut RespondingLabel,
    (size, z): (f32, f32),
    colors: (Color32, Color32),
    reported: Option<u64>,
    turns: &[AgentTurn],
) {
    let (accent, sub) = colors;
    let ctx = ui.ctx().clone();
    let font = FontId::proportional(size);
    let world = |text: &str| {
        canvas_text::world_layout(&ctx, text, font.clone(), f32::INFINITY, egui::Align::LEFT)
    };
    if label.wave.as_ref().is_none_or(|wave| wave.font != font) {
        label.wave = Some(world("Responding"));
        label.count_key = None;
    }
    // Byte length moves whenever text arrives; characters are counted only then.
    let bytes: usize = turns.iter().map(|t| t.text.len()).sum();
    if label.count_key != Some((reported, bytes)) {
        label.count_key = Some((reported, bytes));
        let chars = turns.iter().map(|t| t.text.chars().count()).sum();
        label.count = Some(world(&train_ux::token_readout(reported, chars)));
    }
    let (Some(wave), Some(count)) = (&label.wave, &label.count) else {
        return;
    };
    let wave = canvas_text::world_text(&ctx, wave, z);
    let count = canvas_text::world_text(&ctx, count, z);
    let gap = ui.spacing().item_spacing.x;
    let size = egui::vec2(
        wave.size().x + gap + count.size().x,
        wave.size().y.max(count.size().y),
    );
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    let time = ui.input(|i| i.time) as f32;
    let (galley, scale) = (wave.galley(), wave.scale());
    if let Some(row) = galley.rows.first() {
        for (i, glyph) in row.glyphs.iter().enumerate() {
            let phase = ((time * 2.2 - i as f32 * 0.35).sin() + 1.0) * 0.5;
            let left = rect.left() + glyph.pos.x * scale;
            let right = left + glyph.advance_width * scale;
            let clip =
                Rect::from_x_y_ranges(left..=right, rect.y_range()).intersect(painter.clip_rect());
            canvas_text::Scaled::from_galley(galley.clone(), scale).paint(
                &painter.with_clip_rect(clip),
                rect.min,
                accent.gamma_multiply(0.35 + 0.65 * phase),
            );
        }
    }
    count.paint(
        painter,
        Pos2::new(rect.left() + wave.size().x + gap, rect.top()),
        sub,
    );
}

pub(super) fn paint_chevron_glyph(
    painter: &egui::Painter,
    center: Pos2,
    z: f32,
    up: bool,
    ink: Color32,
) {
    let half = canvas_scale::px(3.0, z);
    let rise = canvas_scale::px(1.6, z) * if up { -1.0 } else { 1.0 };
    let stroke = egui::Stroke::new(canvas_scale::px(1.3, z), ink);
    painter.line_segment(
        [
            center + egui::vec2(-half, -rise),
            center + egui::vec2(0.0, rise),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + egui::vec2(0.0, rise),
            center + egui::vec2(half, -rise),
        ],
        stroke,
    );
}

pub(super) fn write_fold(chat: &mut slate_doc::agent_chat::ChatView, fold: train_ux::CardFold) {
    match fold {
        train_ux::CardFold::Collapsed => {
            chat.collapsed = true;
            chat.partial = false;
        }
        train_ux::CardFold::Partial => {
            chat.collapsed = false;
            chat.partial = true;
        }
        train_ux::CardFold::Open => {
            chat.collapsed = false;
            chat.partial = false;
            chat.size = None;
        }
    }
}

/// The presentation rows of a chat card's ellipsis menu: command, and
/// whether it is offered.
pub(crate) fn agent_presentations(
    chat: &slate_doc::agent_chat::ChatView,
    running: bool,
) -> [(&'static str, bool); 3] {
    use slate_doc::agent_chat::Detail;
    let train = chat.train;
    [
        ("portal.agent.chat", train && !running),
        (
            "portal.agent.train",
            (!train || chat.detail == Detail::Pair) && !running,
        ),
        (
            "portal.agent.pairs",
            (!train || chat.detail != Detail::Pair) && !running,
        ),
    ]
}

/// Bundling a run keeps it on the terminal card, one level deeper; the rest
/// of the run hides (D25).
pub(super) fn bundle_view(node: &mut Node, run: &[NodeId]) {
    let Some((last, members)) = run.split_last() else {
        return;
    };
    if node.id != *last {
        node.hidden = true;
        return;
    }
    if let NodeKind::Portal(p) = &mut node.kind {
        if let Some(a) = &mut p.agent {
            a.chat.bundle_layers.push(a.chat.bundled.clone());
            a.chat.bundled = members.to_vec();
        }
    }
}

/// One line naming a wired context source.
pub(super) fn context_label(n: &Node) -> String {
    match &n.kind {
        NodeKind::Text(t) => {
            let line = t.text.lines().next().unwrap_or("").trim();
            if line.is_empty() {
                "Text".into()
            } else {
                line.chars().take(64).collect()
            }
        }
        NodeKind::Portal(p) => p.title.clone(),
        NodeKind::Image(_) => "Image".into(),
        _ => "Linked context".into(),
    }
}

/// Header, the first lines of `text` at the card's wrap, and the text pad.
/// Measured with the same world layout `paint_agent_summary` paints, so the
/// card fits the rows it shows.
pub(super) fn collapsed_card_height(ctx: &egui::Context, text: String, card_w: f32) -> f32 {
    let lines = canvas_text::world_layout_rows(
        ctx,
        &text,
        FontId::proportional(CARD_TEXT_PX),
        (card_w - 24.0).max(1.0),
        egui::Align::LEFT,
        COLLAPSED_ROWS,
    )
    .height;
    COMPOSER_TOP + lines + CARD_TEXT_PAD
}

pub(super) fn composer_wrap(card_w: f32) -> f32 {
    (card_w - slate_doc::agent_chat::PORT_INSET - 10.0 - 12.0).max(1.0)
}

/// Wrapped height of the text being typed. An empty draft is one line.
pub(super) fn composer_text_height(
    ctx: &egui::Context,
    text: &str,
    wrap: f32,
    font_px: f32,
) -> f32 {
    let font = FontId::proportional(font_px);
    ctx.fonts(|fonts| {
        let row = fonts.row_height(&font);
        if text.is_empty() {
            row
        } else {
            fonts
                .layout(text.to_owned(), font, Color32::TRANSPARENT, wrap.max(1.0))
                .size()
                .y
                .max(row)
        }
    })
}

pub(super) fn hugging_composer_card(prompt_h: f32) -> f32 {
    (COMPOSER_TOP + prompt_h + COMPOSER_BOTTOM).max(slate_doc::agent_chat::DRAFT_HEIGHT)
}

pub(super) fn conversation_card_height(transcript_h: f32, prompt_h: f32) -> f32 {
    if transcript_h <= 1.0 {
        hugging_composer_card(prompt_h)
    } else {
        COMPOSER_TOP + transcript_h + TRANSCRIPT_SLACK + COMPOSER_GAP + prompt_h + COMPOSER_BOTTOM
    }
}

/// What [`SlateApp::build_artifact_node`] made.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ArtifactBuild {
    Node(Node),
    /// More than one face and none chosen: the caller asks.
    Ask,
    Unavailable,
}

/// `[portal, artifact]`, or `{"portal","artifact","face"}` where `face` is a
/// `return.json` `as` value (`PreviewFace` ids are the same words).
pub(super) fn parse_artifact_open(detail: &str) -> Option<(u64, String, atlas_agent::Face)> {
    let value: serde_json::Value = serde_json::from_str(detail).ok()?;
    if let Some(pair) = value.as_array() {
        let id = pair.first()?.as_u64()?;
        let artifact = pair.get(1)?.as_str()?.to_string();
        return Some((id, artifact, atlas_agent::Face::Auto));
    }
    let obj = value.as_object()?;
    let id = obj.get("portal")?.as_u64()?;
    let artifact = obj.get("artifact")?.as_str()?.to_string();
    let face = obj
        .get("face")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    Some((id, artifact, face))
}
