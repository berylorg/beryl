#[path = "../src/shell/transcript_markdown.rs"]
mod transcript_markdown;

use transcript_markdown::{
    Inline, InlineRenderLine, InlineRenderRole, InlineRenderStyle, UnsupportedKind,
    inline_render_lines,
};

fn style(role: InlineRenderRole) -> InlineRenderStyle {
    InlineRenderStyle {
        role,
        link: false,
        emphasis: role == InlineRenderRole::Emphasis,
        strong: role == InlineRenderRole::StrongEmphasis,
        fallback: false,
        atom: false,
    }
}

#[test]
fn inline_render_plan_preserves_explicit_line_breaks() {
    let lines = inline_render_lines(&[
        Inline::text("alpha\nbeta\r\ngamma"),
        Inline::soft_break(),
        Inline::text("delta"),
        Inline::hard_break(),
        Inline::text("epsilon\n"),
    ]);

    assert_eq!(lines.len(), 6);
    assert_eq!(line_text(&lines[0]), "alpha");
    assert_eq!(line_text(&lines[1]), "beta");
    assert_eq!(line_text(&lines[2]), "gamma");
    assert_eq!(line_text(&lines[3]), "delta");
    assert_eq!(line_text(&lines[4]), "epsilon");
    assert!(lines[5].fragments.is_empty());
}

#[test]
fn inline_render_plan_assigns_roles_and_keeps_link_decoration_separate() {
    let lines = inline_render_lines(&[
        Inline::text("plain "),
        Inline::emphasis(vec![
            Inline::text("em "),
            Inline::strong(vec![Inline::text("strong")]),
        ]),
        Inline::text(" "),
        Inline::link(
            "https://example.invalid",
            None,
            vec![Inline::text("link "), Inline::code("code")],
        ),
    ]);

    let fragments = &lines[0].fragments;
    assert_eq!(fragments.len(), 6);
    assert_eq!(fragments[0].text, "plain ");
    assert_eq!(fragments[0].style, style(InlineRenderRole::Conversation));
    assert_eq!(fragments[1].text, "em ");
    assert_eq!(fragments[1].style, style(InlineRenderRole::Emphasis));
    assert_eq!(fragments[2].text, "strong");
    assert_eq!(fragments[2].style, style(InlineRenderRole::StrongEmphasis));
    assert_eq!(fragments[3].text, " ");
    assert_eq!(fragments[3].style, style(InlineRenderRole::Conversation));
    assert_eq!(fragments[4].text, "link ");
    assert_eq!(
        fragments[4].style,
        InlineRenderStyle {
            role: InlineRenderRole::Conversation,
            link: true,
            emphasis: false,
            strong: false,
            fallback: false,
            atom: false,
        }
    );
    assert_eq!(fragments[5].text, "code");
    assert_eq!(
        fragments[5].style,
        InlineRenderStyle {
            role: InlineRenderRole::Code,
            link: true,
            emphasis: false,
            strong: false,
            fallback: false,
            atom: false,
        }
    );
}

#[test]
fn inline_render_plan_preserves_code_ambient_context() {
    let lines = inline_render_lines(&[
        Inline::emphasis(vec![Inline::code("em code")]),
        Inline::text(" "),
        Inline::strong(vec![Inline::code("strong code")]),
        Inline::text(" "),
        Inline::link(
            "https://example.invalid",
            None,
            vec![Inline::code("link code")],
        ),
    ]);

    let fragments = &lines[0].fragments;
    assert_eq!(fragments.len(), 5);
    assert_eq!(
        fragments[0].style,
        InlineRenderStyle {
            role: InlineRenderRole::Code,
            link: false,
            emphasis: true,
            strong: false,
            fallback: false,
            atom: false,
        }
    );
    assert_eq!(
        fragments[2].style,
        InlineRenderStyle {
            role: InlineRenderRole::Code,
            link: false,
            emphasis: false,
            strong: true,
            fallback: false,
            atom: false,
        }
    );
    assert_eq!(
        fragments[4].style,
        InlineRenderStyle {
            role: InlineRenderRole::Code,
            link: true,
            emphasis: false,
            strong: false,
            fallback: false,
            atom: false,
        }
    );
}

#[test]
fn inline_render_plan_uses_literal_fallbacks_for_non_textual_inline_nodes() {
    let lines = inline_render_lines(&[
        Inline::image(
            "diagram",
            "artifact://diagram.png",
            Some("Diagram".to_string()),
        ),
        Inline::text(" "),
        Inline::math("x + y"),
        Inline::text(" "),
        Inline::unsupported(UnsupportedKind::Html, "<span>"),
    ]);

    let fragments = &lines[0].fragments;
    let fallback_style = InlineRenderStyle {
        role: InlineRenderRole::Code,
        link: false,
        emphasis: false,
        strong: false,
        fallback: true,
        atom: false,
    };

    assert_eq!(
        fragments[0].text,
        "![diagram](artifact://diagram.png \"Diagram\")"
    );
    assert_eq!(fragments[0].style, fallback_style);
    assert_eq!(fragments[1].text, " ");
    assert_eq!(fragments[1].style, style(InlineRenderRole::Conversation));
    assert_eq!(fragments[2].text, "$$x + y$$");
    assert_eq!(fragments[2].style, fallback_style);
    assert_eq!(fragments[3].text, " ");
    assert_eq!(fragments[3].style, style(InlineRenderRole::Conversation));
    assert_eq!(fragments[4].text, "<span>");
    assert_eq!(fragments[4].style, fallback_style);
}

fn line_text(line: &InlineRenderLine) -> String {
    line.fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect()
}
