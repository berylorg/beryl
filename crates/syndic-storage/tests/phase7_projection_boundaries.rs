#![cfg(feature = "test-faults")]

#[path = "phase6_live_history/support.rs"]
mod support;

use beryl_home_store::{CursorReadLimits, HomeCommand, HomeStore};
use std::num::NonZeroU64;

use beryl_model::{AssetId, ContentRevision, SyndicDraftMarkerId, SyndicItemId, SyndicResourceId};
use syndic_storage::{
    AdmissionMarkers, AdvanceItemProjectionBuild, CONTENT_CHUNK_MAX_BYTES, ComposerAtom,
    ComposerContentAssembler, ComposerPayload, CreateThread, DraftPayloadUpdate,
    DraftPayloadUpdateDecision, IdleSubmission, ImageLabelOrdinal, ItemProjectionGeneration,
    MARKDOWN_CODE_INLINE_MAX_BYTES, MARKDOWN_CODE_INLINE_MAX_LINES,
    MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES, MARKDOWN_SPAN_MAX_BYTES,
    MARKDOWN_TABLE_INLINE_MAX_BODY_ROWS, MARKDOWN_TABLE_INLINE_MAX_BYTES,
    MARKDOWN_TABLE_INLINE_MAX_COLUMNS, MarkdownBlockKind, PreparedContent, ProjectionPayload,
    ResolvedImageMarker, StartItemProjectionBuild, SyndicPointReadLimit, SyndicReadError,
    SyndicStorage, TRANSCRIPT_PAGE_MAX_BYTES,
};

use support::{TestHome, draft_id, id, open, stage_prepared_content, timestamp};

struct ProjectedFixture {
    store: HomeStore,
    _home: TestHome,
    storage: SyndicStorage,
    item: SyndicItemId,
    generation: ItemProjectionGeneration,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn project_user_markdown(name: &str, text: &str) -> ProjectedFixture {
    let payload = ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap();
    project_user_payload(name, payload, AdmissionMarkers::default())
}

fn project_user_payload(
    name: &str,
    payload: ComposerPayload,
    markers: AdmissionMarkers,
) -> ProjectedFixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(thread, draft, timestamp(1)),
        ),
    );

    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&store, storage, &content);
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );

    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let thread_record = storage
        .thread(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let item = SyndicItemId::from_bytes([4; 16]);
    let submission = IdleSubmission::new(
        thread,
        thread_record.record().revision(),
        draft,
        current.draft().revision(),
        current.draft().content(),
        gate.record().revision(),
        draft_id(3),
        item,
        markers,
        timestamp(3),
    );
    execute(
        &store,
        storage.submit_idle_draft(storage.revision(&store).unwrap(), submission),
    );

    let canonical = storage
        .canonical_item(&store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        &store,
        storage.start_item_projection_build(
            storage.revision(&store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.record().revision(), generation),
        ),
    );
    for _ in 0..128 {
        if storage
            .item_projection_set(&store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return ProjectedFixture {
                store,
                _home: home,
                storage,
                item,
                generation,
            };
        }
        let build = storage
            .item_projection_build(&store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            &store,
            storage.advance_item_projection_build(
                storage.revision(&store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.record().revision()),
            ),
        );
    }
    panic!("bounded Markdown projection did not finish");
}

fn projections(fixture: &ProjectedFixture) -> Vec<syndic_storage::ProjectionRecord> {
    let mut output = Vec::new();
    let mut after = None;
    loop {
        let page = fixture
            .storage
            .item_projections(
                &fixture.store,
                fixture.item,
                fixture.generation,
                after,
                CursorReadLimits::new(256, TRANSCRIPT_PAGE_MAX_BYTES).unwrap(),
            )
            .unwrap();
        for index in page.records() {
            output.push(
                fixture
                    .storage
                    .projection(&fixture.store, index.projection_id(), point_limit())
                    .unwrap()
                    .unwrap()
                    .record()
                    .clone(),
            );
            after = Some(index.ordinal());
        }
        if !page.has_more() {
            return output;
        }
    }
}

fn single_projection(name: &str, markdown: &str) -> syndic_storage::ProjectionRecord {
    let fixture = project_user_markdown(name, markdown);
    let mut projected = projections(&fixture);
    assert_eq!(projected.len(), 1, "{name} must produce one block");
    projected.pop().unwrap()
}

fn assert_inline_block(name: &str, markdown: &str, expected: MarkdownBlockKind) {
    let projection = single_projection(name, markdown);
    match projection.payload() {
        ProjectionPayload::InlineMarkdown {
            block_kind,
            source,
            source_range,
            ..
        } => {
            assert_eq!(*block_kind, expected);
            assert_eq!(&**source, markdown);
            assert_eq!(source_range.start(), 0);
            assert_eq!(source_range.end(), markdown.len() as u64);
        }
        payload => panic!("expected one inline {expected:?} block, got {payload:?}"),
    }
}

