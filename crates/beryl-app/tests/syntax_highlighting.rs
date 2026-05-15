#[path = "../src/shell/syntax_highlighting.rs"]
mod syntax_highlighting;

use syntax_highlighting::{
    SyntaxHighlight, SyntaxLanguage, SyntaxTokenRole, highlight_syntax, normalize_syntax_language,
};

#[test]
fn resolves_markdown_labels_and_plain_fallbacks() {
    for label in [
        "markdown",
        "Markdown",
        "md",
        "mdown",
        "mkd",
        "mkdn",
        "gfm",
        "`md`",
        "\"markdown\"",
        "'gfm'",
    ] {
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
        Some("markdownish"),
        Some("`"),
    ] {
        assert_eq!(normalize_syntax_language(label), None);
        assert!(highlight_syntax("# title", label).is_plain());
    }
}

#[test]
fn resolves_config_language_labels_conservatively() {
    let cases: &[(SyntaxLanguage, &[&str])] = &[
        (
            SyntaxLanguage::Json,
            &["json", "JSON", "json linenos", "`json`", "\"json\""],
        ),
        (
            SyntaxLanguage::Jsonl,
            &["jsonl", "JSONL", "ndjson", "ndjson compact", "'jsonl'"],
        ),
        (
            SyntaxLanguage::Toml,
            &["toml", "TOML", "toml editable", "`toml`"],
        ),
        (
            SyntaxLanguage::WindowsIni,
            &["ini", "INI", "ini windows", "\"ini\""],
        ),
    ];

    for (expected, labels) in cases {
        for label in *labels {
            assert_eq!(
                normalize_syntax_language(Some(label)),
                Some(*expected),
                "{label} should resolve to {}",
                expected.label()
            );
        }
    }

    for label in [
        "json5",
        "jsonc",
        "jsonlines",
        "tom",
        "tml",
        "conf",
        "cfg",
        "windows-ini",
        "powershell",
    ] {
        assert_eq!(normalize_syntax_language(Some(label)), None);
        assert!(highlight_syntax("{}", Some(label)).is_plain());
    }
}

#[test]
fn registered_config_languages_do_not_use_plain_fallback() {
    for (label, language) in [
        ("json", SyntaxLanguage::Json),
        ("jsonl", SyntaxLanguage::Jsonl),
        ("toml", SyntaxLanguage::Toml),
        ("ini", SyntaxLanguage::WindowsIni),
    ] {
        let highlight = highlight_syntax("", Some(label));

        assert_eq!(highlight.language(), Some(language));
        assert!(highlight.tokens().is_empty());
        assert!(!highlight.is_plain());
    }
}

#[test]
fn highlights_strict_json_source_tokens() {
    let source = r#"{
  "outer": {
    "inner": "line\nvalue",
    "items": [1, -2.5e+3, true, false, null]
  }
}"#;

    let highlight = highlight_syntax(source, Some("json"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Json));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxStructuralPunctuation,
        "{",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxStructuralPunctuation,
        ":",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "outer");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "inner");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "items");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "line");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxEscape, "\\n");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "value");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNumber, "1");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNumber, "-2.5e+3");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxBoolean, "true");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxBoolean, "false");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNull, "null");
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn partial_json_preserves_recoverable_source_ranges() {
    let source = r#"{"outer": {"inner": "value", "dangling": "#;

    let highlight = highlight_syntax(source, Some("json"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Json));
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "outer");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "inner");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "dangling");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "value");
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn strict_json_marks_jsonc_and_json5_only_constructs_as_errors() {
    for source in [
        r#"{"a": 1 // no comments
}"#,
        r#"{"a": 1,}"#,
        r#"{a: 1}"#,
        r#"{'a': 1}"#,
        r#"{"a": 0x10}"#,
        r#"{"a": +1}"#,
        r#"{"a": .5}"#,
    ] {
        let highlight = highlight_syntax(source, Some("json"));

        assert_eq!(highlight.language(), Some(SyntaxLanguage::Json));
        assert_role_present(&highlight, SyntaxTokenRole::SyntaxError);
        assert!(
            !highlight
                .tokens()
                .iter()
                .any(|token| token.role() == SyntaxTokenRole::SyntaxComment),
            "strict JSON should not classify rejected comments as comments"
        );
        assert_all_ranges_slice_source(&highlight, source);
    }
}

