use std::path::Path;

use slate_doc::media::{ext_badge, media_kind, web_safe_video, MediaKind};
use slate_doc::scene::{
    web_origin, ConnectorNode, Corner, Dash, DockStripNode, Node, NodeId, NodeKind, PathData,
    PathFillRule, PathSeg, PortalKind, PortalNode, Rgba, Scene, ShapeKind, StrokeCap, StrokeJoin,
    TextAlign, WebExport, WebSourceKind, WidthProfile, WireDisplay, WorldRect,
};
use slate_doc::wire::{
    connector_route_in_scene, filleted_polyline, ConnectorPath, PathCmd, WireRouting,
    ORTHO_CORNER_RADIUS,
};
use slate_doc::SlateDoc;
use vector_ink::kurbo::{BezPath, PathEl, Point};
use vector_ink::{Cap, Join, StrokeStyle};

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
    render_html_with_workbook(doc, assets, None)
}

/// Same as [`render_html`], with a workbook directory so relative portal
/// locators can resolve (Art. IX.2).
pub fn render_html_with_workbook(
    doc: &SlateDoc,
    assets: &AssetMap,
    workbook_dir: Option<&Path>,
) -> String {
    render_html_routed(doc, assets, workbook_dir, WireRouting::Bezier)
}

pub(crate) fn render_html_routed(
    doc: &SlateDoc,
    assets: &AssetMap,
    workbook_dir: Option<&Path>,
    routing: WireRouting,
) -> String {
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
            render_slide(&mut html, doc, assets, spec, i == 0, workbook_dir, routing);
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
    workbook_dir: Option<&Path>,
    routing: WireRouting,
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

    let mut wires = Vec::new();
    let mut rest = Vec::new();
    for id in &spec.member_ids {
        if let Some(node) = doc.scene.node(*id) {
            if matches!(node.kind, NodeKind::Connector(_)) {
                wires.push(node);
            } else {
                rest.push(node);
            }
        }
    }
    for node in wires.into_iter().chain(rest) {
        render_node(
            html,
            doc,
            assets,
            node,
            spec.origin_x,
            spec.origin_y,
            workbook_dir,
            routing,
        );
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
    workbook_dir: Option<&Path>,
    routing: WireRouting,
) {
    // Export honesty (Art. IV): hidden nodes are not part of what the board
    // shows, so they never reach the artifact.
    if node.hidden {
        return;
    }
    let rel = node.rect.translated(-origin_x, -origin_y);
    match &node.kind {
        NodeKind::Image(img) => render_image(html, doc, assets, node, img, rel),
        NodeKind::Shape(shape) => render_shape(html, node, shape, rel),
        NodeKind::Text(text) => render_text(html, node, text, rel),
        NodeKind::Connector(conn) => {
            render_connector(html, &doc.scene, node, conn, origin_x, origin_y, routing)
        }
        NodeKind::Frame(_) => {}
        NodeKind::DockStrip(s) => render_dock_strip(html, node, s, rel),
        NodeKind::Portal(p) if p.kind == PortalKind::Web => {
            render_web_portal(html, assets, node, p, rel);
        }
        NodeKind::Portal(p) => render_portal(html, node, p, rel, workbook_dir),
    }
}

fn render_portal(
    html: &mut String,
    node: &Node,
    portal: &PortalNode,
    rel: WorldRect,
    workbook_dir: Option<&Path>,
) {
    let mut style = geometry_style(rel, node.rotation_deg);
    append_opacity(&mut style, node.opacity);
    style.push_str("background:");
    style.push_str(&portal.fill.css());
    style.push_str(";overflow:hidden;");
    html.push_str("<div class=\"node portal\" style=\"");
    html.push_str(&style);
    html.push_str("\">");

    match portal.kind {
        PortalKind::StatusBoard => render_status_board(html, portal, node.rect, workbook_dir),
        PortalKind::Web => unreachable!("web portals render via render_web_portal"),
        PortalKind::RepoLens => {
            // Repository Lens still exports a poster shell; bake to promote
            // graph geometry into authored nodes the writer serializes.
            html.push_str(
                "<div style=\"width:100%;height:100%;display:flex;align-items:center;justify-content:center;color:rgba(228,230,235,0.85);font:14px system-ui,sans-serif;\">",
            );
            html.push_str(&escape_html(&portal.title));
            html.push_str("</div>");
        }
        PortalKind::Agent => {
            let pointer = portal
                .agent
                .as_ref()
                .map(|a| format!("{} / {}", a.provider, a.session))
                .unwrap_or_else(|| "unbound agent".into());
            html.push_str("<div style=\"width:100%;height:100%;display:flex;align-items:center;justify-content:center;color:rgba(228,230,235,0.85);font:14px system-ui,sans-serif;text-align:center\"><strong>");
            html.push_str(&escape_html(&portal.title));
            html.push_str("</strong><br><span style=\"opacity:.7\">Host portal poster: ");
            html.push_str(&escape_html(&pointer));
            html.push_str(" (live agent state is not exported)</span></div>");
        }
    }

    html.push_str("</div>\n");
}

fn render_dock_strip(html: &mut String, node: &Node, strip: &DockStripNode, rel: WorldRect) {
    let mut style = geometry_style(rel, node.rotation_deg);
    append_opacity(&mut style, node.opacity);
    style.push_str(
        "background:rgba(32,34,40,0.72);border:1px solid rgba(255,255,255,0.12);border-radius:8px;display:flex;align-items:center;gap:6px;padding:0 10px;color:rgba(228,230,235,0.9);font:13px system-ui,sans-serif;",
    );
    html.push_str("<div class=\"node dock-strip\" style=\"");
    html.push_str(&style);
    html.push_str("\">");
    html.push_str(&escape_html(&strip.palette_id));
    if !strip.visible.is_empty() {
        html.push_str("<span style=\"opacity:.65\"> · ");
        html.push_str(&escape_html(&strip.visible.join(" · ")));
        html.push_str("</span>");
    }
    html.push_str("</div>\n");
}

fn render_status_board(
    html: &mut String,
    portal: &PortalNode,
    frame: WorldRect,
    workbook_dir: Option<&Path>,
) {
    let Some(src) = &portal.source else {
        html.push_str(
            "<div style=\"width:100%;height:100%;display:flex;align-items:center;justify-content:center;color:rgba(228,230,235,0.85);font:14px system-ui,sans-serif;\">Choose status snapshot…</div>",
        );
        return;
    };
    let path = resolve_export_source(workbook_dir, &src.locator);
    match status_board::load_snapshot(&path) {
        Ok(snap) => {
            let query = status_board::StatusQuery {
                show_overview: portal.status.show_overview,
                show_phases: portal.status.show_phases,
                show_waves: portal.status.show_waves,
                show_deviations: portal.status.show_deviations,
                show_next: portal.status.show_next,
            };
            let layout = status_board::layout_status(
                &snap,
                &query,
                status_board::Size {
                    w: frame.w.max(1.0),
                    h: frame.h.max(1.0),
                },
            );
            html.push_str(&status_layout_svg(&layout));
        }
        Err(err) => {
            html.push_str(
                "<div style=\"width:100%;height:100%;display:flex;align-items:center;justify-content:center;color:rgba(228,230,235,0.85);font:13px system-ui,sans-serif;padding:16px;text-align:center;\">",
            );
            html.push_str(&escape_html(&err.to_string()));
            html.push_str("</div>");
        }
    }
}

fn resolve_export_source(workbook_dir: Option<&Path>, locator: &str) -> std::path::PathBuf {
    let p = Path::new(locator);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    if let Some(dir) = workbook_dir {
        return dir.join(p);
    }
    p.to_path_buf()
}

fn status_layout_svg(layout: &status_board::StatusLayout) -> String {
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100%\" height=\"100%\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\">",
        layout.bounds.w, layout.bounds.h
    );
    for prim in &layout.prims {
        match prim {
            status_board::Prim::Fill { rect, rgba } => {
                svg.push_str(&format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\"/>",
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    css_rgba(*rgba)
                ));
            }
            status_board::Prim::Stroke { rect, rgba, width } => {
                svg.push_str(&format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"/>",
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    css_rgba(*rgba),
                    width
                ));
            }
            status_board::Prim::Bar { rect, frac, rgba } => {
                let w = rect.w * frac.clamp(0.0, 1.0);
                svg.push_str(&format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\"/>",
                    rect.x,
                    rect.y,
                    w,
                    rect.h,
                    css_rgba(*rgba)
                ));
            }
            status_board::Prim::Text {
                x,
                y,
                text,
                size,
                rgba,
                align,
            } => {
                let anchor = match align {
                    status_board::Align::Left => "start",
                    status_board::Align::Center => "middle",
                    status_board::Align::Right => "end",
                };
                svg.push_str(&format!(
                    "<text x=\"{:.2}\" y=\"{:.2}\" fill=\"{}\" font-size=\"{:.1}\" font-family=\"system-ui,sans-serif\" text-anchor=\"{}\" dominant-baseline=\"hanging\">{}</text>",
                    x,
                    y,
                    css_rgba(*rgba),
                    size,
                    anchor,
                    escape_html(text)
                ));
            }
        }
    }
    svg.push_str("</svg>");
    svg
}

