use std::mem::size_of;

#[cfg(feature = "test-faults")]
#[path = "projection_boundaries/fixture.rs"]
mod fixture;
#[cfg(feature = "test-faults")]
mod support;

#[cfg(feature = "test-faults")]
use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
#[cfg(feature = "test-faults")]
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, ContentRevision, SealedAssetReferenceSetProof,
    SyndicItemId,
};
#[cfg(feature = "test-faults")]
use support::{TestHome, draft_id, exact_cas::submit_prepared_current_draft, id, open, timestamp};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    AdvanceItemProjectionBuild, ComposerAtom, ComposerPayload, CreateThread,
    DraftEditHistoryPolicyV1, ItemProjectionGeneration, PreparedContent, StartItemProjectionBuild,
    SyndicPointReadLimit, SyndicStorage, TRANSCRIPT_PAGE_MAX_BYTES,
};

use syndic_storage::{
    MarkdownBlockId, MarkdownBlockKind, ProjectionPayload, ProjectionSourceRange,
    narrative::{
        NARRATIVE_FRAGMENT_MAX_BYTES, NARRATIVE_MAX_BYTES, NARRATIVE_MAX_FRAGMENTS,
        NARRATIVE_MAX_RUNS, NarrativeDisposition, NarrativeFragment, NarrativeMapping,
        NarrativeMappingKind, NarrativePreparationErrorKind, NarrativeRun, NarrativeTreatment,
        PreparedNarrative, prepare_narrative,
    },
};

fn payload(source: &str, kind: MarkdownBlockKind, ordinal: u64) -> ProjectionPayload {
    ProjectionPayload::InlineMarkdown {
        block_id: MarkdownBlockId::from_bytes([7; 32]),
        block_kind: kind,
        span_ordinal: ordinal,
        source_range: ProjectionSourceRange::new(91, 91 + source.len() as u64).unwrap(),
        source: source.into(),
    }
}

fn prepare(source: &str) -> PreparedNarrative {
    prepare_narrative(payload(source, MarkdownBlockKind::Paragraph, 1), usize::MAX).unwrap()
}

#[test]
fn bounded_inline_styles_and_link_labels_preserve_source_authority() {
    let source = "*em* **strong** ~~gone~~ `raw*` [label](https://x) \\* ***both***";
    let prepared = prepare(source);
    assert_eq!(prepared.source(), source);
    assert_eq!(prepared.display(), "em strong gone raw* label * both");
    for (word, style) in [
        ("em", 0),
        ("strong", 1),
        ("gone", 2),
        ("raw*", 3),
        ("label", 4),
        ("both", 5),
    ] {
        let offset = prepared.display().find(word).unwrap();
        let run = prepared
            .runs()
            .iter()
            .find(|run| run.display.contains(&offset))
            .unwrap();
        match style {
            0 => assert!(run.style.emphasis),
            1 => assert!(run.style.strong),
            2 => assert!(run.style.strikethrough),
            3 => assert!(run.style.code),
            4 => assert!(run.style.link_label),
            _ => assert!(run.style.emphasis && run.style.strong),
        }
    }
    assert_eq!(
        prepared.source_range_for_display(0..prepared.display().len()),
        Some(0..source.len())
    );
    assert_eq!(prepared.block_id(), MarkdownBlockId::from_bytes([7; 32]));
    assert_eq!(
        prepared.source_range(),
        ProjectionSourceRange::new(91, 91 + source.len() as u64).unwrap()
    );
    assert_eq!(prepared.disposition(), NarrativeDisposition::Styled);
}

#[test]
fn exact_selection_expands_only_fully_selected_constructs() {
    let prepared = prepare("**café** and [two words](target)");
    assert_eq!(prepared.source_range_for_display(0..5), Some(0..9));
    assert_eq!(prepared.source_range_for_display(0..3), Some(2..5));
    assert_eq!(prepared.source_range_for_display(3..5), Some(5..7));
    let label = prepared.display().find("two words").unwrap();
    let full = prepared.source_range_for_display(label..label + 9).unwrap();
    assert_eq!(&prepared.source()[full], "[two words](target)");
    let partial = prepared
        .source_range_for_display(label + 4..label + 9)
        .unwrap();
    assert_eq!(&prepared.source()[partial], "words");
    assert_eq!(
        prepared.selected_display(label + 4..label + 9),
        Some("words")
    );
    assert_eq!(prepared.source_range_for_display(4..5), None);
    assert_eq!(prepared.snap_display_endpoint(4, false), Some(3));
    assert_eq!(prepared.snap_display_endpoint(4, true), Some(5));
}

