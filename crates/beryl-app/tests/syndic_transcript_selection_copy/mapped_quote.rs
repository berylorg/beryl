use super::*;
use std::sync::Arc;

fn branch_projection(index: u8, text: &str, start: u64) -> ProjectionRecord {
    let mut record = projection("branch", u64::from(index), text, start);
    record.id =
        ProjectionRecordId(beryl_model::SyndicProjectionId::from_bytes([index; 16]).to_string());
    record.provenance.view_id =
        TranscriptViewId(beryl_model::SyndicThreadId::from_bytes([31; 16]).to_string());
    record.provenance.turn_id = Some(SyndicTurnId(
        beryl_model::SyndicTurnId::from_bytes([32; 16]).to_string(),
    ));
    record.provenance.item_id = Some(SyndicItemId(
        beryl_model::SyndicItemId::from_bytes([33; 16]).to_string(),
    ));
    record.provenance.projection_id = Some(record.id.clone());
    record.provenance.transcript_entry = Some(syndic_storage::CurrentTranscriptEntryProof::new(
        syndic_storage::TranscriptGeneration::FIRST,
        syndic_storage::TranscriptPosition::FIRST,
    ));
    record
}

fn branch_seed(records: Vec<ProjectionRecord>) -> ResidentTranscriptCore {
    let view = records[0].provenance.view_id.clone();
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(
        prepared_for_view(records, REVISION, view),
        TranscriptActivationSource::RunningThread,
    );
    realize_frame_with_viewport(&mut core, 1000.0);
    core
}

fn branch_capture(
    core: &ResidentTranscriptCore,
    ranges: &[(usize, Range<usize>)],
) -> Option<syndic_transcript_core::core::CapturedDiscussionSource> {
    let snapshot = core.presentation_snapshot();
    let ranges = ranges
        .iter()
        .map(|(index, range)| (snapshot.records[*index].id.clone(), range.clone()))
        .collect::<Vec<_>>();
    core.mapped_discussion_source(
        snapshot.activation_revision,
        snapshot.presentation_revision,
        &ranges,
    )
}

#[test]
fn mapped_branch_capture_preserves_exact_utf8_syntax_and_absolute_provenance() {
    let text = "before **é中** after\r\n";
    let core = branch_seed(vec![branch_projection(1, text, 412)]);
    let selected = text.find("**").unwrap()..text.find(" after").unwrap();
    let captured = branch_capture(&core, &[(0, selected.clone())]).unwrap();
    assert_eq!(captured.text.as_str(), "**é中**");
    assert_eq!(captured.source.range().start(), 412 + selected.start as u64);
    assert_eq!(captured.source.range().end(), 412 + selected.end as u64);
    assert_eq!(
        captured.source.thread_id(),
        beryl_model::SyndicThreadId::from_bytes([31; 16])
    );
    assert_eq!(
        captured.source.turn_id(),
        beryl_model::SyndicTurnId::from_bytes([32; 16])
    );
    assert_eq!(
        captured.source.item_id(),
        beryl_model::SyndicItemId::from_bytes([33; 16])
    );
    assert!(branch_capture(&core, &[(0, selected.start + 3..selected.end)]).is_none());
    assert_eq!(
        branch_capture(&core, &[(0, 0..text.len())])
            .unwrap()
            .text
            .as_str(),
        text
    );
}

#[test]
fn mapped_branch_capture_rejects_contiguous_different_projection_owners() {
    let core = branch_seed(vec![
        branch_projection(1, "first", 0),
        branch_projection(2, "second", 5),
    ]);
    assert!(branch_capture(&core, &[(0, 0..5)]).is_some());
    assert!(branch_capture(&core, &[(1, 0..6)]).is_some());
    assert_eq!(
        quote(&core, &[(0, 0..5), (1, 0..6)]).unwrap(),
        "firstsecond"
    );
    assert!(branch_capture(&core, &[(0, 0..5), (1, 0..6)]).is_none());
}