#[test]
fn highlights_jsonl_records_independently() {
    let source = concat!(
        r#"{"first": 1}"#,
        "\n",
        "\n",
        r#"{bad: 2}"#,
        "\n",
        r#"[true, null]"#,
        "\n",
        r#""tail\nvalue""#,
    );

    let highlight = highlight_syntax(source, Some("jsonl"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Jsonl));
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "first");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNumber, "1");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxError, "bad");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxBoolean, "true");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNull, "null");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "tail");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxEscape, "\\n");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "value");
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn jsonl_offsets_crlf_record_ranges_into_original_source() {
    let source = concat!(r#"{"a":1}"#, "\r\n", r#"{"b":2}"#, "\r\n", "true");

    let highlight = highlight_syntax(source, Some("ndjson"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Jsonl));
    assert_has_at(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxKey,
        "b",
        source.find("\"b\"").expect("source should contain b key") + 1,
    );
    assert_has_at(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxBoolean,
        "true",
        source.find("true").expect("source should contain true"),
    );
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn jsonl_preserves_trailing_partial_line_and_scalar_values() {
    let source = concat!("1\n", "true\n", "null\n", r#"{"partial": "#);

    let highlight = highlight_syntax(source, Some("jsonl"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Jsonl));
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNumber, "1");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxBoolean, "true");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNull, "null");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "partial");
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn highlights_toml_source_tokens() {
    let source = concat!(
        "# config\n",
        "title = \"Beryl\"\n",
        "count = 42\n",
        "enabled = true\n",
        "launched = 2026-05-15T12:30:00Z\n",
        "escaped = \"line\\nvalue\"\n",
        "names = [\"one\", \"two\"]\n",
        "inline = { key = \"value\", nested = 1 }\n",
        "\n",
        "[workspace.settings]\n",
        "path = 'C:\\Temp'\n",
        "\n",
        "[[workspace.items]]\n",
        "name = \"\"\"multi\n",
        "line\"\"\"\n",
    );

    let highlight = highlight_syntax(source, Some("toml"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Toml));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxComment,
        "# config",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "title");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxAssignment, "=");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "Beryl");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNumber, "42");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxBoolean, "true");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxDateTime,
        "2026-05-15T12:30:00Z",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "line");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxEscape, "\\n");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "value");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxStructuralPunctuation,
        "[",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxStructuralPunctuation,
        "{",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxStructuralPunctuation,
        ".",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "workspace",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "settings",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "items",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "key");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "nested");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxString,
        "C:\\Temp",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxString,
        "multi\nline",
    );
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn toml_partial_and_invalid_source_keeps_recoverable_ranges() {
    let source = concat!(
        "[partial\n",
        "valid = true\n",
        "broken = \"unterminated\n",
        "later = [1, 2]\n",
    );

    let highlight = highlight_syntax(source, Some("toml"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::Toml));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "partial",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "valid");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxBoolean, "true");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "broken");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "later");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxNumber, "1");
    assert_role_present(&highlight, SyntaxTokenRole::SyntaxError);
    assert_all_ranges_slice_source(&highlight, source);
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

    for label in [None, Some(""), Some("rust"), Some("markdownish")] {
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

fn assert_has_at(
    highlight: &SyntaxHighlight,
    source: &str,
    role: SyntaxTokenRole,
    expected_text: &str,
    expected_start: usize,
) {
    assert!(
        highlight.tokens().iter().any(|token| {
            token.role() == role
                && token.range().start == expected_start
                && token.source_text(source) == Some(expected_text)
        }),
        "expected {role:?} token for {expected_text:?} at {expected_start}; got {:?}",
        role_texts(highlight, source)
    );
}

fn assert_role_present(highlight: &SyntaxHighlight, role: SyntaxTokenRole) {
    assert!(
        highlight.tokens().iter().any(|token| token.role() == role),
        "expected {role:?} token; got {:?}",
        highlight
            .tokens()
            .iter()
            .map(|token| token.role())
            .collect::<Vec<_>>()
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
