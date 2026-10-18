//! The compact hat picker embeds the shared album owner. Its only background
//! work turns bounded embedded SVG data into cards; no source files are read.
use super::art;
use crate::{
    home::{self, AlbumImage},
    theme::Palette,
};
use eframe::egui::{self, Align2, Color32, Id, Mesh, Painter, Rect, TextureHandle, Ui, Vec2};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{mpsc, Arc, Mutex},
};

struct Loading {
    rx: mpsc::Receiver<(usize, egui::ColorImage)>,
    textures: HashMap<usize, TextureHandle>,
}
fn textures(ctx: &egui::Context) -> Vec<Option<egui::TextureId>> {
    let key = Id::new("presence-hat-textures");
    let shared=ctx.data_mut(|data|data.get_temp_mut_or_insert_with(key,||{
        let (tx,rx)=mpsc::channel();let context=ctx.clone();
        std::thread::spawn(move||{
            for (index,id) in art::master().hat_order.iter().enumerate() {
                let hat=&art::master().hats[id];
                let artwork=if hat.svg.is_empty(){"<text x=\"50\" y=\"55\" text-anchor=\"middle\" font-family=\"Segoe UI,sans-serif\" font-size=\"12\" fill=\"#9aa1a8\">No hat</text>".to_owned()}else{format!("<g transform=\"translate(10 10) scale(.8)\">{}</g>",hat.svg)};
                let svg=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\" viewBox=\"0 0 100 100\"><rect width=\"100\" height=\"100\" rx=\"13\" fill=\"#f1f2f0\"/>{artwork}</svg>");
                if let Some((w,h,bytes))=atlas_core::svg::rasterize_bytes(svg.as_bytes(),200) {
                    if tx.send((index,egui::ColorImage::from_rgba_unmultiplied([w as usize,h as usize],&bytes))).is_err(){return;}
                    context.request_repaint();
                }
            }
        });
        Arc::new(Mutex::new(Loading{rx,textures:HashMap::new()}))
    }).clone());
    let Ok(mut shared) = shared.lock() else {
        return vec![None; art::master().hat_order.len()];
    };
    for _ in 0..2 {
        let Ok((index, image)) = shared.rx.try_recv() else {
            break;
        };
        let texture = ctx.load_texture(
            format!("presence-hat-card-{index}"),
            image,
            home::COVER_TEXTURE_OPTIONS,
        );
        shared.textures.insert(index, texture);
        ctx.request_repaint();
    }
    (0..art::master().hat_order.len())
        .map(|n| shared.textures.get(&n).map(TextureHandle::id))
        .collect()
}
pub(super) fn picker(ui: &mut Ui, palette: &Palette, selected: &str, id: Id) -> Option<String> {
    let ids = &art::master().hat_order;
    let focus = ids.iter().position(|id| id == selected).unwrap_or(0);
    let textures = textures(ui.ctx());
    let images: Vec<_> = textures
        .iter()
        .map(|texture| AlbumImage {
            texture: *texture,
            size: Vec2::splat(200.0),
            enabled: true,
        })
        .collect();
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 88.0), egui::Sense::hover());
    let next = home::image_album_picker(ui, id, rect, &images, focus, Vec2::splat(78.0));
    let label = art::master().hats[&ids[next]].label.as_str();
    let (name, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), egui::Sense::hover());
    super::ui::label(ui, name, label, 11.0, palette.sub, Align2::CENTER_CENTER);
    crate::menu_wheel::claim(ui.ctx(), rect.union(name));
    (next != focus).then(|| ids[next].clone())
}
thread_local! { static ICONS:RefCell<HashMap<String,Arc<Mesh>>>=RefCell::new(HashMap::new()); }
pub(super) fn paint_icon(painter: &Painter, id: &str, rect: Rect, _color: Color32) {
    let Some(hat) = art::master().hats.get(id) else {
        return;
    };
    if hat.svg.is_empty() {
        super::ui::draw_glyph(painter, rect.shrink(5.0), super::ui::Glyph::Hats, _color);
        return;
    }
    let mesh = ICONS.with(|cache| {
        cache
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| Arc::new(art::hat_icon_mesh(hat)))
            .clone()
    });
    let scale = (rect.width() / 100.0).min(rect.height() / 90.0);
    super::cached_mesh(
        painter,
        Id::new(("presence-hat-icon", id)),
        mesh,
        egui::emath::TSTransform::new(
            rect.center().to_vec2() - egui::vec2(50.0, 45.0) * scale,
            scale,
        ),
        1.0,
    );
}
