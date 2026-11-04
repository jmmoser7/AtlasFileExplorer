//! Frame selection chrome and host-attached presentation geometry.
use crate::{canvas_scale, canvas_text, icons::Icon, selection_tools as chrome, theme::Palette};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Id, Painter, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Fill,
    Stroke,
    Corners,
    Deck,
    Settings,
    Present,
}

pub const ACTIONS: [(Action, Icon, &str); 6] = [
    (Action::Fill, Icon::Fill, "Fill"),
    (Action::Stroke, Icon::Ellipse, "Stroke"),
    (Action::Corners, Icon::Corners, "Corners"),
    (Action::Deck, Icon::PresentationDeck, "Create Presentation"),
    (
        Action::Settings,
        Icon::Sliders,
        "Presentation media settings",
    ),
    (Action::Present, Icon::View, "Present from this slide"),
];

#[derive(Clone, Copy)]
pub struct Strip {
    pub rect: Rect,
    pub blister: Option<chrome::BlisterLayout>,
    zoom: f32,
}

impl Strip {
    pub fn new(outline: &[Pos2], zoom: f32, open: bool) -> Self {
        let host = Rect::from_points(outline);
        let count = ACTIONS.len();
        let blister = chrome::blister_fits_outline(outline, zoom, count).then(|| {
            let mode = if open {
                chrome::BlisterPresentation::Overlay
            } else {
                chrome::BlisterPresentation::Collapsed
            };
            chrome::blister_layout(host, zoom, count, mode)
        });
        let rect = blister.map_or_else(
            || {
                let points = outline.iter().map(|p| [p.x, p.y]).collect();
                let bottom =
                    chrome::strip_bottom(host, &[points], chrome::strip_size(count).x * zoom);
                chrome::strip_rect_below(bottom, count, zoom, 1.0)
            },
            |layout| layout.band,
        );
        Self {
            rect,
            blister,
            zoom,
        }
    }

    pub fn hit(self, index: usize) -> Rect {
        self.blister.map_or_else(
            || chrome::strip_button_rect(self.rect, index, self.zoom),
            |b| b.icon_rect(index).expand(5.0 * self.zoom),
        )
    }

    pub fn show(
        self,
        ui: &mut Ui,
        id: Id,
        active: Option<Action>,
        theme: Palette,
    ) -> Option<Action> {
        let mut action = None;
        for (i, (kind, icon, label)) in ACTIONS.into_iter().enumerate() {
            let rect = self.hit(i);
            let on = active == Some(kind);
            let response = if let Some(b) = self.blister {
                let r = ui
                    .interact(rect, id.with(i), Sense::click())
                    .on_hover_text(label);
                let color = if on || r.hovered() {
                    theme.select_stroke
                } else {
                    theme.ink
                };
                b.paint_icon(ui.painter(), i, icon, color, theme);
                r
            } else {
                chrome::button(
                    ui,
                    rect,
                    id.with(i),
                    label,
                    icon,
                    on,
                    self.zoom,
                    theme,
                    1.0,
                    true,
                )
            };
            if response.clicked() {
                action = Some(kind);
            }
        }
        action
    }
}

/// The same fill primitive used by Slate's Frame node painter.
pub fn paint_plate(painter: &Painter, points: Vec<Pos2>, fill: Color32) {
    painter.add(Shape::convex_polygon(points, fill, Stroke::NONE));
}

pub fn paint_notched_plate(painter: &Painter, paths: &[Vec<Pos2>], fill: Color32, ring: Stroke) {
    let paths_world: Vec<_> = paths
        .iter()
        .map(|line| line.iter().map(|p| [p.x, p.y]).collect())
        .collect();
    let (points, indices) = vector_ink::fill_triangles(&paths_world);
    let mut mesh = eframe::egui::Mesh::default();
    for [x, y] in points {
        mesh.colored_vertex(pos2(x, y), fill);
    }
    mesh.indices = indices;
    painter.add(Shape::mesh(mesh));
    for line in paths {
        painter.add(Shape::closed_line(line.clone(), ring));
    }
}

pub struct Drawer {
    pub rect: Rect,
    pub bar: Rect,
    pub selector: Rect,
    pub membership: Rect,
    pub sort: Rect,
    pub tiles: Vec<Rect>,
    pub columns: usize,
    pub inline: bool,
    pub attached: bool,
}

