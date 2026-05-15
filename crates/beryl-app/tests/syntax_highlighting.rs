#[path = "../src/shell/syntax_highlighting.rs"]
mod syntax_highlighting;

use syntax_highlighting::{
    SyntaxHighlight, SyntaxLanguage, SyntaxTokenRole, highlight_syntax, normalize_syntax_language,
};

#[test]
fn resolves_markdown_labels_and_plain_fallbacks() {
    for label in ["markdown", "Markdown", "md", "mdown", "mkd", "mkdn", "gfm"] {
        assert_eq!(
            normalize_syntax_language(Some(label)),
            Some(SyntaxLanguage::Markdown),
            "{label} should resolve to markdown"
        );
    }

    assert_eq!(
        normalize_syntax_language(Some("markdown linenos")),
        Some(SyntaxLanguage::Markdown)
    );

    for label in [
        None,
        Some(""),
        Some("   "),
        Some("rust"),
        Some("mark"),
        Some("`"),
    ] {
        assert_eq!(normalize_syntax_language(label), None);
        assert!(highlight_syntax("# title", label).is_plain());
    }
}

#[test]
fn highlights_common_markdown_source_tokens() {
    let source = concat!(
        "# Title\n",
        "\n",
        "- *first* and **second** with `code`\n",
        "> [docs](https://example.invalid) and ![alt](image.png)\n",
        "---\n",
        "```markdown\n",
        "# nested\n",
        "```\n",
    );

    let highlight = highlight_syntax(source, Some("md"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Markdown));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupHeadingMarker,
        "#",
    );
    assert_has(&highlight, source, SyntaxTokenRole::MarkupListMarker, "-");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupEmphasisDelimiter,
        "*",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupStrongDelimiter,
        "**",
    );
    assert_has(&highlight, source, SyntaxTokenRole::MarkupCodeSpan, "code");
    assert_has(&highlight, source, SyntaxTokenRole::MarkupQuoteMarker, ">");
    assert_has(&highlight, source, SyntaxTokenRole::MarkupLinkText, "docs");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupLinkDestination,
        "https://example.invalid",
    );
    assert_has(&highlight, source, SyntaxTokenRole::MarkupImageMarker, "!");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupThematicBreak,
        "---",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupFenceInfo,
        "markdown",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupCodeBlock,
        "# nested",
    );
}

#[test]
fn partial_and_invalid_markdown_still_preserve_source_ranges() {
    let source = "prefix \\\\* [dangling](\n```markdown\n# partial";

    let highlight = highlight_syntax(source, Some("markdown"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Markdown));
    assert_has(&highlight, source, SyntaxTokenRole::Escape, "\\\\");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupFenceDelimiter,
        "```",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupFenceInfo,
        "markdown",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::MarkupCodeBlock,
        "# partial",
    );
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn unsupported_or_empty_labels_never_tokenize_source() {
    let source = "# not markdown\n\n```markdown\nstill plain\n```";

    for label in [
        None,
        Some(""),
        Some("rust"),
        Some("json"),
        Some("markdownish"),
    ] {
        let highlight = highlight_syntax(source, label);
        assert!(highlight.is_plain(), "{label:?} should use plain fallback");
        assert!(highlight.tokens().is_empty());
    }
}

fn assert_has(
    highlight: &SyntaxHighlight,
    source: &str,
    role: SyntaxTokenRole,
    expected_text: &str,
) {
    assert!(
        highlight.tokens().iter().any(|token| {
            token.role() == role && token.source_text(source) == Some(expected_text)
        }),
        "expected {role:?} token for {expected_text:?}; got {:?}",
        role_texts(highlight, source)
    );
}

fn assert_all_ranges_slice_source(highlight: &SyntaxHighlight, source: &str) {
    for token in highlight.tokens() {
        assert!(
            token.source_text(source).is_some(),
            "token range {:?} should slice source",
            token.range()
        );
    }
}

fn role_texts<'a>(
    highlight: &'a SyntaxHighlight,
    source: &'a str,
) -> Vec<(SyntaxTokenRole, Option<&'a str>)> {
    highlight
        .tokens()
        .iter()
        .map(|token| (token.role(), token.source_text(source)))
        .collect()
}