fn css_rgba(rgba: [u8; 4]) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        rgba[0],
        rgba[1],
        rgba[2],
        rgba[3] as f32 / 255.0
    )
}

/// A web portal serializes as whatever is honest for its source (D24).
///
/// Local material was packaged beside the artifact, so an `<iframe>` over the
/// copy *is* the serialization Art. IV.1 asks for — a screenshot of a working
/// dashboard would be the lossy imitation it forbids. A remote page cannot be
/// packaged and must not be silently refetched, so it gets the poster + pointer
/// Art. V.3 names. Either way the caption states what the reader is looking at.
fn render_web_portal(
    html: &mut String,
    assets: &AssetMap,
    node: &Node,
    portal: &slate_doc::scene::PortalNode,
    rel: WorldRect,
) {
    let mut style = geometry_style(rel, node.rotation_deg);
    append_opacity(&mut style, node.opacity);
    style.push_str("overflow:hidden;background:");
    style.push_str(&portal.fill.css());
    style.push(';');
    html.push_str("<div class=\"node portal portal-web\" style=\"");
    html.push_str(&style);
    html.push_str("\">");

    let locator = portal
        .source
        .as_ref()
        .map(|s| s.locator.clone())
        .unwrap_or_default();
    let remote = web_origin(&locator).is_some();
    let kind = if remote {
        WebSourceKind::Remote
    } else {
        WebSourceKind::LocalFile
    };
    let mode = portal
        .web_ref()
        .export
        .unwrap_or_else(|| kind.default_export());
    let packaged = assets.web_page(node.id);
    let poster = assets.web_poster(node.id);

    let caption = match (mode, packaged, remote) {
        (WebExport::Iframe, Some(asset), _) => {
            html.push_str("<iframe src=\"");
            html.push_str(&escape_attr(&asset.url));
            // Scripts, because a dashboard is scripts; same-origin, because it
            // must read its own sibling data files. Nothing beyond that.
            html.push_str(
                "\" sandbox=\"allow-scripts allow-same-origin\" loading=\"lazy\" \
                 style=\"width:100%;height:100%;border:0;display:block\" title=\"",
            );
            html.push_str(&escape_attr(&portal.title));
            html.push_str("\"></iframe>");
            format!("Packaged from {}", asset.origin)
        }
        (WebExport::Iframe, None, true) => {
            html.push_str("<iframe src=\"");
            html.push_str(&escape_attr(&locator));
            html.push_str(
                "\" sandbox=\"allow-scripts\" loading=\"lazy\" \
                 style=\"width:100%;height:100%;border:0;display:block\" title=\"",
            );
            html.push_str(&escape_attr(&portal.title));
            html.push_str("\"></iframe>");
            format!("Loads live from {locator}")
        }
        _ => {
            // Poster + pointer. An unbound or unavailable portal exports its
            // state card rather than an empty rectangle (P1.portal.export-honesty).
            if !locator.is_empty() {
                html.push_str("<a href=\"");
                html.push_str(&escape_attr(&locator));
                html.push_str(
                    "\" target=\"_blank\" rel=\"noreferrer noopener\" \
                               style=\"display:block;width:100%;height:100%\">",
                );
            }
            match poster {
                Some(url) => {
                    html.push_str("<img src=\"");
                    html.push_str(&escape_attr(url));
                    html.push_str("\" alt=\"");
                    html.push_str(&escape_attr(&portal.title));
                    html.push_str(
                        "\" style=\"width:100%;height:100%;object-fit:cover;display:block\">",
                    );
                }
                None => {
                    html.push_str(
                        "<div style=\"display:flex;align-items:center;justify-content:center;\
                         width:100%;height:100%;color:rgba(228,230,235,0.85);\
                         font:14px system-ui,sans-serif;text-align:center\">",
                    );
                    html.push_str(&escape_html(if locator.is_empty() {
                        "Web portal — no page was bound"
                    } else {
                        "Web portal — no capture was available"
                    }));
                    html.push_str("</div>");
                }
            }
            if !locator.is_empty() {
                html.push_str("</a>");
            }
            if locator.is_empty() {
                "Unbound web portal".to_string()
            } else if remote {
                format!("Live page, captured from {locator}")
            } else {
                format!("Captured from {locator}")
            }
        }
    };

    html.push_str(
        "<div class=\"portal-caption\" style=\"position:absolute;left:0;right:0;bottom:0;\
         padding:4px 8px;background:rgba(8,10,14,0.72);color:rgba(214,222,236,0.9);\
         font:11px system-ui,sans-serif\">",
    );
    html.push_str(&escape_html(&caption));
    html.push_str("</div>");
    html.push_str("</div>\n");
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

    let mut style = geometry_style(rel, node.rotation_deg);
    append_opacity(&mut style, node.opacity);
    append_clip(&mut style, node, rel);
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
        ShapeKind::Path => render_path(html, node, shape, rel),
    }
}