impl Drawer {
    pub fn new(host: Rect, strip: Strip, viewport: Rect, count: usize) -> Self {
        let z = strip.zoom;
        let attached = strip.blister.is_some();
        let width = if attached {
            host.width()
        } else {
            host.width().min(viewport.width() - 16.0 * z).max(460.0 * z)
        };
        let mut x = host.center().x - width / 2.0;
        if !attached && width <= viewport.width() - 16.0 * z {
            x = x.clamp(
                viewport.left() + 8.0 * z,
                viewport.right() - width - 8.0 * z,
            );
        }
        let y = if attached {
            host.bottom()
        } else {
            strip.rect.bottom().max(host.bottom()) + 8.0 * z
        };
        let bar = Rect::from_min_size(pos2(x, y), vec2(width, 46.0 * z));
        let inline = 420.0 + 40.0 * count as f32 <= width / z;
        let columns = if inline {
            count.max(1)
        } else {
            count
                .clamp(1, 10)
                .min(((width / z - 12.0) / 22.0).floor().max(1.0) as usize)
        };
        let rows = count.div_ceil(columns);
        let size = if inline {
            34.0
        } else {
            ((width / z - 18.0 - 6.0 * (columns - 1) as f32) / columns as f32)
                .floor()
                .min(((viewport.height() / z - 65.0) / rows.max(1) as f32).floor() - 9.0)
                .clamp(13.0, 50.0)
        };
        let selector = Rect::from_min_size(bar.min + vec2(9.0, 10.0) * z, vec2(138.0, 26.0) * z);
        let membership =
            Rect::from_min_size(selector.min + vec2(144.0 * z, 0.0), vec2(158.0, 26.0) * z);
        let sort = Rect::from_min_size(
            pos2(bar.right() - 122.0 * z, selector.top()),
            vec2(113.0, 26.0) * z,
        );
        let mut tiles = Vec::with_capacity(count);
        for i in 0..count {
            let row = i / columns;
            let cell = i % columns;
            let items = (count - row * columns).min(columns) as f32;
            let left = if inline {
                membership.right() + 6.0 * z
            } else {
                bar.center().x - (items * (size + 6.0) - 6.0) * z / 2.0
            };
            let top = if inline {
                bar.center().y - size * z / 2.0
            } else {
                bar.bottom() + (4.0 + row as f32 * (size + 9.0)) * z
            };
            tiles.push(Rect::from_min_size(
                pos2(
                    left + cell as f32 * (size + if inline { 5.0 } else { 6.0 }) * z,
                    top,
                ),
                vec2(size, size) * z,
            ));
        }
        let height = if inline {
            bar.height()
        } else {
            bar.height() + (rows as f32 * (size + 9.0) + 5.0) * z
        };
        let rect = Rect::from_min_size(bar.min, vec2(width, height));
        Self {
            rect,
            bar,
            selector,
            membership,
            sort,
            tiles,
            columns,
            inline,
            attached,
        }
    }
}

pub fn paint_attached_host(p: &Painter, host: Rect, z: f32, fill: Color32, ring: Color32) {
    let top = (8.0 * z).round() as u8;
    let corners = CornerRadius {
        nw: top,
        ne: top,
        sw: 0,
        se: 0,
    };
    p.rect_filled(host, corners, fill);
    let rail = Rect::from_min_max(host.min, host.max + vec2(0.0, 4.0 * z));
    let clip = Rect::from_min_max(
        p.clip_rect().min,
        pos2(p.clip_rect().right(), host.bottom()),
    );
    p.with_clip_rect(clip).rect_stroke(
        rail,
        corners,
        Stroke::new(2.0 * z, ring),
        StrokeKind::Middle,
    );
}

/// Paint the drawer in the chrome pass, after every canvas object.
pub fn paint_attachment(p: &Painter, drawer: &Drawer, z: f32, theme: Palette, ring: Color32) {
    let r = (12.0 * z).round() as u8;
    if drawer.attached {
        let corners = CornerRadius {
            nw: 0,
            ne: 0,
            sw: r,
            se: r,
        };
        p.rect_filled(drawer.rect, corners, theme.card);
        let rail = Rect::from_min_max(drawer.rect.min - vec2(0.0, 4.0 * z), drawer.rect.max);
        let clip = Rect::from_min_max(
            pos2(p.clip_rect().left(), drawer.rect.top()),
            p.clip_rect().max,
        );
        p.with_clip_rect(clip).rect_stroke(
            rail,
            corners,
            Stroke::new(2.0 * z, ring),
            StrokeKind::Middle,
        );
    } else {
        p.rect(
            drawer.rect,
            r,
            theme.card,
            Stroke::new(z, theme.border),
            StrokeKind::Inside,
        );
    }
}

pub fn control(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    label: &str,
    z: f32,
    theme: Palette,
    on: bool,
) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    p.rect(
        rect,
        (7.0 * z).round() as u8,
        if response.hovered() {
            theme.card_hover
        } else {
            theme.card
        },
        Stroke::new(z, if on { theme.accent } else { theme.border }),
        StrokeKind::Inside,
    );
    canvas_text::text(
        p,
        pos2(rect.left() + 9.0 * z, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        canvas_scale::font(12.0, z),
        theme.ink,
    );
    response
}

pub fn chevron(p: &Painter, at: Pos2, z: f32, color: Color32) {
    p.add(Shape::line(
        vec![
            at + vec2(-4.0, -2.0) * z,
            at + vec2(0.0, 2.0) * z,
            at + vec2(4.0, -2.0) * z,
        ],
        Stroke::new(1.8 * z, color),
    ));
}