#[test]
fn multiline_emphasis_and_code_use_bound_source_units() {
    let source = "**first\r\nsecond** and *a `*` b* and ` code\r\nline `";
    let prepared = prepare(source);
    assert_eq!(prepared.display(), "first\nsecond and a * b and code line");
    let first = prepared.source_range_for_display(0..12).unwrap();
    assert_eq!(&source[first], "**first\r\nsecond**");
    let code = prepared.display().find("code line").unwrap();
    let selected = prepared.source_range_for_display(code..code + 9).unwrap();
    assert_eq!(&source[selected], "` code\r\nline `");
    let newline = prepared
        .source_range_for_display(code + 4..code + 5)
        .unwrap();
    assert_eq!(&source[newline], "\r\n");
    assert_eq!(prepared.selected_display(code + 4..code + 5), Some(" "));
}

#[test]
fn crlf_escapes_unicode_and_hard_breaks_have_exact_atomic_mappings() {
    let source = "café \\*  \r\nnext\\\r\nlast";
    let prepared = prepare(source);
    assert_eq!(prepared.display(), "café *\nnext\nlast");
    let escaped = prepared.source_range_for_display(6..7).unwrap();
    assert_eq!(&source[escaped], "\\*");
    let newline = prepared.source_range_for_display(7..8).unwrap();
    assert_eq!(&source[newline], "  \r\n");
    let next = prepared.display().find("next").unwrap();
    let newline = prepared
        .source_range_for_display(next + 4..next + 5)
        .unwrap();
    assert_eq!(&source[newline], "\\\r\n");
    assert_eq!(
        prepared.selected_display(0..prepared.display().len()),
        Some("café *\nnext\nlast")
    );
    assert_eq!(
        prepared.source_range_for_display(0..prepared.display().len()),
        Some(0..source.len())
    );
    for map in prepared.mappings() {
        assert!(source.get(map.source.clone()).is_some());
        assert!(prepared.display().get(map.display.clone()).is_some());
    }
    assert_eq!(
        prepared
            .mappings()
            .iter()
            .filter(|map| map.kind == NarrativeMappingKind::Transformed)
            .count(),
        3
    );
}

#[test]
fn recognized_block_treatment_removes_only_bound_source_syntax() {
    for (source, kind, display, treatment) in [
        (
            "## *Title* ##\r\n",
            MarkdownBlockKind::Heading(2),
            "Title\n",
            NarrativeTreatment::Heading(2),
        ),
        (
            "> quoted\r\n> **next**",
            MarkdownBlockKind::BlockQuote,
            "quoted\nnext",
            NarrativeTreatment::Quote,
        ),
        (
            "1. first\n2. second",
            MarkdownBlockKind::List,
            "first\nsecond",
            NarrativeTreatment::List,
        ),
        (
            "---\r\n",
            MarkdownBlockKind::ThematicBreak,
            "---\r\n",
            NarrativeTreatment::ThematicBreak,
        ),
    ] {
        let prepared = prepare_narrative(payload(source, kind, 1), usize::MAX).unwrap();
        assert_eq!(prepared.display(), display);
        assert_eq!(prepared.treatment(), treatment);
        assert_eq!(
            prepared.source_range_for_display(0..display.len()),
            Some(0..source.len())
        );
    }
}

#[test]
fn fallback_and_continuation_are_source_sharing_literal_models() {
    for (kind, ordinal, disposition) in [
        (
            MarkdownBlockKind::Fallback,
            1,
            NarrativeDisposition::LiteralFallback,
        ),
        (
            MarkdownBlockKind::List,
            2,
            NarrativeDisposition::LiteralContinuation,
        ),
    ] {
        let source = "**still source**\r\n";
        let input = payload(source, kind, ordinal);
        let ProjectionPayload::InlineMarkdown {
            source: original, ..
        } = &input
        else {
            unreachable!()
        };
        let allocation = original.as_ptr();
        let prepared = prepare_narrative(input, usize::MAX).unwrap();
        assert_eq!(prepared.source().as_ptr(), allocation);
        assert_eq!(prepared.display().as_ptr(), allocation);
        assert_eq!(prepared.display(), source);
        assert_eq!(prepared.block_kind(), kind);
        assert_eq!(prepared.span_ordinal(), ordinal);
        assert_eq!(prepared.disposition(), disposition);
        assert_eq!(prepared.treatment(), NarrativeTreatment::Literal);
        assert_eq!(prepared.mappings().len(), 1);
        assert_eq!(prepared.source_range_for_display(2..7), Some(2..7));
    }
}

