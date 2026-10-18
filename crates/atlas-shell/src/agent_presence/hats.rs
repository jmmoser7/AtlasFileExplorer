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
    rx: mpsc::Receiver<((usize, bool), egui::ColorImage)>,
    textures: HashMap<(usize, bool), TextureHandle>,
}
fn textures(ctx: &egui::Context) -> Vec<[Option<egui::TextureId>; 2]> {
    let key = Id::new("presence-hat-textures");
    let shared=ctx.data_mut(|data|data.get_temp_mut_or_insert_with(key,||{
        let (tx,rx)=mpsc::channel();let context=ctx.clone();
        std::thread::spawn(move||{
            for (index,id) in art::master().hat_order.iter().enumerate() {
                let hat=&art::master().hats[id];
                let artwork=if hat.svg.is_empty(){"<text x=\"50\" y=\"55\" text-anchor=\"middle\" font-family=\"Segoe UI,sans-serif\" font-size=\"12\" fill=\"#9aa1a8\">No hat</text>".to_owned()}else{format!("<g transform=\"translate(10 10) scale(.8)\">{}</g>",hat.svg)};
                // Only card material varies; the shared album still owns fit,
                // wheel, spring, projection and selection.
                for selected in [false, true] {
                    let svg = card_svg(&artwork, selected);
                    if let Some((w,h,bytes))=atlas_core::svg::rasterize_bytes(svg.as_bytes(),200) {
                        if tx.send(((index, selected),egui::ColorImage::from_rgba_unmultiplied([w as usize,h as usize],&bytes))).is_err(){return;}
                        context.request_repaint();
                    }
                }
            }
        });
        Arc::new(Mutex::new(Loading{rx,textures:HashMap::new()}))
    }).clone());
    let Ok(mut shared) = shared.lock() else {
        return vec![[None; 2]; art::master().hat_order.len()];
    };
    for _ in 0..2 {
        let Ok((key, image)) = shared.rx.try_recv() else {
            break;
        };
        let texture = ctx.load_texture(
            format!("presence-hat-card-{}-{}", key.0, key.1),
            image,
            home::COVER_TEXTURE_OPTIONS,
        );
        shared.textures.insert(key, texture);
        ctx.request_repaint();
    }
    (0..art::master().hat_order.len())
        .map(|n| {
            [
                shared.textures.get(&(n, false)).map(TextureHandle::id),
                shared.textures.get(&(n, true)).map(TextureHandle::id),
            ]
        })
        .collect()
}
fn card_svg(artwork: &str, selected: bool) -> String {
    let material = if selected {
        "<defs><linearGradient id=\"selected-card\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"><stop stop-color=\"#f0f6ff\"/><stop offset=\"1\" stop-color=\"#e9f0f9\"/></linearGradient></defs><rect x=\".625\" y=\".625\" width=\"98.75\" height=\"98.75\" rx=\"12.375\" fill=\"url(#selected-card)\" stroke=\"#9bbce7\" stroke-width=\"1.25\"/>"
    } else {
        "<rect width=\"100\" height=\"100\" rx=\"13\" fill=\"#f1f2f0\"/>"
    };
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"200\" viewBox=\"0 0 100 100\">{material}{artwork}</svg>")
}
pub(super) fn picker(ui: &mut Ui, palette: &Palette, selected: &str, id: Id) -> Option<String> {
    let ids = &art::master().hat_order;
    let focus = ids.iter().position(|id| id == selected).unwrap_or(0);
    let textures = textures(ui.ctx());
    let images: Vec<_> = textures
        .iter()
        .enumerate()
        .map(|(index, texture)| AlbumImage {
            texture: texture[if index == focus { 1 } else { 0 }].or(texture[0]),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_card_material_rasterizes_the_approved_blue_edge_and_tint() {
        let raster = |selected| {
            atlas_core::svg::rasterize_bytes(card_svg("", selected).as_bytes(), 200).unwrap()
        };
        let (width, height, selected) = raster(true);
        let (plain_width, plain_height, plain) = raster(false);
        assert_eq!((width, height), (plain_width, plain_height));
        let pixel = |bytes: &[u8], x: usize, y: usize| {
            let start = (y * width as usize + x) * 4;
            [
                bytes[start],
                bytes[start + 1],
                bytes[start + 2],
                bytes[start + 3],
            ]
        };
        let edge = pixel(&selected, 2, height as usize / 2);
        let tint = pixel(&selected, width as usize / 2, height as usize / 2);
        let neutral = pixel(&plain, width as usize / 2, height as usize / 2);
        assert!(
            u16::from(edge[2]) > u16::from(edge[0]) + 30 && edge[3] > 240,
            "selected border: {edge:?}"
        );
        assert!(
            tint[2] > tint[0] + 5 && tint[3] == 255,
            "selected tint: {tint:?}"
        );
        assert!(
            neutral[2] <= neutral[1],
            "normal card stays neutral: {neutral:?}"
        );
    }
}