fn render_rect_shape(
    html: &mut String,
    node: &Node,
    shape: &slate_doc::scene::ShapeNode,
    rel: WorldRect,
    ellipse: bool,
) {
    let mut style = geometry_style(rel, node.rotation_deg);
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

    let mut wrap = geometry_style(rel, node.rotation_deg);
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

fn render_path(
    html: &mut String,
    node: &Node,
    shape: &slate_doc::scene::ShapeNode,
    rel: WorldRect,
) {
    let path = match shape.path.as_ref() {
        Some(p) if !p.is_empty() => p,
        _ => return,
    };

    let w = rel.w;
    let h = rel.h;
    let d = path_data_d(path, w, h);
    let fill_css = shape
        .fill
        .map(|f| f.css())
        .unwrap_or_else(|| "none".to_string());

    let mut wrap = geometry_style(rel, node.rotation_deg);
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
    html.push_str("\" style=\"display:block;overflow:visible\">");

    match shape.stroke.profile {
        WidthProfile::Uniform => {
            push_path_open(html, &d, &fill_css, path.fill_rule);
            if shape.stroke.is_none() {
                html.push_str(" stroke=\"none\"");
            } else {
                html.push_str(" stroke=\"");
                html.push_str(&shape.stroke.color.css());
                html.push_str("\" stroke-width=\"");
                html.push_str(&fmt_px(shape.stroke.width));
                html.push_str("\" stroke-linecap=\"");
                html.push_str(stroke_cap_css(shape.stroke.cap));
                html.push_str("\" stroke-linejoin=\"");
                html.push_str(stroke_join_css(shape.stroke.join));
                html.push('"');
                if let Some(dash) = line_dash_attrs(&shape.stroke) {
                    html.push_str(" stroke-dasharray=\"");
                    html.push_str(dash);
                    html.push('"');
                }
                if shape.stroke.dash == Dash::Dotted {
                    html.push_str(" stroke-linecap=\"round\"");
                }
            }
            html.push_str("></path>");
        }
        WidthProfile::Taper { start, end } => {
            if shape.fill.is_some() && path.closed {
                push_path_open(html, &d, &fill_css, path.fill_rule);
                html.push_str(" stroke=\"none\"></path>");
            }
            if !shape.stroke.is_none() {
                let bez = path_data_to_bez(path, w, h);
                let style = StrokeStyle {
                    width: shape.stroke.width,
                    cap: ink_cap(shape.stroke.cap),
                    join: ink_join(shape.stroke.join),
                    taper: Some((start, end)),
                    dash: stroke_dash_ink(&shape.stroke),
                };
                let outline = vector_ink::stroke_outline(&bez, &style, 0.25);
                let outline_d = bezpath_to_d(&outline);
                if !outline_d.is_empty() {
                    push_path_open(
                        html,
                        &outline_d,
                        &shape.stroke.color.css(),
                        PathFillRule::NonZero,
                    );
                    html.push_str(" stroke=\"none\"></path>");
                }
            }
        }
    }

    html.push_str("</svg></div>\n");
}

fn push_path_open(html: &mut String, d: &str, fill: &str, rule: PathFillRule) {
    html.push_str("<path d=\"");
    html.push_str(d);
    html.push_str("\" fill=\"");
    html.push_str(fill);
    html.push('"');
    if matches!(rule, PathFillRule::EvenOdd) {
        html.push_str(" fill-rule=\"evenodd\"");
    }
}

fn denorm_pt(p: [f32; 2], w: f32, h: f32) -> (f32, f32) {
    (p[0] * w, p[1] * h)
}

fn path_data_d(path: &PathData, w: f32, h: f32) -> String {
    let mut d = String::new();
    let (x, y) = denorm_pt(path.start, w, h);
    d.push_str("M ");
    d.push_str(&fmt_px(x));
    d.push(' ');
    d.push_str(&fmt_px(y));
    for seg in &path.segs {
        match seg {
            PathSeg::Line { to } => {
                let (x, y) = denorm_pt(*to, w, h);
                d.push_str(" L ");
                d.push_str(&fmt_px(x));
                d.push(' ');
                d.push_str(&fmt_px(y));
            }
            PathSeg::Quad { ctrl, to } => {
                let (cx, cy) = denorm_pt(*ctrl, w, h);
                let (x, y) = denorm_pt(*to, w, h);
                d.push_str(" Q ");
                d.push_str(&fmt_px(cx));
                d.push(' ');
                d.push_str(&fmt_px(cy));
                d.push(' ');
                d.push_str(&fmt_px(x));
                d.push(' ');
                d.push_str(&fmt_px(y));
            }
            PathSeg::Cubic { c1, c2, to } => {
                let (c1x, c1y) = denorm_pt(*c1, w, h);
                let (c2x, c2y) = denorm_pt(*c2, w, h);
                let (x, y) = denorm_pt(*to, w, h);
                d.push_str(" C ");
                d.push_str(&fmt_px(c1x));
                d.push(' ');
                d.push_str(&fmt_px(c1y));
                d.push(' ');
                d.push_str(&fmt_px(c2x));
                d.push(' ');
                d.push_str(&fmt_px(c2y));
                d.push(' ');
                d.push_str(&fmt_px(x));
                d.push(' ');
                d.push_str(&fmt_px(y));
            }
        }
    }
    if path.closed {
        d.push_str(" Z");
    }
    for extra in &path.extra {
        let (x, y) = denorm_pt(extra.start, w, h);
        d.push_str(" M ");
        d.push_str(&fmt_px(x));
        d.push(' ');
        d.push_str(&fmt_px(y));
        for seg in &extra.segs {
            match seg {
                PathSeg::Line { to } => {
                    let (x, y) = denorm_pt(*to, w, h);
                    d.push_str(" L ");
                    d.push_str(&fmt_px(x));
                    d.push(' ');
                    d.push_str(&fmt_px(y));
                }
                PathSeg::Quad { ctrl, to } => {
                    let (cx, cy) = denorm_pt(*ctrl, w, h);
                    let (x, y) = denorm_pt(*to, w, h);
                    d.push_str(" Q ");
                    d.push_str(&fmt_px(cx));
                    d.push(' ');
                    d.push_str(&fmt_px(cy));
                    d.push(' ');
                    d.push_str(&fmt_px(x));
                    d.push(' ');
                    d.push_str(&fmt_px(y));
                }
                PathSeg::Cubic { c1, c2, to } => {
                    let (c1x, c1y) = denorm_pt(*c1, w, h);
                    let (c2x, c2y) = denorm_pt(*c2, w, h);
                    let (x, y) = denorm_pt(*to, w, h);
                    d.push_str(" C ");
                    d.push_str(&fmt_px(c1x));
                    d.push(' ');
                    d.push_str(&fmt_px(c1y));
                    d.push(' ');
                    d.push_str(&fmt_px(c2x));
                    d.push(' ');
                    d.push_str(&fmt_px(c2y));
                    d.push(' ');
                    d.push_str(&fmt_px(x));
                    d.push(' ');
                    d.push_str(&fmt_px(y));
                }
            }
        }
        if extra.closed {
            d.push_str(" Z");
        }
    }
    d
}

fn path_data_to_bez(path: &PathData, w: f32, h: f32) -> BezPath {
    let mut bez = BezPath::new();
    let dn = |p: [f32; 2]| {
        let (x, y) = denorm_pt(p, w, h);
        Point::new(x as f64, y as f64)
    };
    bez.move_to(dn(path.start));
    for seg in &path.segs {
        match seg {
            PathSeg::Line { to } => bez.line_to(dn(*to)),
            PathSeg::Quad { ctrl, to } => bez.quad_to(dn(*ctrl), dn(*to)),
            PathSeg::Cubic { c1, c2, to } => bez.curve_to(dn(*c1), dn(*c2), dn(*to)),
        }
    }
    if path.closed {
        bez.close_path();
    }
    bez
}

fn bezpath_to_d(bez: &BezPath) -> String {
    let mut d = String::new();
    for el in bez.elements() {
        match el {
            PathEl::MoveTo(p) => {
                d.push_str("M ");
                d.push_str(&fmt_px(p.x as f32));
                d.push(' ');
                d.push_str(&fmt_px(p.y as f32));
            }
            PathEl::LineTo(p) => {
                d.push_str(" L ");
                d.push_str(&fmt_px(p.x as f32));
                d.push(' ');
                d.push_str(&fmt_px(p.y as f32));
            }
            PathEl::QuadTo(p1, p2) => {
                d.push_str(" Q ");
                d.push_str(&fmt_px(p1.x as f32));
                d.push(' ');
                d.push_str(&fmt_px(p1.y as f32));
                d.push(' ');
                d.push_str(&fmt_px(p2.x as f32));
                d.push(' ');
                d.push_str(&fmt_px(p2.y as f32));
            }
            PathEl::CurveTo(p1, p2, p3) => {
                d.push_str(" C ");
                d.push_str(&fmt_px(p1.x as f32));
                d.push(' ');
                d.push_str(&fmt_px(p1.y as f32));
                d.push(' ');
                d.push_str(&fmt_px(p2.x as f32));
                d.push(' ');
                d.push_str(&fmt_px(p2.y as f32));
                d.push(' ');
                d.push_str(&fmt_px(p3.x as f32));
                d.push(' ');
                d.push_str(&fmt_px(p3.y as f32));
            }
            PathEl::ClosePath => d.push_str(" Z"),
        }
    }
    d
}

fn stroke_cap_css(cap: StrokeCap) -> &'static str {
    match cap {
        StrokeCap::Butt => "butt",
        StrokeCap::Round => "round",
        StrokeCap::Square => "square",
    }
}

