#[path = "../src/shell/syntax_highlighting.rs"]
mod syntax_highlighting;

use syntax_highlighting::{SyntaxHighlight, SyntaxLanguage, SyntaxTokenRole, highlight_syntax};

#[test]
fn toml_invalid_non_ascii_escape_ranges_remain_utf8_safe() {
    let source = concat!(
        "unknown = \"\\\u{00E9}tail\"\n",
        "unicode = \"\\u123\u{00E9}tail\"\n",
        "next = \"ok\"\n",
    );

    let highlight = highlight_syntax(source, Some("toml"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Toml));
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "unknown");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxEscape,
        "\\\u{00E9}",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "unicode");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "next");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "ok");
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn toml_valid_escapes_multiline_and_non_ascii_content_stay_highlighted() {
    let source = concat!(
        "simple = \"line\\nvalue\"\n",
        "unicode = \"\\u00E9\"\n",
        "multi = \"\"\"first\\nsecond\"\"\"\n",
        "non_ascii = \"caf\u{00E9}\"\n",
    );

    let highlight = highlight_syntax(source, Some("toml"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Toml));
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxEscape, "\\n");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxEscape, "\\u00E9");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "line");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "value");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "first");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "second");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxString,
        "caf\u{00E9}",
    );
    assert_all_ranges_slice_source(&highlight, source);
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