#[test]
fn mapped_branch_capture_bounds_utf8_bytes_without_shortening_context() {
    let text = "é".repeat(32_769);
    let core = branch_seed(vec![branch_projection(1, &text, 0)]);
    let captured = branch_capture(&core, &[(0, 0..65_536)]).unwrap();
    assert_eq!(captured.text.as_str(), &text[..65_536]);
    assert!(branch_capture(&core, &[(0, 0..65_538)]).is_none());
    assert!(branch_capture(&core, &[(0, 0..65_535)]).is_none());
    assert_eq!(quote(&core, &[(0, 0..2)]).unwrap(), "é");
}

fn projection(name: &str, position: u64, text: &str, start: u64) -> ProjectionRecord {
    let mut record = text_projection(&view_id(), position, name, text, None);
    record.provenance.source_range = Some(start..start + text.len() as u64);
    record.provenance.copy_source_range = record.provenance.source_range.clone();
    record
}

fn prepared(records: Vec<ProjectionRecord>) -> PreparedTranscriptActivation {
    prepared_at(records, REVISION)
}

fn prepared_at(
    records: Vec<ProjectionRecord>,
    revision: ProviderRevision,
) -> PreparedTranscriptActivation {
    prepared_for_view(records, revision, view_id())
}

fn prepared_for_view(
    mut records: Vec<ProjectionRecord>,
    revision: ProviderRevision,
    view: TranscriptViewId,
) -> PreparedTranscriptActivation {
    for record in &mut records {
        record.revision = revision;
    }
    let views = records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            view_record_with_provenance(
                index as u64 + 1,
                &record.id.0,
                record.id.clone(),
                record.provenance.clone(),
            )
        })
        .collect();
    PreparedTranscriptActivation::new(
        view.clone(),
        TranscriptActivationPlacement::Start,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: view.clone(),
            revision,
            history_state: TranscriptProviderHistoryState::Complete,
            records: views,
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        Some(TranscriptProviderResponseKind::ProjectionRecords(
            ProjectionRecordSet {
                view_id: view,
                revision,
                records,
                rejections: Vec::new(),
            },
        )),
    )
}

fn seed(records: Vec<ProjectionRecord>) -> ResidentTranscriptCore {
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(prepared(records), TranscriptActivationSource::RunningThread);
    realize_frame_with_viewport(&mut core, 1000.0);
    core
}

fn quote(
    core: &ResidentTranscriptCore,
    ranges: &[(usize, Range<usize>)],
) -> Result<String, ResidentSelectionUnavailable> {
    let snapshot = core.presentation_snapshot();
    let ranges = ranges
        .iter()
        .map(|(index, range)| (snapshot.records[*index].id.clone(), range.clone()))
        .collect::<Vec<_>>();
    core.mapped_quote_markdown(
        snapshot.activation_revision,
        snapshot.presentation_revision,
        &ranges,
    )
}

fn narrative_payload(text: &str, start: u64) -> syndic_storage::ProjectionPayload {
    syndic_storage::ProjectionPayload::inline_markdown(
        syndic_storage::MarkdownBlockId::from_bytes([19; 32]),
        syndic_storage::MarkdownBlockKind::Paragraph,
        1,
        syndic_storage::ProjectionSourceRange::new(start, start + text.len() as u64).unwrap(),
        text,
    )
    .unwrap()
}

#[test]
fn mapped_quote_preserves_partial_unicode_crlf_and_exact_absolute_span_containment() {
    let text = "α **é\\*中**\r\n末\r\nω";
    let core = seed(vec![projection("unicode", 1, text, 90_000)]);
    let start = text.find('é').unwrap();
    let end = text.find('ω').unwrap();
    let expected = &text[start..end];
    assert_eq!(quote(&core, &[(0, start..end)]).unwrap(), expected);
    assert_eq!(quote(&core, &[(0, 0..text.len())]).unwrap(), text);
    assert!(matches!(
        quote(&core, &[(0, 1..end)]),
        Err(ResidentSelectionUnavailable::MissingStableProvenance { .. })
    ));

    let mut shortened = projection("shortened", 1, text, 90_000);
    shortened.provenance.source_range = Some(90_000..90_000 + end as u64 - 1);
    let core = seed(vec![shortened]);
    assert!(matches!(
        quote(&core, &[(0, start..end)]),
        Err(ResidentSelectionUnavailable::MissingStableProvenance { .. })
    ));

    let mut overflow = projection("overflow", 1, "abc", 0);
    overflow.provenance.source_range = Some(u64::MAX - 1..u64::MAX);
    let core = seed(vec![overflow]);
    assert!(matches!(
        quote(&core, &[(0, 0..2)]),
        Err(ResidentSelectionUnavailable::MissingStableProvenance { .. })
    ));
}