fn stroke_join_css(join: StrokeJoin) -> &'static str {
    match join {
        StrokeJoin::Miter => "miter",
        StrokeJoin::Round => "round",
        StrokeJoin::Bevel => "bevel",
    }
}

fn ink_cap(cap: StrokeCap) -> Cap {
    match cap {
        StrokeCap::Butt => Cap::Butt,
        StrokeCap::Round => Cap::Round,
        StrokeCap::Square => Cap::Square,
    }
}

fn ink_join(join: StrokeJoin) -> Join {
    match join {
        StrokeJoin::Miter => Join::Miter,
        StrokeJoin::Round => Join::Round,
        StrokeJoin::Bevel => Join::Bevel,
    }
}

/// Dash pattern for `vector_ink` — same on/off lengths as `line_dash_attrs`.
fn stroke_dash_ink(stroke: &slate_doc::scene::Stroke) -> Option<(Vec<f32>, f32)> {
    match stroke.dash {
        Dash::Solid => None,
        Dash::Dashed => Some((vec![12.0, 8.0], 0.0)),
        Dash::Dotted => Some((vec![2.0, 6.0], 0.0)),
    }
}

/// Faint wires render at 40% opacity in both interpreters.
const FAINT_OPACITY: f32 = 0.4;
/// Connector label font size (world units) — labels have no size in the
/// model yet, so both interpreters pin the same constant.
const CONNECTOR_LABEL_SIZE: f32 = 14.0;

