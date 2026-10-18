//! Native fixture using the production Tags family, inline body, and popup host.
use super::capture::Capture;
use atlas_shell::{
    canvas_text,
    dock::{self, DockIcon, DockItem, DockItemKind, DockSide},
    selection_tools::{self as chrome, TagEditorAction, TagEditorState},
    theme::Palette,
};
use eframe::egui;

#[derive(Default)]
struct TagReview {
    open: bool,
    editor: TagEditorState,
    tags: Vec<ReviewTag>,
    capture: Capture,
    dark: bool,
    settle_started: Option<f64>,
    last_body: Option<egui::Rect>,
    stable_frames: usize,
    narrow: bool,
}

struct ReviewTag {
    name: String,
    color: [u8; 3],
    selected: bool,
    visible: bool,
}

impl TagReview {
    fn from_args(args: &[String]) -> Self {
        let case = args.iter().find_map(|arg| arg.strip_prefix("--tags-case="));
        let names: &[&str] = match case {
            Some("single") => &["Coral review"],
            Some("several" | "selected") => &["Coral review", "Final review", "Approved"],
            Some("long" | "long-narrow") => &[
                "Architectural material references awaiting review",
                "QA",
                "Final",
            ],
            _ => &[],
        };
        Self {
            open: case == Some("open"),
            capture: Capture::from_args(args),
            narrow: case == Some("long-narrow"),
            tags: names
                .iter()
                .enumerate()
                .map(|(index, name)| ReviewTag {
                    name: (*name).into(),
                    color: if index == 2 {
                        [25, 130, 128]
                    } else {
                        [254, 87, 90]
                    },
                    selected: case == Some("selected") && index < 2,
                    visible: true,
                })
                .collect(),
            ..Default::default()
        }
    }
}

impl eframe::App for TagReview {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let theme = Palette::for_mode(self.dark);
        ctx.set_visuals(theme.visuals());
        if self.capture.is_active() {
            dock::set_panel_open(ctx, "tag-review-dock", "tool.tags", true);
        }
        let canvas = egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg))
            .show(ctx, |ui| ui.max_rect())
            .inner;
        let items = [DockItem {
            id: "tool.tags",
            label: "Tags",
            description: "Show or hide tagged objects; Shift/Ctrl-click circles selects a batch.",
            icon: DockIcon::Tags,
            kind: DockItemKind::Tool,
            active: false,
            visible: true,
            gap_before: false,
        }];
        let mut body_rect = None;
        dock::floating_dock(
            ctx,
            "tag-review-dock",
            canvas,
            &theme,
            DockSide::BottomCenter,
            &items,
            &["tool.tags".into()],
            &["*".into()],
            &[],
            &[],
            false,
            |ui, _| {
                let diameter = dock::flyout_icon_size(&atlas_shell::tokens::current().dock);
                let add = chrome::tag_add_button(ui, self.tags.is_empty(), diameter, theme);
                if add.clicked() {
                    self.open = !self.open;
                    if self.open {
                        self.editor.begin();
                    }
                }
                if !self.tags.is_empty() {
                    let tags: Vec<_> = self
                        .tags
                        .iter()
                        .map(|tag| chrome::TagPaletteItem {
                            label: &tag.name,
                            color: tag.color,
                            selected: tag.selected,
                            visible: tag.visible,
                        })
                        .collect();
                    let response = chrome::tag_palette(ui, &tags, diameter, theme);
                    // Local fixture state exercises presentation; Slate owns workbook commands.
                    if let Some((index, visible)) = response.visibility {
                        self.tags[index].visible = visible;
                    }
                    if let Some(index) = response.selection {
                        self.tags[index].selected = !self.tags[index].selected;
                    }
                }
                if self.open {
                    match chrome::tag_editor_popup(
                        ui,
                        egui::Id::new("tag-review-editor"),
                        add.rect,
                        &mut self.editor,
                        theme,
                    ) {
                        TagEditorAction::None => {}
                        TagEditorAction::Cancel => self.open = false,
                        TagEditorAction::Add => {
                            self.tags.push(ReviewTag {
                                name: self.editor.name.trim().to_owned(),
                                color: self.editor.color,
                                selected: false,
                                visible: true,
                            });
                            self.open = false;
                        }
                    }
                }
                body_rect = Some(ui.min_rect());
            },
        );
        if self.capture.is_active() {
            let now = ctx.input(|input| input.time);
            let started = *self.settle_started.get_or_insert(now);
            self.stable_frames = if body_rect.is_some() && body_rect == self.last_body {
                self.stable_frames + 1
            } else {
                0
            };
            self.last_body = body_rect;
            let reveal = atlas_shell::tokens::current().dock.inline.open_duration as f64;
            let ready = dock::panel_is_open(ctx, "tag-review-dock", "tool.tags")
                && now - started >= reveal
                && self.stable_frames >= 3;
            self.capture.update(ctx, &mut self.dark, ready);
        }
    }
}

pub(super) fn run() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    let review = TagReview::from_args(&args);
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([if review.narrow { 420.0 } else { 1000.0 }, 600.0]);
    if review.capture.is_active() {
        viewport = viewport.with_position([-4000.0, -4000.0]);
    }
    eframe::run_native(
        "Slate tag UI review",
        eframe::NativeOptions {
            viewport,
            renderer: eframe::Renderer::Glow,
            multisampling: atlas_shell::NATIVE_MSAA_SAMPLES,
            ..Default::default()
        },
        Box::new(move |cc| {
            canvas_text::install(&cc.egui_ctx);
            Ok(Box::new(review))
        }),
    )
}