#[test]
fn mapped_quote_admits_prepared_narrative_and_preserves_selected_source_syntax() {
    let text = "before **é中** after\r\n";
    let mut record = projection("prepared", 1, text, 412);
    record.payload = ProjectionPayload::NarrativeReady {
        prepared: Arc::new(
            syndic_storage::narrative::prepare_narrative(narrative_payload(text, 412), usize::MAX)
                .unwrap(),
        ),
    };
    let core = seed(vec![record]);
    assert!(matches!(
        core.presentation_snapshot().records[0].kind,
        ResidentPresentationRecordKind::NarrativeReady { .. }
    ));
    let start = text.find('é').unwrap();
    let end = text.find(" after").unwrap();
    assert_eq!(quote(&core, &[(0, start..end)]).unwrap(), "é中**");
    assert_eq!(quote(&core, &[(0, 0..text.len())]).unwrap(), text);
}

#[test]
fn mapped_quote_rejects_synthetic_and_mixed_context_without_changing_presentation() {
    let mut activation = prepared(vec![projection("authored", 1, "authored", 600)]);
    activation.discussion_context = Some(SyntheticContextGroup {
        provenance: Arc::new(SyntheticContextProvenance {
            view_id: view_id(),
            group_id: "quote-context".into(),
            insertion_after: 1,
            insertion_parent: beryl_model::SyndicTurnId::from_bytes([4; 16]),
            descriptor: None,
            envelope_revision: 1,
            chunk_count: 1,
        }),
        chunks: vec![SyntheticContextChunk {
            range: 0..7,
            text: "context".into(),
            plain_text: "context".into(),
        }],
    });
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(activation, TranscriptActivationSource::RunningThread);
    realize_frame_with_viewport(&mut core, 1000.0);
    let before = core.presentation_snapshot();
    assert!(matches!(
        before.records[1].provenance.source,
        ResidentRecordSource::LocalContext { .. }
    ));
    assert!(matches!(
        quote(&core, &[(1, 0..7)]),
        Err(ResidentSelectionUnavailable::NonContentRecord { .. })
    ));
    assert!(matches!(
        quote(&core, &[(0, 0..8), (1, 0..7)]),
        Err(ResidentSelectionUnavailable::NonContentRecord { .. })
    ));
    assert_eq!(core.presentation_snapshot(), before);
    assert_eq!(quote(&core, &[(0, 0..8)]).unwrap(), "authored");
}

#[test]
fn mapped_quote_rejects_stale_activation_presentation_and_same_identity_source_replacement() {
    let mut core = seed(vec![projection("source", 1, "before", 210)]);
    let snapshot = core.presentation_snapshot();
    let ranges = [(snapshot.records[0].id.clone(), 0..6)];
    assert!(matches!(
        core.mapped_quote_markdown(
            snapshot.activation_revision + 1,
            snapshot.presentation_revision,
            &ranges
        ),
        Err(ResidentSelectionUnavailable::StalePresentationRevision { .. })
    ));
    assert!(matches!(
        core.mapped_quote_markdown(
            snapshot.activation_revision,
            snapshot.presentation_revision + 1,
            &ranges
        ),
        Err(ResidentSelectionUnavailable::StalePresentationRevision { .. })
    ));
    core.apply_prepared_activation(
        prepared_at(
            vec![projection("source", 1, "change", 210)],
            ProviderRevision(REVISION.0 + 1),
        ),
        TranscriptActivationSource::RunningThread,
    );
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(ProviderRevision(REVISION.0 + 1))
        .insert_projection_record(projection("source", 1, "change", 210));
    let replacement = core.reserve_provider_request(
        TranscriptProviderRequestKind::ReadProjectionRecords(ProjectionRecordsRequest {
            view_id: view_id(),
            projection_ids: vec![projection_id("source")],
            observed_revision: Some(ProviderRevision(REVISION.0 + 1)),
        }),
        ProviderRequestReason::ProjectionAdmission,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, replacement),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 0,
            rejected_count: 0,
        }
    );
    realize_frame_with_viewport(&mut core, 1000.0);
    assert_eq!(
        core.presentation_snapshot().records[0].id,
        snapshot.records[0].id
    );
    assert!(matches!(
        core.mapped_quote_markdown(
            snapshot.activation_revision,
            snapshot.presentation_revision,
            &ranges
        ),
        Err(ResidentSelectionUnavailable::StalePresentationRevision { .. })
    ));
    assert_eq!(quote(&core, &[(0, 0..6)]).unwrap(), "change");
}

