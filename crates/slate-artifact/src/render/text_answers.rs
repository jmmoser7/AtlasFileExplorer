//! Serialize authored answer ranges through the existing text renderer.
use super::escape_html;
use slate_doc::{
    scene::NodeId,
    text_answers::{valid_answers, TextAnswer},
};

pub(super) const CSS: &str = r#"
a[data-text-answer]{color:inherit;text-decoration:underline;cursor:pointer}
dialog.text-answer{max-width:min(560px,90vw);max-height:80vh;overflow:auto;white-space:pre-wrap;font:16px/1.5 system-ui;color:#eee;background:#222;border:1px solid #777;border-radius:12px;padding:24px}
dialog.text-answer::backdrop{background:#0008}
dialog.text-answer blockquote{margin:12px 0;color:#bbb}
"#;

pub(super) fn render_inline(html: &mut String, id: NodeId, text: &str, answers: &[TextAnswer]) {
    let mut end = 0;
    for answer in valid_answers(text, answers) {
        html.push_str(&escape_html(&text[end..answer.start]));
        html.push_str(&format!(
            "<a data-text-answer href=\"#text-answer-{}-{}\" title=\"{}\">",
            id.0,
            answer.start,
            escape_html(&format!("{}\n\n{}", answer.question, answer.answer))
        ));
        html.push_str(&escape_html(&text[answer.start..answer.end]));
        html.push_str("</a>");
        end = answer.end;
    }
    html.push_str(&escape_html(&text[end..]));
}

pub(super) fn render_answers(html: &mut String, id: NodeId, text: &str, answers: &[TextAnswer]) {
    for answer in valid_answers(text, answers) {
        html.push_str(&format!("<dialog class=\"text-answer\" id=\"text-answer-{}-{}\"><form method=\"dialog\"><button aria-label=\"Close answer\">Close</button></form><blockquote>", id.0, answer.start));
        html.push_str(&escape_html(&answer.question));
        html.push_str("</blockquote><div>");
        html.push_str(&escape_html(&answer.answer));
        html.push_str("</div></dialog>\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::text_answers::TextAnswerRef;

    #[test]
    fn unicode_ranges_escape_public_content_without_exporting_session_identity() {
        let text = "A café & tea.";
        let answer = TextAnswer::new(
            text,
            2..7,
            TextAnswerRef {
                session: "private-session".into(),
                through: 2,
            },
            "Why <this>?".into(),
            "<script>bad()</script>".into(),
        )
        .unwrap();
        let mut html = String::new();
        render_inline(&mut html, NodeId(7), text, &[answer.clone()]);
        render_answers(&mut html, NodeId(7), text, &[answer]);
        assert!(html.contains(">café</a> &amp; tea."));
        assert!(html.contains("Why &lt;this&gt;?"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("private-session"));
    }

    #[test]
    fn edited_passages_export_as_plain_text() {
        let answer = TextAnswer::new(
            "old",
            0..3,
            TextAnswerRef {
                session: "s".into(),
                through: 2,
            },
            "q".into(),
            "a".into(),
        )
        .unwrap();
        let mut html = String::new();
        render_inline(&mut html, NodeId(1), "new", &[answer.clone()]);
        render_answers(&mut html, NodeId(1), "new", &[answer]);
        assert_eq!(html, "new");
    }
}