fn render_connector(
    html: &mut String,
    scene: &Scene,
    node: &Node,
    conn: &ConnectorNode,
    origin_x: f32,
    origin_y: f32,
    routing: WireRouting,
) {
    // Geometry is derived from the *current* rects of anchored nodes.
    // Hidden anchor nodes resolve to nothing: the wire is skipped until the
    // node is shown again (simplest per the scene-flags spec).
    let Some(path) = connector_route_in_scene(scene, Some(node.id), &conn.a, &conn.b, routing)
    else {
        return;
    };

    // Position the wrapper at the curve AABB, padded so stroke width and
    // arrowheads survive even a degenerate (straight axis-aligned) box.
    let pad = conn.stroke.width.max(1.0) * 0.5 + arrow_len(conn) + 2.0;
    let aabb = path.aabb();
    let boxed = WorldRect::new(
        aabb.x - pad,
        aabb.y - pad,
        aabb.w + pad * 2.0,
        aabb.h + pad * 2.0,
    );
    let rel = boxed.translated(-origin_x, -origin_y);
    let local = |p: [f32; 2]| (p[0] - boxed.x, p[1] - boxed.y);

    let mut wrap = geometry_style(rel, node.rotation_deg);
    let opacity = match conn.display {
        WireDisplay::Faint => node.opacity * FAINT_OPACITY,
        WireDisplay::Default => node.opacity,
    };
    append_opacity(&mut wrap, opacity);
    wrap.push_str("overflow:visible;background:transparent;");

    html.push_str("<div class=\"node\" style=\"");
    html.push_str(&wrap);
    html.push_str("\"><svg width=\"");
    html.push_str(&fmt_px(rel.w));
    html.push_str("\" height=\"");
    html.push_str(&fmt_px(rel.h));
    html.push_str("\" viewBox=\"0 0 ");
    html.push_str(&fmt_px(rel.w));
    html.push(' ');
    html.push_str(&fmt_px(rel.h));
    html.push_str("\" style=\"display:block;overflow:visible\">");

    // The wire itself.
    let d = svg_d_for_path(&path, &local);
    push_path_open(html, &d, "none", PathFillRule::NonZero);
    html.push_str(" stroke=\"");
    html.push_str(&conn.stroke.color.css());
    html.push_str("\" stroke-width=\"");
    html.push_str(&fmt_px(conn.stroke.width));
    html.push_str("\" stroke-linecap=\"");
    html.push_str(stroke_cap_css(conn.stroke.cap));
    html.push('"');
    if let Some(dash) = line_dash_attrs(&conn.stroke) {
        html.push_str(" stroke-dasharray=\"");
        html.push_str(dash);
        html.push('"');
    }
    html.push_str("></path>");

    // Arrowheads: small filled triangles oriented to the end tangents,
    // computed inline (the writer builds elements directly).
    if conn.arrow_a {
        push_arrow_head(html, conn, local(path.start()), path.start_dir());
    }
    if conn.arrow_b {
        push_arrow_head(html, conn, local(path.end()), path.end_dir());
    }

    if let Some(label) = conn.label.as_deref().filter(|l| !l.is_empty()) {
        let (mx, my) = local(path.midpoint());
        html.push_str("<text x=\"");
        html.push_str(&fmt_px(mx));
        html.push_str("\" y=\"");
        html.push_str(&fmt_px(my));
        html.push_str("\" text-anchor=\"middle\" dominant-baseline=\"middle\" fill=\"");
        html.push_str(&conn.stroke.color.css());
        html.push_str("\" style=\"font:");
        html.push_str(&fmt_px(CONNECTOR_LABEL_SIZE));
        html.push_str("px ");
        html.push_str(slate_doc::scene::FontChoice::Sans.css_stack());
        html.push_str("\">");
        html.push_str(&escape_html(label));
        html.push_str("</text>");
    }

    html.push_str("</svg></div>\n");
}