#[test]
fn unsupported_and_ambiguous_inline_spelling_remains_local_literal() {
    let source = "unclosed *em and ![image](target) [reference][id] <tag> a_b_c *yes*";
    let prepared = prepare(source);
    assert_eq!(
        prepared.display(),
        "unclosed *em and ![image](target) [reference][id] <tag> a_b_c yes"
    );
    assert!(prepared.runs().last().unwrap().style.emphasis);
}

#[test]
fn syntax_and_run_limits_return_one_coherent_literal_span() {
    let run_limit = "*x*.".repeat(NARRATIVE_MAX_RUNS / 2);
    let allowed = prepare(&run_limit);
    assert_eq!(allowed.disposition(), NarrativeDisposition::Styled);
    assert_eq!(allowed.runs().len(), NARRATIVE_MAX_RUNS);
    let exceeded = format!("{run_limit}*x*.");
    let literal = prepare(&exceeded);
    assert_eq!(
        literal.disposition(),
        NarrativeDisposition::LiteralSyntaxLimit
    );
    assert_eq!(literal.display(), exceeded);
    assert!(
        literal
            .runs()
            .iter()
            .all(|run| run.style == Default::default())
    );
    assert_eq!(literal.mappings().len(), 1);
    let allowed = format!("[label]({}x{})", "(".repeat(15), ")".repeat(15));
    assert_eq!(prepare(&allowed).display(), "label");
    let exceeded = format!("[label]({}x{})", "(".repeat(16), ")".repeat(16));
    let literal = prepare(&exceeded);
    assert_eq!(
        literal.disposition(),
        NarrativeDisposition::LiteralSyntaxLimit
    );
    assert_eq!(literal.display(), exceeded);
}

#[test]
fn maximum_source_and_unicode_fragment_boundaries_are_bounded() {
    for source in [
        "x".repeat(NARRATIVE_MAX_BYTES),
        format!("{}é{}", "x".repeat(4095), "y".repeat(5000)),
    ] {
        let prepared = prepare(&source);
        assert_eq!(prepared.display(), source);
        assert!(prepared.fragments().len() <= NARRATIVE_MAX_FRAGMENTS);
        assert!(prepared.mappings().len() <= source.len() + 1);
        let mut end = 0;
        for fragment in prepared.fragments() {
            assert_eq!(fragment.display.start, end);
            assert!(fragment.display.len() <= NARRATIVE_FRAGMENT_MAX_BYTES);
            assert!(fragment.runs.len() <= NARRATIVE_MAX_RUNS);
            assert!(prepared.display().get(fragment.display.clone()).is_some());
            let runs = &prepared.runs()[fragment.runs.clone()];
            assert_eq!(runs.first().unwrap().display.start, fragment.display.start);
            assert_eq!(runs.last().unwrap().display.end, fragment.display.end);
            end = fragment.display.end;
        }
        assert_eq!(end, source.len());
    }
}

#[test]
fn refusal_preserves_exact_source_and_reports_all_retained_capacity() {
    let source = "**capacity**\r\n";
    let prepared = prepare(source);
    let charge = source.len()
        + prepared.display().len()
        + prepared.runs().len() * size_of::<NarrativeRun>()
        + prepared.mappings().len() * size_of::<NarrativeMapping>()
        + prepared.fragments().len() * size_of::<NarrativeFragment>();
    assert!(prepared.retained_bytes() >= charge);
    let accepted = prepare_narrative(
        payload(source, MarkdownBlockKind::Paragraph, 1),
        prepared.retained_bytes(),
    )
    .unwrap();
    assert_eq!(accepted.retained_bytes(), prepared.retained_bytes());
    let error = prepare_narrative(
        payload(source, MarkdownBlockKind::Paragraph, 1),
        prepared.retained_bytes() - 1,
    )
    .unwrap_err();
    assert_eq!(error.kind(), NarrativePreparationErrorKind::Capacity);
    assert_eq!(
        error.into_payload(),
        payload(source, MarkdownBlockKind::Paragraph, 1)
    );
}

