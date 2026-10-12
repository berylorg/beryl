use super::super::copy_text::{CopyTextError, MAX_COPY_SOURCE_BYTES, stripped_source};

#[test]
fn continuation_spans_keep_literal_content_without_unloaded_delimiter_context() {
    use super::super::copy_text::stripped_span;
    for source in [
        "**literal**` and tail",
        "\\*literal\\*\ncontinued`",
        "[label](destination)",
    ] {
        assert_eq!(stripped_span(source, 2).unwrap(), source);
        assert_eq!(stripped_span(source, u64::MAX).unwrap(), source);
    }
    assert_eq!(stripped_span("**readable**", 1).unwrap(), "readable");
}

#[test]
fn source_markup_becomes_readable_bounded_text() {
    let source = "# Heading\r\n> quoted **strong** and *soft*\r\n- item\r\n12. item\r\n[label](https://example.test) ![alt](image.png)\r\n";
    assert_eq!(
        stripped_source(source).unwrap(),
        "Heading\r\nquoted strong and soft\r\nitem\r\nitem\r\nlabel alt\r\n"
    );
}

#[test]
fn code_content_preserves_literal_markup_and_line_endings() {
    let source =
        "`**literal**`\r\n```rust\r\n**literal**\\x\r\n```\r\n    **indented**\n\t*tabbed*\n";
    assert_eq!(
        stripped_source(source).unwrap(),
        "**literal**\r\n**literal**\\x\r\n    **indented**\n\t*tabbed*\n"
    );
}

#[test]
fn uncertain_or_unsupported_syntax_remains_readable() {
    for source in [
        "**unfinished",
        "**mismatch*",
        "```rust\nunfinished",
        "[missing](",
        "<tag>text</tag>",
        "a_word_here",
        "[nested](a(b)c)",
        "---\n",
        "***ambiguous***",
    ] {
        assert_eq!(stripped_source(source).unwrap(), source);
    }
}

#[test]
fn uncertain_multiline_inline_code_preserves_literal_source() {
    for source in [
        "`**literal**\ncontinued`",
        "`\\*literal\\*\ncontinued`",
        "`first\r\n**literal** \\*escaped\\*\r\ncontinued`",
        "`**unclosed**\n# still literal",
    ] {
        assert_eq!(stripped_source(source).unwrap(), source);
    }
}

#[test]
fn unsupported_container_fences_preserve_literal_source() {
    for source in [
        "> ```rust\n> **literal**\\x\n> ```\n",
        "- ```rust\n  **literal**\\x\n  ```\n",
        "> > ~~~\r\n> > **literal**\\x\r\n> > ~~~\r\n",
    ] {
        assert_eq!(stripped_source(source).unwrap(), source);
    }
}

#[test]
fn container_indentation_and_lazy_inline_code_keep_literal_source() {
    for source in [
        ">     **literal**\n",
        "> \t*literal*\n",
        "> >     **literal**\n",
        "- >     **literal**\n",
        "1. >     \\*literal\\*\r\n",
        "> - > \t**literal**\n",
        "> `**literal**\n*continued*`",
        "- `\\*literal\\*\n*continued*`",
    ] {
        assert_eq!(stripped_source(source).unwrap(), source);
    }
}

#[test]
fn unsupported_html_and_autolink_forms_are_opaque_source() {
    for source in [
        "<code>**literal**</code>",
        "<pre>\\*literal\\*\n**continued**</pre>",
        "<unknown>_uncertain_</unknown>",
        "<https://example.test/*literal*>",
    ] {
        assert_eq!(stripped_source(source).unwrap(), source);
    }
}

#[test]
fn ambiguous_surrounding_markup_never_consumes_inline_code_characters() {
    for source in [
        "*`*literal*`*",
        "**`**literal**`**",
        "[`x](y)`](url)",
        "![`x](y)`](url)",
    ] {
        assert_eq!(stripped_source(source).unwrap(), source);
    }
}

#[test]
fn ambiguous_link_destinations_preserve_complete_source_spelling() {
    for source in [
        "[label](a b)",
        "[**label**](a b)",
        "![alt](image.png \"title\")",
        "[label](a\tb)",
        "[label](a\\b)",
    ] {
        assert_eq!(stripped_source(source).unwrap(), source);
    }
}

#[test]
fn escapes_unicode_and_limits_preserve_source_bounds() {
    assert_eq!(
        stripped_source("\\*žluťoučký\\* _kůň_ 😀").unwrap(),
        "*žluťoučký* kůň 😀"
    );
    assert_eq!(stripped_source("").unwrap(), "");
    let at_limit = "x".repeat(MAX_COPY_SOURCE_BYTES);
    assert_eq!(stripped_source(&at_limit).unwrap(), at_limit);
    assert_eq!(
        stripped_source(&"x".repeat(MAX_COPY_SOURCE_BYTES + 1)),
        Err(CopyTextError::Capacity)
    );
    for source in [
        "[".repeat(MAX_COPY_SOURCE_BYTES),
        "* ".repeat(MAX_COPY_SOURCE_BYTES / 2),
        "😀".repeat(MAX_COPY_SOURCE_BYTES / 4),
    ] {
        let output = stripped_source(&source).unwrap();
        assert!(output.len() <= source.len());
    }
}

#[test]
fn admitted_worker_payload_preserves_markdown_and_derives_plain_text() {
    use super::{AtomicBool, Fixture};
    let fixture = Fixture::new();
    fixture.seed_projection("# Authored **source**\n", 1);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |seed| {
            let Some(crate::syndic_transcript::TranscriptProviderResponseKind::ProjectionRecords(
                records,
            )) = seed.projection_records_response
            else {
                panic!("missing source projection")
            };
            let crate::syndic_transcript::ProjectionPayload::Text { text, plain_text } =
                &records.records[0].payload
            else {
                panic!("missing text")
            };
            assert_eq!(text.as_ref(), "# Authored **source**\n");
            assert_eq!(plain_text.as_deref(), Some("Authored source\n"));
        })
        .unwrap();
    assert!(fixture.reader.retire());
}