fn svg_d_for_path(path: &ConnectorPath, local: &impl Fn([f32; 2]) -> (f32, f32)) -> String {
    match path {
        ConnectorPath::Bezier(bez) => {
            let (p0x, p0y) = local(bez.p0);
            let (c1x, c1y) = local(bez.c1);
            let (c2x, c2y) = local(bez.c2);
            let (p3x, p3y) = local(bez.p3);
            format!(
                "M {} {} C {} {} {} {} {} {}",
                fmt_px(p0x),
                fmt_px(p0y),
                fmt_px(c1x),
                fmt_px(c1y),
                fmt_px(c2x),
                fmt_px(c2y),
                fmt_px(p3x),
                fmt_px(p3y),
            )
        }
        ConnectorPath::Orthogonal(pts) => {
            let cmds = filleted_polyline(pts, ORTHO_CORNER_RADIUS);
            let mut d = String::new();
            for cmd in cmds {
                match cmd {
                    PathCmd::Move(p) => {
                        let (x, y) = local(p);
                        d.push_str(&format!("M {} {} ", fmt_px(x), fmt_px(y)));
                    }
                    PathCmd::Line(p) => {
                        let (x, y) = local(p);
                        d.push_str(&format!("L {} {} ", fmt_px(x), fmt_px(y)));
                    }
                    PathCmd::Cubic { c1, c2, to } => {
                        let (x1, y1) = local(c1);
                        let (x2, y2) = local(c2);
                        let (x, y) = local(to);
                        d.push_str(&format!(
                            "C {} {} {} {} {} {} ",
                            fmt_px(x1),
                            fmt_px(y1),
                            fmt_px(x2),
                            fmt_px(y2),
                            fmt_px(x),
                            fmt_px(y)
                        ));
                    }
                }
            }
            d
        }
    }
}