fn assert_resource_block(name: &str, markdown: &str, expected: MarkdownBlockKind) {
    let projection = single_projection(name, markdown);
    match projection.payload() {
        ProjectionPayload::ResourceReference {
            block_kind,
            source_range,
            ..
        } => {
            assert_eq!(*block_kind, expected);
            assert!(source_range.end() <= markdown.len() as u64);
        }
        payload => panic!("expected one {expected:?} resource, got {payload:?}"),
    }
}

#[test]
fn composer_chunk_boundaries_never_split_utf8_scalars() {
    let mut text = "a".repeat(CONTENT_CHUNK_MAX_BYTES - 19);
    text.push('🧵');
    let payload = ComposerPayload::new(vec![ComposerAtom::text(&text).unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();

    assert_eq!(content.chunks().len(), 2);
    assert_eq!(
        content.chunks()[0].bytes().len(),
        CONTENT_CHUNK_MAX_BYTES - 1
    );
    assert_eq!(content.text_spans().len(), 2);
    for span in content.text_spans() {
        let chunk = &content.chunks()[span.chunk_ordinal().get() as usize - 1];
        let local_start = usize::try_from(span.encoded_start() - span.chunk_start()).unwrap();
        let local_end = usize::try_from(span.encoded_end() - span.chunk_start()).unwrap();
        assert!(std::str::from_utf8(&chunk.bytes()[local_start..local_end]).is_ok());
    }

    let reference = content.reference(ContentRevision::new(1).unwrap());
    let mut assembler = ComposerContentAssembler::new(reference).unwrap();
    for chunk in content.chunks() {
        assembler.push(chunk).unwrap();
    }
    assert_eq!(assembler.finish().unwrap(), payload);
}

#[test]
fn zero_width_image_pieces_survive_marker_only_and_trailing_positions() {
    let marker_id = SyndicDraftMarkerId::from_bytes([31; 16]);
    let label = ImageLabelOrdinal::new(1).unwrap();
    let marker = ResolvedImageMarker::new(
        marker_id,
        label,
        AssetId::sha256_v1([32; 32], NonZeroU64::new(17).unwrap()),
    );
    let markers = || AdmissionMarkers::new(vec![marker]).unwrap();

    let marker_only = project_user_payload(
        "phase7-marker-only",
        ComposerPayload::new(vec![ComposerAtom::image_marker(marker_id, label)]).unwrap(),
        markers(),
    );
    let marker_only = projections(&marker_only);
    assert_eq!(marker_only.len(), 1);
    assert!(matches!(
        marker_only[0].payload(),
        ProjectionPayload::ImageMarker {
            source_offset: 0,
            ..
        }
    ));

    let trailing = project_user_payload(
        "phase7-trailing-marker",
        ComposerPayload::new(vec![
            ComposerAtom::text("before marker").unwrap(),
            ComposerAtom::image_marker(marker_id, label),
        ])
        .unwrap(),
        markers(),
    );
    let trailing = projections(&trailing);
    assert_eq!(trailing.len(), 2);
    assert!(matches!(
        trailing[0].payload(),
        ProjectionPayload::InlineMarkdown {
            block_kind: MarkdownBlockKind::Paragraph,
            ..
        }
    ));
    assert!(matches!(
        trailing[1].payload(),
        ProjectionPayload::ImageMarker {
            source_offset: 13,
            ..
        }
    ));
}

#[test]
fn paragraph_threshold_changes_one_inline_block_into_bounded_spans() {
    let exact_text = "a".repeat(MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES);
    let exact = project_user_markdown("phase7-paragraph-exact", &exact_text);
    let exact_projections = projections(&exact);
    assert_eq!(exact_projections.len(), 1);
    match exact_projections[0].payload() {
        ProjectionPayload::InlineMarkdown {
            block_kind,
            span_ordinal,
            source_range,
            source,
            ..
        } => {
            assert_eq!(*block_kind, MarkdownBlockKind::Paragraph);
            assert_eq!(*span_ordinal, 1);
            assert_eq!(source_range.start(), 0);
            assert_eq!(source_range.end(), exact_text.len() as u64);
            assert_eq!(&**source, exact_text);
        }
        payload => panic!("expected one inline paragraph, got {payload:?}"),
    }

    let over_text = "b".repeat(MARKDOWN_PARAGRAPH_INLINE_MAX_BYTES + 1);
    let over = project_user_markdown("phase7-paragraph-over", &over_text);
    let over_projections = projections(&over);
    assert_eq!(over_projections.len(), 3);
    assert_eq!(
        over_projections
            .iter()
            .map(|projection| projection.payload().inline_source().unwrap().len())
            .collect::<Vec<_>>(),
        [MARKDOWN_SPAN_MAX_BYTES, MARKDOWN_SPAN_MAX_BYTES, 1]
    );
    let mut reconstructed = String::new();
    let mut expected_start = 0_u64;
    let mut block_id = None;
    for (index, projection) in over_projections.iter().enumerate() {
        match projection.payload() {
            ProjectionPayload::InlineMarkdown {
                block_id: actual_block_id,
                block_kind,
                span_ordinal,
                source_range,
                source,
            } => {
                assert_eq!(*block_kind, MarkdownBlockKind::Fallback);
                assert_eq!(*span_ordinal, index as u64 + 1);
                assert_eq!(source_range.start(), expected_start);
                expected_start = source_range.end();
                if let Some(expected_block_id) = block_id {
                    assert_eq!(*actual_block_id, expected_block_id);
                } else {
                    block_id = Some(*actual_block_id);
                }
                reconstructed.push_str(source);
            }
            payload => panic!("expected bounded inline span, got {payload:?}"),
        }
    }
    assert_eq!(expected_start, over_text.len() as u64);
    assert_eq!(reconstructed, over_text);
    assert!(over_projections.iter().all(|projection| {
        projection.payload().inline_source().unwrap().len() <= MARKDOWN_SPAN_MAX_BYTES
    }));
}

#[test]
fn projection_pages_clamp_oversized_caller_limits() {
    let text = "p".repeat(MARKDOWN_SPAN_MAX_BYTES * 300);
    let fixture = project_user_markdown("phase7-projection-page-clamp", &text);
    let page = fixture
        .storage
        .item_projections(
            &fixture.store,
            fixture.item,
            fixture.generation,
            None,
            CursorReadLimits::new(1_000, 1_000_000).unwrap(),
        )
        .unwrap();

    assert_eq!(page.records().len(), 256);
    assert!(page.stored_bytes() <= TRANSCRIPT_PAGE_MAX_BYTES);
    assert!(page.has_more());
}

#[test]
fn heavy_resource_text_is_range_read_with_exact_bounded_continuations() {
    let body = "z".repeat(CONTENT_CHUNK_MAX_BYTES + 2_000);
    let markdown = format!("```text\n{body}\n```");
    let fixture = project_user_markdown("phase7-resource-range", &markdown);
    let projected = projections(&fixture);
    assert_eq!(projected.len(), 1);
    let resource_id = match projected[0].payload() {
        ProjectionPayload::ResourceReference {
            block_kind,
            resource_id,
            ..
        } => {
            assert_eq!(*block_kind, MarkdownBlockKind::FencedCode);
            *resource_id
        }
        payload => panic!("expected a heavy code resource, got {payload:?}"),
    };
    let expected = format!("{body}\n").into_bytes();
    let metadata = fixture
        .storage
        .resource(&fixture.store, resource_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(metadata.record().byte_length(), Some(expected.len() as u64));

    let mut observed = Vec::new();
    let mut offset = 0_u64;
    loop {
        let page = fixture
            .storage
            .resource_text_range(
                &fixture.store,
                resource_id,
                offset,
                expected.len() as u64,
                4_093,
            )
            .unwrap()
            .unwrap();
        assert_eq!(page.start(), offset);
        assert!(!page.bytes().is_empty());
        assert!(page.bytes().len() <= 4_093);
        assert!(page.stored_bytes() >= page.bytes().len());
        observed.extend_from_slice(page.bytes());
        match page.next_offset() {
            Some(next) => {
                assert_eq!(next, offset + page.bytes().len() as u64);
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(observed, expected);

    let empty = fixture
        .storage
        .resource_text_range(&fixture.store, resource_id, 17, 17, 1)
        .unwrap()
        .unwrap();
    assert!(empty.bytes().is_empty());
    assert_eq!(empty.next_offset(), None);
    assert!(
        fixture
            .storage
            .resource_text_range(
                &fixture.store,
                SyndicResourceId::from_bytes([99; 16]),
                0,
                0,
                1,
            )
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        fixture.storage.resource_text_range(
            &fixture.store,
            resource_id,
            0,
            expected.len() as u64,
            0,
        ),
        Err(SyndicReadError::InvalidResourceReadLimit {
            maximum: TRANSCRIPT_PAGE_MAX_BYTES,
            actual: 0,
        })
    ));
    assert!(matches!(
        fixture.storage.resource_text_range(
            &fixture.store,
            resource_id,
            0,
            expected.len() as u64,
            TRANSCRIPT_PAGE_MAX_BYTES + 1,
        ),
        Err(SyndicReadError::InvalidResourceReadLimit { .. })
    ));
    assert!(matches!(
        fixture
            .storage
            .resource_text_range(&fixture.store, resource_id, 2, 1, 1),
        Err(SyndicReadError::InvalidResourceRange { .. })
    ));
    assert!(matches!(
        fixture.storage.resource_text_range(
            &fixture.store,
            resource_id,
            0,
            expected.len() as u64 + 1,
            1,
        ),
        Err(SyndicReadError::InvalidResourceRange { .. })
    ));
}

#[test]
fn fenced_code_thresholds_crlf_and_unfinished_blocks_preserve_exact_source() {
    let exact_bytes_body = format!("{}\n", "x".repeat(MARKDOWN_CODE_INLINE_MAX_BYTES - 1));
    let exact_bytes = format!("```text\n{exact_bytes_body}```\n");
    assert_inline_block(
        "phase7-code-exact-bytes",
        &exact_bytes,
        MarkdownBlockKind::FencedCode,
    );

    let over_bytes_body = format!("{}\n", "x".repeat(MARKDOWN_CODE_INLINE_MAX_BYTES));
    let over_bytes = format!("```text\n{over_bytes_body}```\n");
    assert_resource_block(
        "phase7-code-over-bytes",
        &over_bytes,
        MarkdownBlockKind::FencedCode,
    );

    let exact_lines_body = "x\n".repeat(MARKDOWN_CODE_INLINE_MAX_LINES as usize);
    let exact_lines = format!("```\n{exact_lines_body}```\n");
    assert_inline_block(
        "phase7-code-exact-lines",
        &exact_lines,
        MarkdownBlockKind::FencedCode,
    );

    let over_lines_body = "x\n".repeat(MARKDOWN_CODE_INLINE_MAX_LINES as usize + 1);
    let over_lines = format!("```\n{over_lines_body}```\n");
    assert_resource_block(
        "phase7-code-over-lines",
        &over_lines,
        MarkdownBlockKind::FencedCode,
    );

    let crlf = "```rust\r\nlet value = 1;\r\n```\r\n";
    assert_inline_block("phase7-code-crlf", crlf, MarkdownBlockKind::FencedCode);
    let unfinished = "```rust\nlet value = 1;\n";
    assert_inline_block(
        "phase7-code-unfinished",
        unfinished,
        MarkdownBlockKind::FencedCode,
    );
}

#[test]
fn table_thresholds_and_malformed_delimiters_preserve_exact_source() {
    let exact_rows = format!(
        "a|b\n---|---\n{}",
        "x|y\n".repeat(MARKDOWN_TABLE_INLINE_MAX_BODY_ROWS as usize)
    );
    assert_inline_block(
        "phase7-table-exact-rows",
        &exact_rows,
        MarkdownBlockKind::Table,
    );
    let over_rows = format!(
        "a|b\n---|---\n{}",
        "x|y\n".repeat(MARKDOWN_TABLE_INLINE_MAX_BODY_ROWS as usize + 1)
    );
    assert_resource_block(
        "phase7-table-over-rows",
        &over_rows,
        MarkdownBlockKind::Table,
    );

    let make_columns = |columns: usize| {
        format!(
            "{}\n{}\n{}\n",
            vec!["h"; columns].join("|"),
            vec!["---"; columns].join("|"),
            vec!["v"; columns].join("|")
        )
    };
    let exact_columns = make_columns(MARKDOWN_TABLE_INLINE_MAX_COLUMNS as usize);
    assert_inline_block(
        "phase7-table-exact-columns",
        &exact_columns,
        MarkdownBlockKind::Table,
    );
    let over_columns = make_columns(MARKDOWN_TABLE_INLINE_MAX_COLUMNS as usize + 1);
    assert_resource_block(
        "phase7-table-over-columns",
        &over_columns,
        MarkdownBlockKind::Table,
    );

    let exact_bytes = format!(
        "a|b\n---|---\n{}|b\n",
        "x".repeat(MARKDOWN_TABLE_INLINE_MAX_BYTES - 15)
    );
    assert_eq!(exact_bytes.len(), MARKDOWN_TABLE_INLINE_MAX_BYTES);
    assert_inline_block(
        "phase7-table-exact-bytes",
        &exact_bytes,
        MarkdownBlockKind::Table,
    );
    let over_bytes = format!(
        "a|b\n---|---\n{}|b\n",
        "x".repeat(MARKDOWN_TABLE_INLINE_MAX_BYTES - 14)
    );
    assert_eq!(over_bytes.len(), MARKDOWN_TABLE_INLINE_MAX_BYTES + 1);
    assert_resource_block(
        "phase7-table-over-bytes",
        &over_bytes,
        MarkdownBlockKind::Table,
    );

    let malformed = "a|`b|c`\n--|---\nx|y\n";
    assert_inline_block(
        "phase7-table-malformed-delimiter",
        malformed,
        MarkdownBlockKind::Paragraph,
    );
}
