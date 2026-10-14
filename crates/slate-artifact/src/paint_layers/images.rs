//! Pixel source adapter for hosted image children. Filtering has one shared owner.
use super::*;
use std::io::Cursor;

pub(super) fn image_data_uri(doc: &SlateDoc, img: &ImageNode) -> Option<String> {
    let path = &doc.item(img.item)?.path;
    if atlas_core::cloud::is_dehydrated(path) {
        return None;
    }
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes = std::fs::read(path).ok()?;
    let (mime, bytes) = if !img.adjust.is_identity()
        || !img.crop.is_full()
        || !matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "gif" | "webp" | "svg" | "png"
        ) {
        let mut pixels = decode(&bytes, &ext)?;
        let crop = img.crop.clamped();
        let w = pixels.width().max(1);
        let h = pixels.height().max(1);
        // Crop coordinates are recorded in the displayed (mirrored) image.
        // The SVG image element supplies mirroring after this raw-pixel crop.
        let cx = if img.flip_x {
            1.0 - crop.x - crop.w
        } else {
            crop.x
        };
        let cy = if img.flip_y {
            1.0 - crop.y - crop.h
        } else {
            crop.y
        };
        let x = ((cx * w as f32).round() as u32).min(w - 1);
        let y = ((cy * h as f32).round() as u32).min(h - 1);
        let cw = ((crop.w * w as f32).round() as u32).clamp(1, w - x);
        let ch = ((crop.h * h as f32).round() as u32).clamp(1, h - y);
        pixels = image::imageops::crop_imm(&pixels, x, y, cw, ch).to_image();
        slate_doc::image_adjust::adjust_rgba8(pixels.as_mut(), &img.adjust);
        let mut encoded = Cursor::new(Vec::new());
        pixels
            .write_to(&mut encoded, image::ImageFormat::Png)
            .ok()?;
        ("image/png", encoded.into_inner())
    } else {
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        (mime, bytes)
    };
    Some(format!(
        "data:{mime};base64,{}",
        crate::assets::base64_encode(&bytes)
    ))
}

fn decode(bytes: &[u8], ext: &str) -> Option<image::RgbaImage> {
    if ext != "svg" {
        return image::load_from_memory(bytes)
            .ok()
            .map(|image| image.to_rgba8());
    }
    let tree = usvg::Tree::from_data(bytes, svg_options()).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    image::RgbaImage::from_vec(size.width(), size.height(), unpremultiply(pixmap))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::image_paint::{PaintLayer, PaintLayerId};
    use slate_doc::scene::{Crop, ImageAdjust};

    #[test]
    fn layer_bake_preserves_group_opacity_and_bottom_first_render_order() {
        let dir = std::env::temp_dir().join(format!("slate-layer-bake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut doc = SlateDoc::new("layer bake");
        let mut items = Vec::new();
        for (name, color) in [("red", [255, 0, 0, 255]), ("blue", [0, 0, 255, 255])] {
            let path = dir.join(format!("{name}.png"));
            image::RgbaImage::from_pixel(2, 2, image::Rgba(color))
                .save(&path)
                .unwrap();
            items.push(doc.add_item(path, name, 0, 0, ""));
        }
        let host = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 32.0, 32.0),
            NodeKind::Image(ImageNode::new(items[0])),
        );
        let red = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Image(ImageNode::new(items[0])),
        );
        let blue = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Image(ImageNode::new(items[1])),
        );
        let mut img = ImageNode::new(items[0]);
        img.paint_layers.push(PaintLayer {
            opacity: 0.5,
            nodes: vec![red.clone(), red],
            ..PaintLayer::new(PaintLayerId(1))
        });
        let mut stamps = LayerStamps::default();
        let cutout = rasterize_paint_layers(&host, &img, 32, 32, Some(&doc), &mut stamps).unwrap();
        assert_eq!(
            &cutout[(16 * 32 + 16) * 4..][..4],
            &[255, 0, 0, 128],
            "layer opacity applies once to overlapping children"
        );
        img.paint_layers[0].opacity = 1.0;
        img.paint_layers.push(PaintLayer {
            opacity: 0.5,
            nodes: vec![blue],
            ..PaintLayer::new(PaintLayerId(2))
        });
        let board = rasterize_paint_layers(&host, &img, 32, 32, Some(&doc), &mut stamps).unwrap();
        let svg = paint_layers_svg_with_doc(&host, &img, 32, 32, &doc);
        let exported = rasterize_paint_layers_svg(&svg, 32, 32).unwrap();
        assert_eq!(board.len(), exported.len());
        for (board, exported) in board
            .as_chunks::<4>()
            .0
            .iter()
            .zip(exported.as_chunks::<4>().0.iter())
        {
            assert_eq!(board[3], exported[3], "both renderers preserve alpha");
            // SVG composites authored floating-point opacity; the board
            // composites an 8-bit layer alpha. Their rounding can differ by 1.
            for channel in 0..3 {
                assert!(board[channel].abs_diff(exported[channel]) <= 1);
            }
        }
        assert_eq!(
            &board[(16 * 32 + 16) * 4..][..4],
            &[127, 0, 128, 255],
            "last adapted layer paints above earlier layers"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selected_image_child_keeps_filter_and_mirrored_crop_in_raster_export() {
        let dir = std::env::temp_dir().join(format!("slate-filtered-child-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("two-colors.png");
        let mut source = image::RgbaImage::from_pixel(8, 4, image::Rgba([100, 0, 0, 255]));
        for y in 0..4 {
            for x in 4..8 {
                source.put_pixel(x, y, image::Rgba([0, 0, 100, 255]));
            }
        }
        source.save(&path).unwrap();
        let mut doc = SlateDoc::new("filtered cutout");
        let item = doc.add_item(path, "two-colors.png", 0, 0, "");
        let mut img = ImageNode::new(item);
        let host = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 32.0, 32.0),
            NodeKind::Image(img.clone()),
        );
        let mut child_img = ImageNode::new(item);
        child_img.adjust = ImageAdjust {
            brightness: 2.0,
            ..Default::default()
        };
        child_img.flip_x = true;
        child_img.crop = Crop {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        let child = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, 1.0, 1.0),
            NodeKind::Image(child_img),
        );
        img.paint_layers.push(PaintLayer {
            nodes: vec![child],
            ..PaintLayer::new(PaintLayerId(1))
        });
        let svg = paint_layers_svg_with_doc(&host, &img, 32, 32, &doc);
        let exported = rasterize_paint_layers_svg(&svg, 32, 32).unwrap();
        let board =
            rasterize_paint_layers(&host, &img, 32, 32, Some(&doc), &mut LayerStamps::default())
                .unwrap();
        assert_eq!(exported, board);
        let middle = &board[(16 * 32 + 16) * 4..][..4];
        assert_eq!(middle, &[0, 0, 200, 255]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
