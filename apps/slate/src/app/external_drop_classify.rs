//! Browser image intent, separated from the native transfer adapter.
use super::{board_web::web_url_text, external_drop::Payload};

pub(super) enum ImageSource {
    Encoded(Vec<u8>),
    Bitmap(Vec<u8>),
    Url(String),
}

/// Image pixels or an HTML image win over the page URL browsers also publish.
/// SourceURL is only a base for a relative img src, never a placed page.
pub(super) fn classify(
    html: Option<&str>,
    urls: &[String],
    image: Option<ImageSource>,
) -> Option<Payload> {
    if let Some(image) = image {
        return Some(Payload::Image(image));
    }
    if let Some(html) = html {
        let fragment = fragment(html);
        let image_intent = tags(fragment).any(|body| tag_name(body).eq_ignore_ascii_case("img"));
        if let Some(src) = attribute(fragment, "img", "src") {
            let base = html
                .lines()
                .find_map(|line| line.strip_prefix("SourceURL:"));
            let url = url::Url::parse(&src)
                .ok()
                .or_else(|| url::Url::parse(base?.trim()).ok()?.join(&src).ok());
            if let Some(url) = url.filter(|url| matches!(url.scheme(), "http" | "https")) {
                return Some(Payload::Image(ImageSource::Url(url.into())));
            }
        }
        if image_intent {
            // blob/data sources sometimes accompany a usable direct URL.
            // An unavailable image must still never become its hosting page.
            return urls
                .iter()
                .filter_map(|url| direct_image(url))
                .next()
                .map(|url| Payload::Image(ImageSource::Url(url)));
        }
        if let Some(href) = attribute(fragment, "a", "href") {
            if let Some(url) = web_url_text(&href) {
                return Some(Payload::Url(url.to_owned()));
            }
        }
    }
    let urls: Vec<_> = urls
        .iter()
        .filter_map(|value| web_url_text(value))
        .collect();
    for value in &urls {
        if let Some(image) = direct_image(value) {
            return Some(Payload::Image(ImageSource::Url(image)));
        }
    }
    urls.first().map(|url| Payload::Url((*url).to_owned()))
}

fn direct_image(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    // Image search wrappers can carry the actual resource separately.
    for (key, value) in url.query_pairs() {
        if matches!(key.as_ref(), "imgurl" | "mediaurl") && web_url_text(&value).is_some() {
            return Some(value.into_owned());
        }
    }
    let path = std::path::Path::new(url.path());
    (slate_doc::media_kind(path) == slate_doc::MediaKind::Image).then(|| value.to_owned())
}

fn fragment(html: &str) -> &str {
    let offset = |name: &str| {
        html.lines()
            .find_map(|line| line.strip_prefix(name))?
            .trim()
            .parse::<usize>()
            .ok()
    };
    if let (Some(start), Some(end)) = (offset("StartFragment:"), offset("EndFragment:")) {
        if let Some(fragment) = html.get(start..end) {
            return fragment;
        }
    }
    if let Some((_, after)) = html.split_once("<!--StartFragment-->") {
        return after
            .split_once("<!--EndFragment-->")
            .map_or(after, |(text, _)| text);
    }
    html
}

/// A small attribute tokenizer for the browser's offered fragment, not a DOM.
fn attribute(html: &str, tag: &str, wanted: &str) -> Option<String> {
    for body in tags(html) {
        let name = tag_name(body);
        if !name.eq_ignore_ascii_case(tag) {
            continue;
        }
        let mut rest = &body[name.len()..];
        while !rest.is_empty() {
            rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
            let end = rest
                .find(|c: char| c.is_ascii_whitespace() || c == '=')
                .unwrap_or(rest.len());
            let name = &rest[..end];
            rest = rest[end..].trim_start();
            if !rest.starts_with('=') {
                if rest.is_empty() {
                    break;
                }
                continue;
            }
            rest = rest[1..].trim_start();
            let (value, tail) = if let Some(quote @ ('\'' | '"')) = rest.chars().next() {
                let end = rest[1..].find(quote)? + 1;
                (&rest[1..end], &rest[end + 1..])
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                (&rest[..end], &rest[end..])
            };
            if name.eq_ignore_ascii_case(wanted) {
                return Some(decode_entities(value));
            }
            rest = tail;
        }
    }
    None
}

fn tag_name(body: &str) -> &str {
    let end = body
        .find(|c: char| c.is_ascii_whitespace() || c == '/')
        .unwrap_or(body.len());
    &body[..end]
}

fn tags(mut html: &str) -> impl Iterator<Item = &str> {
    std::iter::from_fn(move || {
        let start = html.find('<')?;
        let body = &html[start + 1..];
        let mut quote = None;
        let end = body.char_indices().find_map(|(index, c)| {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (None, '\'' | '"') => quote = Some(c),
                (None, '>') => return Some(index),
                _ => {}
            }
            None
        })?;
        html = &body[end + 1..];
        Some(&body[..end])
    })
}

fn decode_entities(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest.find(';').filter(|end| *end <= 12).and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|n| u32::from_str_radix(n, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((c, end))
        });
        if let Some((c, end)) = decoded {
            out.push(c);
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_image_pixels_win_over_hosting_page() {
        assert!(matches!(
            classify(
                None,
                &["https://example.com/gallery".into()],
                Some(ImageSource::Encoded(vec![1]))
            ),
            Some(Payload::Image(ImageSource::Encoded(_)))
        ));
    }

    #[test]
    fn linked_img_uses_image_and_not_anchor_or_source_page() {
        let html = "SourceURL:https://example.com/gallery\r\n<!--StartFragment--><a href='https://example.com/page'><IMG alt='photo' src='/photo.webp?a=1&amp;b=2'></a><!--EndFragment-->";
        assert!(
            matches!(classify(Some(html), &[], None), Some(Payload::Image(ImageSource::Url(url)))
            if url == "https://example.com/photo.webp?a=1&b=2")
        );
    }

    #[test]
    fn browser_links_still_place_the_link_not_source_url() {
        let html = "SourceURL:https://origin.test/\n<a href=\"https://target.test/\">link</a>";
        assert!(
            matches!(classify(Some(html), &[], None), Some(Payload::Url(url)) if url == "https://target.test/")
        );
        assert!(matches!(
            classify(None, &["https://example.com/".into()], None),
            Some(Payload::Url(_))
        ));
        assert!(classify(None, &["ordinary text".into()], None).is_none());
    }

    #[test]
    fn direct_image_and_search_resource_are_pictures() {
        for url in [
            "https://cdn.test/photo.jpg?size=large",
            "https://google.test/imgres?imgurl=https%3A%2F%2Fcdn.test%2Fimage",
        ] {
            assert!(matches!(
                classify(None, &[url.into()], None),
                Some(Payload::Image(ImageSource::Url(_)))
            ));
        }
        assert!(classify(
            Some("<img src='blob:private'>"),
            &["https://page.test".into()],
            None
        )
        .is_none());
    }

    #[test]
    fn html_quotes_and_blob_fallback_preserve_image_intent() {
        assert!(
            matches!(classify(Some("<img alt='1 > 0' src='https://cdn.test/image.png'>"), &[], None),
            Some(Payload::Image(ImageSource::Url(url))) if url == "https://cdn.test/image.png")
        );
        assert!(
            matches!(classify(Some("<img src='blob:private'>"), &["https://cdn.test/image.png".into()], None),
            Some(Payload::Image(ImageSource::Url(url))) if url == "https://cdn.test/image.png")
        );
    }
}
