use slate_doc::media::{ext_badge, media_kind, web_safe_video, MediaKind};
use slate_doc::scene::{
    Corner, Dash, Node, NodeId, NodeKind, Rgba, Scene, ShapeKind, TextAlign, WorldRect,
};
use slate_doc::SlateDoc;

use crate::assets::{mime_for_path, AssetMap};

const JS_RUNTIME: &str = r#"(function(){
var deck=document.getElementById('deck');
var slides=deck?[].slice.call(deck.querySelectorAll('.slide')):[];
var counter=document.querySelector('.counter');
var idx=0;
function syncVideos(){
for(var j=0;j<slides.length;j++){
var vids=slides[j].querySelectorAll('video');
for(var k=0;k<vids.length;k++){
var v=vids[k];
if(j===idx){if(v.hasAttribute('autoplay')){var p=v.play();if(p&&p.catch)p.catch(function(){});}}
else{v.pause();}
}
}
}
function show(i){
if(!slides.length)return;
idx=Math.max(0,Math.min(i,slides.length-1));
for(var j=0;j<slides.length;j++){slides[j].classList.toggle('active',j===idx);}
if(counter)counter.textContent=(idx+1)+' / '+slides.length;
syncVideos();
}
// Enforce trim windows (#t= fragments only seek the start; the out-point
// and loop-back-to-in-point need script).
[].slice.call(document.querySelectorAll('video[data-tstart]')).forEach(function(v){
var t0=parseFloat(v.getAttribute('data-tstart'))||0;
var t1=v.hasAttribute('data-tend')?parseFloat(v.getAttribute('data-tend')):NaN;
v.addEventListener('loadedmetadata',function(){if(v.currentTime<t0)v.currentTime=t0;});
v.addEventListener('timeupdate',function(){
if(v.currentTime<t0-0.25)v.currentTime=t0;
if(!isNaN(t1)&&v.currentTime>=t1){
if(v.loop){v.currentTime=t0;}else{v.pause();v.currentTime=t0;}
}
});
v.addEventListener('ended',function(){if(v.loop){v.currentTime=t0;var p=v.play();if(p&&p.catch)p.catch(function(){});}});
});
function scaleSlides(){
for(var i=0;i<slides.length;i++){
var s=slides[i];
var w=parseFloat(s.getAttribute('data-w'));
var h=parseFloat(s.getAttribute('data-h'));
var sc=Math.min(window.innerWidth/w,window.innerHeight/h)*0.96;
s.style.transform='scale('+sc+')';
}
}
function next(){show(idx+1);}
function prev(){show(idx-1);}
window.addEventListener('resize',scaleSlides);
document.addEventListener('keydown',function(e){
if(e.key==='ArrowRight'||e.key===' '||e.key==='PageDown'){e.preventDefault();next();}
else if(e.key==='ArrowLeft'||e.key==='PageUp'){prev();}
else if(e.key==='Home'){show(0);}
else if(e.key==='End'){show(slides.length-1);}
else if(e.key==='f'||e.key==='F'){
if(!document.fullscreenElement){document.documentElement.requestFullscreen();}
else{document.exitFullscreen();}
}
});
document.addEventListener('click',function(e){
var x=e.clientX/window.innerWidth;
if(x>2/3)next();
else if(x<1/3)prev();
});
show(0);
scaleSlides();
})();"#;

struct SlideSpec {
    width: f32,
    height: f32,
    origin_x: f32,
    origin_y: f32,
    background: String,
    member_ids: Vec<NodeId>,
}

