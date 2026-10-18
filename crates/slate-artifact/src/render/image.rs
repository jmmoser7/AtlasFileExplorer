//! Image framing, mask and source-pixel export.

use super::*;

pub(super) fn render_image(
    html: &mut String,
    doc: &SlateDoc,
    assets: &AssetMap,
    node: &Node,
    img: &slate_doc::scene::ImageNode,
    rel: WorldRect,
) {
    let item = doc.item(img.item);
    let (file_name, path_opt) = match item {
        Some(item) => (item.file_name.as_str(), Some(&item.path)),
        None => ("?", None),
    };

    let mut style = geometry_style(rel, node.rotation_deg);
    append_opacity(&mut style, node.opacity);
    let corner = path_opt
        .map(|path| slate_doc::media::text_card_corner(path, img.corner))
        .unwrap_or(img.corner);
    if node.clip.is_none() {
        append_corner(&mut style, corner, rel.w, rel.h);
        append_stroke(&mut style, &img.stroke);
        style.push_str("overflow:hidden;");
    } else {
        style.push_str("overflow:visible;");
    }

    html.push_str("<div class=\"node\" style=\"");
    html.push_str(&style);
    html.push_str("\">");
    if node.clip.is_some() {
        let mut pixels = String::from("position:absolute;inset:0;overflow:hidden;");
        append_clip(&mut pixels, node, rel);
        html.push_str("<div class=\"image-mask\" style=\"");
        html.push_str(&pixels);
        html.push_str("\">");
    }

    // A picture an agent makes shows its newest result until one is picked;
    // the app lists the shown result first, as the board paints it.
    let shown = (img.agent.is_some() && img.item.is_none())
        .then(|| assets.agent_images(node.id).and_then(|v| v.first()))
        .flatten();
    let missing = item.is_none() || path_opt.is_none_or(|p| assets.get(p).is_none());
    if let Some(url) = shown {
        render_img_tag(html, url, img, node.id);
    } else if img.agent.is_some() && img.item.is_none() {
        // Not generated yet: an empty picture, not a missing file.
    } else if let Some(text) = img
        .text_override
        .as_deref()
        .filter(|_| path_opt.is_some_and(|path| media_kind(path) == MediaKind::Text))
    {
        let path = path_opt.unwrap();
        render_text_card(html, assets.get(path), file_name, Some(text), path);
    } else if missing {
        html.push_str("<div class=\"missing\">");
        html.push_str(&escape_html(file_name));
        html.push_str("</div>");
    } else {
        let path = path_opt.unwrap();
        let url = assets.get(path).unwrap();

        match media_kind(path) {
            MediaKind::Image => render_img_tag(html, url, img, node.id),
            MediaKind::Video if web_safe_video(path) => {
                render_video_tag(html, url, img, path, assets.item_thumb(img.item, path));
            }
            MediaKind::Text => {
                if let Some(sheet) = assets.sheet(path) {
                    render_sheet_card(html, url, file_name, sheet, &img.sheet);
                } else {
                    render_text_card(html, Some(url), file_name, assets.snippet(path), path);
                }
            }
            // 3D models: the frozen-camera poster for exactly this node,
            // falling back to the item thumbnail, always linking to the
            // copied original. No extension badge, matching the board.
            MediaKind::Model => {
                let poster = assets
                    .model_poster(node.id)
                    .or_else(|| assets.item_thumb(img.item, path));
                render_model_poster_card(html, url, poster, img);
            }
            // PDFs, docs, non-web-safe video, workbooks (legacy docs may
            // still carry one as an item), anything else: poster thumbnail
            // when available, labeled card otherwise — always linking to the
            // copied original.
            _ => render_file_card(
                html,
                url,
                file_name,
                &ext_badge(path),
                assets.item_thumb(img.item, path),
            ),
        }
    }

    let alpha_filtered = shown.is_some()
        || (img.agent.is_some() && img.item.is_none())
        || path_opt.is_some_and(|path| media_kind(path) == MediaKind::Image);
    if let Some(overlay) = img.adjust.overlay.filter(|_| !alpha_filtered) {
        html.push_str("<div class=\"ovl\" style=\"background:");
        html.push_str(&overlay.css());
        html.push_str(";\"></div>");
    }

    render_image_paint_layers(html, doc, assets, node, img, rel);

    if let Some(clip) = &node.clip {
        html.push_str("</div>");
        if !img.stroke.is_none() {
            // The existing path interpreter owns caps, joins, dash, softness,
            // brush profiles and compound contours in both shape and mask ink.
            let shape = slate_doc::scene::ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke: img.stroke,
                corner: Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(std::sync::Arc::new(clip.clone())),
                text: None,
            };
            let mut local = node.clone();
            local.rect = WorldRect::new(0.0, 0.0, rel.w, rel.h);
            local.rotation_deg = 0.0;
            local.opacity = 1.0;
            local.clip = None;
            render_path(html, &local, &shape, local.rect);
        }
    }

    html.push_str("</div>\n");
}

/// Authored text remains visible when its linked original is unavailable.
fn render_text_card(
    html: &mut String,
    url: Option<&str>,
    file_name: &str,
    snippet: Option<&str>,
    path: &std::path::Path,
) {
    let Some(snippet) = snippet else {
        if let Some(url) = url {
            render_file_card(html, url, file_name, &ext_badge(path), None);
        }
        return;
    };
    if let Some(url) = url {
        html.push_str("<a class=\"textcard\" href=\"");
        html.push_str(&escape_attr(url));
        html.push_str("\" target=\"_blank\">");
    } else {
        html.push_str("<div class=\"textcard\">");
    }
    html.push_str("<pre>");
    html.push_str(&escape_html(snippet));
    html.push_str("</pre><span class=\"fname\">");
    html.push_str(&escape_html(file_name));
    html.push_str(if url.is_some() {
        "</span></a>"
    } else {
        "</span></div>"
    });
}

#[cfg(test)]
mod tests;