#[test]
fn mapped_quote_capacity_applies_to_exact_aggregate_selected_source() {
    let left = "α".repeat(16_384);
    let right = "β".repeat(16_384);
    let core = seed(vec![
        projection("left", 1, &left, 100),
        projection("right", 2, &right, 40_000),
        projection("extra", 3, "x", 80_000),
    ]);
    let before = core.presentation_snapshot();
    let accepted = quote(&core, &[(0, 0..left.len()), (1, 0..right.len())]).unwrap();
    assert_eq!(accepted.len(), 65_536);
    assert_eq!(accepted, format!("{left}{right}"));
    assert_eq!(
        quote(&core, &[(0, 0..left.len()), (1, 0..right.len()), (2, 0..1)]),
        Err(ResidentSelectionUnavailable::CopyCapacity)
    );
    assert_eq!(core.presentation_snapshot(), before);
    assert_eq!(quote(&core, &[(0, 0..2)]).unwrap(), "α");
}

#[test]
fn mapped_quote_rejects_missing_provenance_unavailable_pending_and_missing_records() {
    for missing in 0..4 {
        let mut record = projection("missing", 1, "text", 100);
        match missing {
            0 => record.provenance.turn_id = None,
            1 => record.provenance.item_id = None,
            2 => record.provenance.position = None,
            _ => record.provenance.source_range = None,
        }
        let core = seed(vec![record]);
        assert!(
            quote(&core, &[(0, 0..4)]).is_err(),
            "missing provenance field {missing}"
        );
    }
    for pending in [true, false] {
        let mut record = projection("not-ready", 1, "text", 100);
        let source = Arc::new(narrative_payload("text", 100));
        record.payload = if pending {
            ProjectionPayload::NarrativePending { source }
        } else {
            ProjectionPayload::NarrativeUnavailable {
                source,
                reason: syndic_storage::narrative::NarrativePreparationErrorKind::Capacity,
            }
        };
        let core = seed(vec![record]);
        assert!(matches!(
            quote(&core, &[(0, 0..4)]),
            Err(ResidentSelectionUnavailable::NonContentRecord { .. })
        ));
    }
    let core = seed(vec![projection("present", 1, "text", 100)]);
    let snapshot = core.presentation_snapshot();
    assert!(matches!(
        core.mapped_quote_markdown(
            snapshot.activation_revision,
            snapshot.presentation_revision,
            &[(ResidentPresentationRecordId("absent".into()), 0..4)]
        ),
        Err(ResidentSelectionUnavailable::RecordNotResident { .. })
    ));
    assert_eq!(
        quote(&core, &[]),
        Err(ResidentSelectionUnavailable::EmptySelection)
    );
}

#[test]
fn mapped_quote_rejects_noncontiguous_duplicate_and_unrealized_record_ranges() {
    let mut core = seed(vec![
        projection("first", 1, "one", 10),
        projection("second", 2, "two", 20),
        projection("third", 3, "tri", 30),
    ]);
    assert!(matches!(
        quote(&core, &[(0, 0..3), (2, 0..3)]),
        Err(ResidentSelectionUnavailable::RecordNotRealized { .. })
    ));
    assert!(matches!(
        quote(&core, &[(0, 0..1), (0, 1..3)]),
        Err(ResidentSelectionUnavailable::RecordNotRealized { .. })
    ));
    realize_frame_with_viewport(&mut core, 1.0);
    assert!(matches!(
        quote(&core, &[(2, 0..3)]),
        Err(ResidentSelectionUnavailable::RecordNotRealized { .. })
    ));
}