#[test]
fn invalid_bindings_refuse_without_losing_payload() {
    let mut bad_range = payload("source", MarkdownBlockKind::Paragraph, 1);
    if let ProjectionPayload::InlineMarkdown { source_range, .. } = &mut bad_range {
        *source_range = ProjectionSourceRange::new(0, 2).unwrap();
    }
    for input in [
        ProjectionPayload::Empty,
        bad_range,
        payload("text", MarkdownBlockKind::Table, 1),
        payload("text", MarkdownBlockKind::FencedCode, 1),
        payload("text", MarkdownBlockKind::Heading(7), 1),
        payload("text", MarkdownBlockKind::Paragraph, 0),
        payload(
            &"x".repeat(NARRATIVE_MAX_BYTES + 1),
            MarkdownBlockKind::Paragraph,
            1,
        ),
    ] {
        let error = prepare_narrative(input.clone(), usize::MAX).unwrap_err();
        assert_eq!(error.kind(), NarrativePreparationErrorKind::InvalidBinding);
        assert_eq!(error.into_payload(), input);
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn genuine_saved_projection_preparation_keeps_block_source_bindings() {
    let source =
        "# Héading\r\n\r\npara **bold** and \\*\r\n\r\n> quoted\r\n\r\n- list\r\n\r\n---\r\n";
    let saved = fixture::project_user_markdown("narrative_saved_projection", source);
    let mut kinds = Vec::new();
    for projection in fixture::projections(&saved) {
        let ProjectionPayload::InlineMarkdown {
            block_id,
            block_kind,
            span_ordinal,
            source_range,
            source: original,
        } = projection.payload()
        else {
            panic!("ordinary fixture must remain inline");
        };
        let prepared = prepare_narrative(projection.payload().clone(), usize::MAX).unwrap();
        assert_eq!(prepared.block_id(), *block_id);
        assert_eq!(prepared.block_kind(), *block_kind);
        assert_eq!(prepared.span_ordinal(), *span_ordinal);
        assert_eq!(prepared.source_range(), *source_range);
        assert_eq!(prepared.source(), original.as_ref());
        assert_eq!(
            prepared.source(),
            &source[source_range.start() as usize..source_range.end() as usize]
        );
        assert_eq!(
            prepared.source_range_for_display(0..prepared.display().len()),
            Some(0..original.len())
        );
        kinds.push(*block_kind);
    }
    for kind in [
        MarkdownBlockKind::Heading(1),
        MarkdownBlockKind::Paragraph,
        MarkdownBlockKind::BlockQuote,
        MarkdownBlockKind::List,
        MarkdownBlockKind::ThematicBreak,
    ] {
        assert!(kinds.contains(&kind), "saved projection missing {kind:?}");
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn saved_bounded_spans_keep_shared_identity_and_literal_continuity() {
    let source = "x".repeat(syndic_storage::MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES + 1);
    let saved = fixture::project_user_markdown("narrative_saved_continuity", &source);
    let records = fixture::projections(&saved);
    assert!(records.len() > 1);
    let mut source_offset = 0;
    let mut block_id = None;
    for (index, record) in records.iter().enumerate() {
        let prepared = prepare_narrative(record.payload().clone(), usize::MAX).unwrap();
        assert_eq!(prepared.span_ordinal(), index as u64 + 1);
        assert_eq!(prepared.source_range().start(), source_offset);
        source_offset = prepared.source_range().end();
        if let Some(expected) = block_id {
            assert_eq!(prepared.block_id(), expected);
        } else {
            block_id = Some(prepared.block_id());
        }
        assert_eq!(prepared.display(), prepared.source());
        assert_eq!(
            prepared.disposition(),
            if index == 0 {
                NarrativeDisposition::LiteralFallback
            } else {
                NarrativeDisposition::LiteralContinuation
            }
        );
        assert_eq!(prepared.treatment(), NarrativeTreatment::Literal);
    }
    assert_eq!(source_offset as usize, source.len());
}