fn arrow_len(conn: &ConnectorNode) -> f32 {
    (conn.stroke.width * 4.0).max(10.0)
}

/// One filled triangle: tip at the endpoint, base back along `into_curve`
/// (the unit tangent pointing from the endpoint into the curve).
fn push_arrow_head(html: &mut String, conn: &ConnectorNode, tip: (f32, f32), into_curve: [f32; 2]) {
    let len = arrow_len(conn);
    let half_w = len * 0.4;
    let base = (tip.0 + into_curve[0] * len, tip.1 + into_curve[1] * len);
    let perp = [-into_curve[1], into_curve[0]];
    let b1 = (base.0 + perp[0] * half_w, base.1 + perp[1] * half_w);
    let b2 = (base.0 - perp[0] * half_w, base.1 - perp[1] * half_w);
    let d = format!(
        "M {} {} L {} {} L {} {} Z",
        fmt_px(tip.0),
        fmt_px(tip.1),
        fmt_px(b1.0),
        fmt_px(b1.1),
        fmt_px(b2.0),
        fmt_px(b2.1),
    );
    push_path_open(html, &d, &conn.stroke.color.css(), PathFillRule::NonZero);
    html.push_str(" stroke=\"none\"></path>");
}

fn render_text(html: &mut String, node: &Node, text: &slate_doc::scene::TextNode, rel: WorldRect) {
    let mut style = geometry_style(rel, node.rotation_deg);
    append_opacity(&mut style, node.opacity);
    append_clip(&mut style, node, rel);
    if let Some(fill) = text.fill {
        style.push_str("background:");
        style.push_str(&fill.css());
        style.push(';');
    }
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

fn geometry_style(rect: WorldRect, rotation_deg: f32) -> String {
    let mut style = format!(
        "left:{}px;top:{}px;width:{}px;height:{}px;",
        fmt_px(rect.x),
        fmt_px(rect.y),
        fmt_px(rect.w),
        fmt_px(rect.h),
    );
    if rotation_deg.abs() > f32::EPSILON {
        use std::fmt::Write;
        let _ = write!(
            style,
            "transform:rotate({:.3}deg);transform-origin:center center;",
            rotation_deg
        );
    }
    style
}

fn append_clip(style: &mut String, node: &Node, rel: WorldRect) {
    let Some(clip) = &node.clip else {
        return;
    };
    let d = path_data_d(clip, rel.w, rel.h);
    if matches!(clip.fill_rule, PathFillRule::EvenOdd) {
        style.push_str("clip-path:path(evenodd, '");
        style.push_str(&d);
        style.push_str("');clip-rule:evenodd;");
    } else {
        style.push_str("clip-path:path('");
        style.push_str(&d);
        style.push_str("');");
    }
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
    fn status_board_export_contains_layout_caption() {
        use slate_doc::scene::{
            NodeKind, PortalNode, STATUS_PORTAL_DEFAULT_H, STATUS_PORTAL_DEFAULT_W,
        };
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/status-board/tests/fixtures/project-state.json");
        let mut portal = PortalNode::unbound_status_board("Status Board");
        portal.source = Some(slate_doc::scene::SourceUri {
            locator: fixture.to_string_lossy().into_owned(),
        });
        let mut doc = SlateDoc::new("status export");
        let node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, STATUS_PORTAL_DEFAULT_W, STATUS_PORTAL_DEFAULT_H),
            NodeKind::Portal(portal),
        );
        doc.scene.nodes.push(node);
        let html = render_html(&doc, &AssetMap::default());
        assert!(html.contains("STATUS BOARD"), "{html}");
        assert!(html.contains("664e2e1"), "{html}");
    }

    #[test]
    fn unbound_status_board_exports_a_state_card() {
        use slate_doc::scene::{
            NodeKind, PortalNode, STATUS_PORTAL_DEFAULT_H, STATUS_PORTAL_DEFAULT_W,
        };
        let mut doc = SlateDoc::new("unbound status");
        let node = doc.scene.build_node(
            WorldRect::new(0.0, 0.0, STATUS_PORTAL_DEFAULT_W, STATUS_PORTAL_DEFAULT_H),
            NodeKind::Portal(PortalNode::unbound_status_board("Status Board")),
        );
        doc.scene.nodes.push(node);
        let html = render_html(&doc, &AssetMap::default());
        assert!(html.contains("Choose status snapshot"), "{html}");
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
