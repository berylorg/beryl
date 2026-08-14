#[path = "../src/shell/syntax_highlighting.rs"]
mod syntax_highlighting;

use syntax_highlighting::{SyntaxHighlight, SyntaxLanguage, SyntaxTokenRole, highlight_syntax};

#[test]
fn highlights_base_windows_ini_source_tokens() {
    let source = concat!(
        "; global comment\n",
        "[display]\n",
        "name=Beryl\n",
        "empty=\n",
        "duplicate=first\n",
        "duplicate=second\n",
        "unicode=Žluťoučký kůň\n",
    );

    let highlight = highlight_syntax(source, Some("ini"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::WindowsIni));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxComment,
        "; global comment",
    );
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
        "]",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "display",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "name");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "empty");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxAssignment, "=");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "Beryl");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "first");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "second");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxString,
        "Žluťoučký kůň",
    );
    assert_count(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxKey,
        "duplicate",
        2,
    );
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn windows_ini_duplicate_keys_are_syntax_not_semantic_errors() {
    let source = concat!("[section]\n", "key=one\n", "key=two\n");

    let highlight = highlight_syntax(source, Some("ini"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::WindowsIni));
    assert_count(&highlight, source, SyntaxTokenRole::SyntaxKey, "key", 2);
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "one");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "two");
    assert_no_role(&highlight, SyntaxTokenRole::SyntaxError);
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn windows_ini_does_not_infer_registry_semantics() {
    let source = concat!(
        "[HKEY_CURRENT_USER\\Software\\Beryl]\n",
        "\"DisplayName\"=\"Beryl\"\n",
    );

    let highlight = highlight_syntax(source, Some("ini"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::WindowsIni));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "HKEY_CURRENT_USER\\Software\\Beryl",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxKey,
        "\"DisplayName\"",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxString,
        "\"Beryl\"",
    );
    assert_no_role(&highlight, SyntaxTokenRole::SyntaxError);
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn windows_ini_handles_whitespace_and_crlf_ranges() {
    let source = concat!(
        "  [ spaced ]  \r\n",
        "  key = value  \r\n",
        "  ; comment  \r\n",
        "  empty =   \r\n",
    );

    let highlight = highlight_syntax(source, Some("ini"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::WindowsIni));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "spaced",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "key");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "value");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxComment,
        "; comment",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "empty");
    assert_count(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxAssignment,
        "=",
        2,
    );
    assert_no_role(&highlight, SyntaxTokenRole::SyntaxError);
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn windows_ini_rejects_hash_comments_and_colon_assignments() {
    let source = concat!("# not a windows ini comment\n", "key: value\n", "next=ok\n",);

    let highlight = highlight_syntax(source, Some("ini"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::WindowsIni));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxError,
        "# not a windows ini comment",
    );
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxError,
        "key: value",
    );
    assert_not_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxComment,
        "# not a windows ini comment",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "next");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "ok");
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn windows_ini_malformed_lines_do_not_corrupt_following_ranges() {
    let source = concat!("[broken\n", "bare_key\n", "[ok]\n", "key=value\n");

    let highlight = highlight_syntax(source, Some("ini"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::WindowsIni));
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxError, "[broken");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxError, "bare_key");
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "ok",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "key");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "value");
    assert_all_ranges_slice_source(&highlight, source);
}

#[test]
fn windows_ini_marks_section_trailing_junk_and_empty_keys_as_errors() {
    let source = concat!("[closed] trailing\n", "=value\n", "next=ok\n");

    let highlight = highlight_syntax(source, Some("ini"));

    assert_eq!(highlight.language(), Some(SyntaxLanguage::WindowsIni));
    assert_has(
        &highlight,
        source,
        SyntaxTokenRole::SyntaxSectionHeader,
        "closed",
    );
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxError, "trailing");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxError, "=value");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxKey, "next");
    assert_has(&highlight, source, SyntaxTokenRole::SyntaxString, "ok");
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

fn assert_not_has(
    highlight: &SyntaxHighlight,
    source: &str,
    role: SyntaxTokenRole,
    expected_text: &str,
) {
    assert!(
        !highlight.tokens().iter().any(|token| {
            token.role() == role && token.source_text(source) == Some(expected_text)
        }),
        "did not expect {role:?} token for {expected_text:?}; got {:?}",
        role_texts(highlight, source)
    );
}

fn assert_count(
    highlight: &SyntaxHighlight,
    source: &str,
    role: SyntaxTokenRole,
    expected_text: &str,
    expected_count: usize,
) {
    let actual_count = highlight
        .tokens()
        .iter()
        .filter(|token| token.role() == role && token.source_text(source) == Some(expected_text))
        .count();

    assert_eq!(
        actual_count,
        expected_count,
        "expected {expected_count} {role:?} token(s) for {expected_text:?}; got {:?}",
        role_texts(highlight, source)
    );
}

fn assert_no_role(highlight: &SyntaxHighlight, role: SyntaxTokenRole) {
    assert!(
        !highlight.tokens().iter().any(|token| token.role() == role),
        "did not expect {role:?}; got {:?}",
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