/// Renders a complete HTML document for `doc` using resolved asset URLs.
pub fn render_html(doc: &SlateDoc, assets: &AssetMap) -> String {
    let slides = collect_slides(&doc.scene);
    let slide_count = slides.len();
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>");
    html.push_str(&escape_html(&doc.name));
    html.push_str("</title>\n<style>\n");
    html.push_str(CSS);
    html.push_str("</style>\n</head>\n<body>\n");

    if slide_count == 0 {
        html.push_str("<div class=\"empty\">Empty board</div>\n");
    } else {
        html.push_str("<div id=\"deck\">\n");
        for (i, spec) in slides.iter().enumerate() {
            render_slide(&mut html, doc, assets, spec, i == 0);
        }
        html.push_str("</div>\n");
        if slide_count == 1 {
            html.push_str("<div class=\"counter hidden\">1 / 1</div>\n");
        } else {
            html.push_str("<div class=\"counter\">1 / ");
            html.push_str(&slide_count.to_string());
            html.push_str("</div>\n");
        }
    }

    html.push_str("<script>\n");
    html.push_str(JS_RUNTIME);
    html.push_str("\n</script>\n</body>\n</html>\n");
    html
}

const CSS: &str = r#"*{box-sizing:border-box}
html,body{margin:0;height:100%;background:#111;overflow:hidden}
#deck{position:relative;width:100%;height:100%;display:flex;align-items:center;justify-content:center}
.slide{position:absolute;width:var(--sw);height:var(--sh);opacity:0;transition:opacity 150ms;transform-origin:center center;overflow:hidden}
.slide.active{opacity:1}
.node{position:absolute;overflow:hidden}
.ovl{position:absolute;inset:0;pointer-events:none;border-radius:inherit}
.missing,.filecard{display:flex;align-items:center;justify-content:center;width:100%;height:100%;background:#2a2a2a;color:#ccc;font:14px system-ui,sans-serif;text-align:center;padding:8px;word-break:break-word}
.filecard{flex-direction:column;gap:6px;text-decoration:none}
.badge{display:inline-block;background:#555;color:#eee;font:bold 11px system-ui,sans-serif;padding:2px 6px;border-radius:3px;letter-spacing:0.06em}
.thumbcard{display:block;position:relative;width:100%;height:100%;text-decoration:none}
.thumbcard img{width:100%;height:100%;object-fit:cover;display:block}
.thumbcard .badge{position:absolute;left:6px;bottom:6px}
.textcard{display:block;width:100%;height:100%;background:#fdfdfb;color:#222;text-decoration:none;overflow:hidden}
.textcard pre{margin:0;padding:10px 12px 4px;font:12px/1.45 ui-monospace,Consolas,monospace;white-space:pre-wrap;word-break:break-word}
.textcard .fname{display:block;padding:2px 12px 8px;color:#888;font:11px system-ui,sans-serif}
.empty{position:fixed;inset:0;display:flex;align-items:center;justify-content:center;color:#888;font:18px system-ui,sans-serif}
.counter{position:fixed;bottom:16px;right:16px;color:#888;font:14px monospace;z-index:100}
.counter.hidden{display:none}
"#;

fn collect_slides(scene: &Scene) -> Vec<SlideSpec> {
    let frames = scene.frames_in_order();
    if !frames.is_empty() {
        return frames
            .iter()
            .map(|frame| {
                let mut member_ids = scene.members_of(frame.id);
                member_ids.sort_by_key(|id| scene.index_of(*id).unwrap_or(usize::MAX));
                let background = match &frame.kind {
                    NodeKind::Frame(f) => f.fill.css(),
                    _ => Rgba::WHITE.css(),
                };
                SlideSpec {
                    width: frame.rect.w,
                    height: frame.rect.h,
                    origin_x: frame.rect.x,
                    origin_y: frame.rect.y,
                    background,
                    member_ids,
                }
            })
            .collect();
    }

    let content: Vec<&Node> = scene.nodes.iter().filter(|n| !n.is_frame()).collect();
    if content.is_empty() {
        return Vec::new();
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for node in &content {
        let r = node.rect;
        min_x = min_x.min(r.x);
        min_y = min_y.min(r.y);
        max_x = max_x.max(r.x + r.w);
        max_y = max_y.max(r.y + r.h);
    }

    const PAD: f32 = 40.0;
    let mut member_ids: Vec<NodeId> = content.iter().map(|n| n.id).collect();
    member_ids.sort_by_key(|id| scene.index_of(*id).unwrap_or(usize::MAX));

    vec![SlideSpec {
        width: max_x - min_x + PAD * 2.0,
        height: max_y - min_y + PAD * 2.0,
        origin_x: min_x - PAD,
        origin_y: min_y - PAD,
        background: Rgba::WHITE.css(),
        member_ids,
    }]
}

fn render_slide(
    html: &mut String,
    doc: &SlateDoc,
    assets: &AssetMap,
    spec: &SlideSpec,
    active: bool,
) {
    html.push_str("<section class=\"slide");
    if active {
        html.push_str(" active");
    }
    html.push_str("\" data-w=\"");
    html.push_str(&fmt_px(spec.width));
    html.push_str("\" data-h=\"");
    html.push_str(&fmt_px(spec.height));
    html.push_str("\" style=\"--sw:");
    html.push_str(&fmt_px(spec.width));
    html.push_str("px;--sh:");
    html.push_str(&fmt_px(spec.height));
    html.push_str("px;background-color:");
    html.push_str(&spec.background);
    html.push_str(";\">\n");

    for id in &spec.member_ids {
        if let Some(node) = doc.scene.node(*id) {
            render_node(html, doc, assets, node, spec.origin_x, spec.origin_y);
        }
    }

    html.push_str("</section>\n");
}

fn render_node(
    html: &mut String,
    doc: &SlateDoc,
    assets: &AssetMap,
    node: &Node,
    origin_x: f32,
    origin_y: f32,
) {
    let rel = node.rect.translated(-origin_x, -origin_y);
    match &node.kind {
        NodeKind::Image(img) => render_image(html, doc, assets, node, img, rel),
        NodeKind::Shape(shape) => render_shape(html, node, shape, rel),
        NodeKind::Text(text) => render_text(html, node, text, rel),
        NodeKind::Frame(_) => {}
    }
}

fn render_image(
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

    let mut style = geometry_style(rel);
    append_opacity(&mut style, node.opacity);
    append_corner(&mut style, img.corner);
    append_stroke(&mut style, &img.stroke);
    style.push_str("overflow:hidden;");

    html.push_str("<div class=\"node\" style=\"");
    html.push_str(&style);
    html.push_str("\">");

    let missing = item.is_none() || path_opt.is_none_or(|p| assets.get(p).is_none());
    if missing {
        html.push_str("<div class=\"missing\">");
        html.push_str(&escape_html(file_name));
        html.push_str("</div>");
    } else {
        let path = path_opt.unwrap();
        let url = assets.get(path).unwrap();

        match media_kind(path) {
            MediaKind::Image => render_img_tag(html, url, img),
            MediaKind::Video if web_safe_video(path) => {
                render_video_tag(html, url, img, path, assets.thumb(path));
            }
            MediaKind::Text => {
                render_text_card(html, url, file_name, assets.snippet(path), path);
            }
            // 3D models: the frozen-camera poster rendered on the board for
            // exactly this node (its saved perspective), falling back to
            // the generic item thumbnail, always linking to the copied
            // original so viewers can open it in Rhino.
            MediaKind::Model => {
                let poster = assets.model_poster(node.id).or_else(|| assets.thumb(path));
                match poster {
                    Some(poster_url) => {
                        render_poster_card(html, url, path, poster_url);
                    }
                    None => render_file_card(html, url, file_name, path, None),
                }
            }
            // PDFs, docs, non-web-safe video, workbooks (legacy docs may
            // still carry one as an item), anything else: poster thumbnail
            // when available, labeled card otherwise — always linking to the
            // copied original.
            _ => render_file_card(html, url, file_name, path, assets.thumb(path)),
        }
    }

    if let Some(overlay) = img.adjust.overlay {
        html.push_str("<div class=\"ovl\" style=\"background:");
        html.push_str(&overlay.css());
        html.push_str(";\"></div>");
    }

    html.push_str("</div>\n");
}

fn render_img_tag(html: &mut String, url: &str, img: &slate_doc::scene::ImageNode) {
    html.push_str("<img src=\"");
    html.push_str(&escape_attr(url));
    html.push_str("\" alt=\"\" style=\"");
    html.push_str(&crop_style(&img.crop));
    let filter = img.adjust.css_filter();
    if !filter.is_empty() {
        html.push_str("filter:");
        html.push_str(&filter);
        html.push(';');
    }
    html.push_str("\" draggable=\"false\">");
}

fn render_video_tag(
    html: &mut String,
    url: &str,
    img: &slate_doc::scene::ImageNode,
    path: &std::path::Path,
    poster: Option<&str>,
) {
    let mime = mime_for_path(path);
    let v = img.video.clamped();

    html.push_str("<video playsinline");
    if v.autoplay {
        html.push_str(" autoplay");
    }
    if v.looped {
        html.push_str(" loop");
    }
    if v.muted {
        html.push_str(" muted");
    }
    if v.controls {
        html.push_str(" controls");
    }
    if v.is_trimmed() {
        use std::fmt::Write;
        let _ = write!(html, " data-tstart=\"{:.3}\"", v.start);
        if let Some(end) = v.end {
            let _ = write!(html, " data-tend=\"{:.3}\"", end);
        }
    }
    if let Some(poster) = poster {
        html.push_str(" poster=\"");
        html.push_str(&escape_attr(poster));
        html.push('"');
    }
    html.push_str(" style=\"");
    html.push_str(&crop_style(&img.crop));
    let filter = img.adjust.css_filter();
    if !filter.is_empty() {
        html.push_str("filter:");
        html.push_str(&filter);
        html.push(';');
    }
    html.push_str("\"><source src=\"");
    html.push_str(&escape_attr(url));
    if v.is_trimmed() {
        use std::fmt::Write;
        let _ = write!(html, "#t={:.3}", v.start);
        if let Some(end) = v.end {
            let _ = write!(html, ",{end:.3}");
        }
    }
    html.push_str("\" type=\"");
    html.push_str(mime);
    html.push_str("\"></video>");
}

/// Excerpt card for text files: monospace snippet + filename, linked to the
/// copied original.
fn render_text_card(
    html: &mut String,
    url: &str,
    file_name: &str,
    snippet: Option<&str>,
    path: &std::path::Path,
) {
    match snippet {
        Some(snippet) => {
            html.push_str("<a class=\"textcard\" href=\"");
            html.push_str(&escape_attr(url));
            html.push_str("\" target=\"_blank\"><pre>");
            html.push_str(&escape_html(snippet));
            html.push_str("</pre><span class=\"fname\">");
            html.push_str(&escape_html(file_name));
            html.push_str("</span></a>");
        }
        None => render_file_card(html, url, file_name, path, None),
    }
}

/// Full-bleed poster card for 3D model nodes: the frozen-camera render
/// fills the node rect (it was rendered at this node's aspect), with the
/// extension badge marking it as a model file behind the image.
fn render_poster_card(html: &mut String, url: &str, path: &std::path::Path, poster: &str) {
    let badge = ext_badge(path);
    html.push_str("<a class=\"thumbcard\" href=\"");
    html.push_str(&escape_attr(url));
    html.push_str("\" target=\"_blank\"><img src=\"");
    html.push_str(&escape_attr(poster));
    html.push_str("\" alt=\"\" draggable=\"false\">");
    if !badge.is_empty() {
        html.push_str("<span class=\"badge\">");
        html.push_str(&escape_html(&badge));
        html.push_str("</span>");
    }
    html.push_str("</a>");
}

/// Poster-thumbnail card (when the app supplied one) or a labeled card with
/// an extension badge; either way a link to the copied original.
fn render_file_card(
    html: &mut String,
    url: &str,
    file_name: &str,
    path: &std::path::Path,
    thumb: Option<&str>,
) {
    let badge = ext_badge(path);
    match thumb {
        Some(thumb) => {
            html.push_str("<a class=\"thumbcard\" href=\"");
            html.push_str(&escape_attr(url));
            html.push_str("\" target=\"_blank\"><img src=\"");
            html.push_str(&escape_attr(thumb));
            html.push_str("\" alt=\"\" draggable=\"false\">");
            if !badge.is_empty() {
                html.push_str("<span class=\"badge\">");
                html.push_str(&escape_html(&badge));
                html.push_str("</span>");
            }
            html.push_str("</a>");
        }
        None => {
            html.push_str("<a class=\"filecard\" href=\"");
            html.push_str(&escape_attr(url));
            html.push_str("\" target=\"_blank\">");
            if !badge.is_empty() {
                html.push_str("<span class=\"badge\">");
                html.push_str(&escape_html(&badge));
                html.push_str("</span>");
            }
            html.push_str("<span>");
            html.push_str(&escape_html(file_name));
            html.push_str("</span></a>");
        }
    }
}

fn render_shape(
    html: &mut String,
    node: &Node,
    shape: &slate_doc::scene::ShapeNode,
    rel: WorldRect,
) {
    match shape.shape {
        ShapeKind::Line => render_line(html, node, shape, rel),
        ShapeKind::Rect => render_rect_shape(html, node, shape, rel, false),
        ShapeKind::Ellipse => render_rect_shape(html, node, shape, rel, true),
    }
}

fn render_rect_shape(
    html: &mut String,
    node: &Node,
    shape: &slate_doc::scene::ShapeNode,
    rel: WorldRect,
    ellipse: bool,
) {
    let mut style = geometry_style(rel);
    append_opacity(&mut style, node.opacity);

    if ellipse {
        style.push_str("border-radius:50%;");
    } else {
        append_corner(&mut style, shape.corner);
    }

    if let Some(fill) = shape.fill {
        style.push_str("background:");
        style.push_str(&fill.css());
        style.push(';');
    } else {
        style.push_str("background:transparent;");
    }

    append_stroke(&mut style, &shape.stroke);

    html.push_str("<div class=\"node\" style=\"");
    html.push_str(&style);
    html.push_str("\"></div>\n");
}

fn render_line(
    html: &mut String,
    node: &Node,
    shape: &slate_doc::scene::ShapeNode,
    rel: WorldRect,
) {
    let w = rel.w;
    let h = rel.h;
    let (x1, y1, x2, y2) = if shape.flip {
        (0.0, h, w, 0.0)
    } else {
        (0.0, 0.0, w, h)
    };

    let stroke_color = shape.stroke.color.css();
    let stroke_width = shape.stroke.width;
    let dash = line_dash_attrs(&shape.stroke);

    let mut wrap = geometry_style(rel);
    append_opacity(&mut wrap, node.opacity);
    wrap.push_str("overflow:visible;background:transparent;");

    html.push_str("<div class=\"node\" style=\"");
    html.push_str(&wrap);
    html.push_str("\"><svg width=\"");
    html.push_str(&fmt_px(w));
    html.push_str("\" height=\"");
    html.push_str(&fmt_px(h));
    html.push_str("\" viewBox=\"0 0 ");
    html.push_str(&fmt_px(w));
    html.push(' ');
    html.push_str(&fmt_px(h));
    html.push_str("\" style=\"display:block;overflow:visible\"><line x1=\"");
    html.push_str(&fmt_px(x1));
    html.push_str("\" y1=\"");
    html.push_str(&fmt_px(y1));
    html.push_str("\" x2=\"");
    html.push_str(&fmt_px(x2));
    html.push_str("\" y2=\"");
    html.push_str(&fmt_px(y2));
    html.push_str("\" stroke=\"");
    html.push_str(&stroke_color);
    html.push_str("\" stroke-width=\"");
    html.push_str(&fmt_px(stroke_width));
    html.push('"');
    if let Some(dash) = dash {
        html.push_str(" stroke-dasharray=\"");
        html.push_str(dash);
        html.push('"');
    }
    if shape.stroke.dash == Dash::Dotted {
        html.push_str(" stroke-linecap=\"round\"");
    }
    html.push_str("></line></svg></div>\n");
}

fn render_text(html: &mut String, node: &Node, text: &slate_doc::scene::TextNode, rel: WorldRect) {
    let mut style = geometry_style(rel);
    append_opacity(&mut style, node.opacity);
    style.push_str("font-family:");
    style.push_str(text.family.css_stack());
    style.push_str(";font-size:");
    style.push_str(&fmt_px(text.size));
    style.push_str("px;color:");
    style.push_str(&text.color.css());
    style.push_str(";text-align:");
    style.push_str(text_align_css(text.align));
    style.push_str(";white-space:pre-wrap;line-height:1.3;overflow:hidden;");

    html.push_str("<div class=\"node\" style=\"");
    html.push_str(&style);
    html.push_str("\">");
    html.push_str(&escape_html(&text.text));
    html.push_str("</div>\n");
}

fn geometry_style(rect: WorldRect) -> String {
    format!(
        "left:{}px;top:{}px;width:{}px;height:{}px;",
        fmt_px(rect.x),
        fmt_px(rect.y),
        fmt_px(rect.w),
        fmt_px(rect.h),
    )
}

fn append_opacity(style: &mut String, opacity: f32) {
    if (opacity - 1.0).abs() > f32::EPSILON {
        use std::fmt::Write;
        let _ = write!(style, "opacity:{:.3};", opacity);
    }
}

fn append_corner(style: &mut String, corner: Corner) {
    match corner {
        Corner::Square => {}
        Corner::Rounded { radius } => {
            use std::fmt::Write;
            let _ = write!(style, "border-radius:{:.1}px;", radius);
        }
        Corner::Chamfer { cut } => {
            use std::fmt::Write;
            let c = fmt_px(cut);
            let _ = write!(
                style,
                "clip-path:polygon({c}px 0,calc(100% - {c}px) 0,100% {c}px,100% calc(100% - {c}px),calc(100% - {c}px) 100%,{c}px 100%,0 calc(100% - {c}px),0 {c}px);",
            );
        }
    }
}

fn append_stroke(style: &mut String, stroke: &slate_doc::scene::Stroke) {
    if stroke.is_none() {
        return;
    }
    use std::fmt::Write;
    let dash = match stroke.dash {
        Dash::Solid => "solid",
        Dash::Dashed => "dashed",
        Dash::Dotted => "dotted",
    };
    let _ = write!(
        style,
        "border:{:.1}px {} {};",
        stroke.width,
        dash,
        stroke.color.css()
    );
}

fn crop_style(crop: &slate_doc::scene::Crop) -> String {
    format!(
        "position:absolute;width:{:.4}%;height:{:.4}%;left:{:.4}%;top:{:.4}%;",
        100.0 / crop.w,
        100.0 / crop.h,
        -crop.x / crop.w * 100.0,
        -crop.y / crop.h * 100.0,
    )
}

fn line_dash_attrs(stroke: &slate_doc::scene::Stroke) -> Option<&'static str> {
    match stroke.dash {
        Dash::Solid => None,
        Dash::Dashed => Some("12 8"),
        Dash::Dotted => Some("2 6"),
    }
}

fn text_align_css(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn fmt_px(v: f32) -> String {
    format!("{:.1}", v)
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    escape_html(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_entities() {
        assert_eq!(
            escape_html(r#"<script>"a" & 'b'</script>"#),
            "&lt;script&gt;&quot;a&quot; &amp; &#39;b&#39;&lt;/script&gt;"
        );
    }

    #[test]
    fn crop_style_math() {
        use slate_doc::scene::Crop;
        let s = crop_style(&Crop {
            x: 0.25,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        });
        assert!(s.contains("width:200.0000%"));
        assert!(s.contains("left:-50.0000%"));
    }
}
