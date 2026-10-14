use std::ops::Range;

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::*;

fn view_id() -> TranscriptViewId {
    TranscriptViewId("renderer-selection-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn record_id(name: &str) -> ResidentPresentationRecordId {
    ResidentPresentationRecordId(format!("record-{name}"))
}

fn source_provenance(
    name: &str,
    position: u64,
    copy_source_range: Option<Range<u64>>,
) -> SyndicSourceProvenance {
    let projection_id = projection_id(name);
    SyndicSourceProvenance {
        transcript_entry: None,
        view_id: view_id(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(SyndicTurnId(format!("turn-{name}"))),
        item_id: Some(SyndicItemId(format!("item-{name}"))),
        block_id: None,
        projection_id: Some(projection_id),
        resource_id: None,
        source_range: Some(position..position + 8),
        resource_range: None,
        copy_source_range,
    }
}

fn resource_source_provenance(
    name: &str,
    position: u64,
    resource_id: &ResourceId,
) -> SyndicSourceProvenance {
    let mut provenance = source_provenance(name, position, None);
    provenance.resource_id = Some(resource_id.clone());
    provenance.resource_range = Some(0..128);
    provenance
}

fn text_record(
    presentation_revision: u64,
    name: &str,
    position: u64,
    copy_source_range: Option<Range<u64>>,
) -> ResidentPresentationRecord {
    let projection_id = projection_id(name);
    ResidentPresentationRecord {
        id: record_id(name),
        kind: ResidentPresentationRecordKind::TextChunk {
            plain_text: None,
            narrative_kind: TranscriptNarrativeKind::AssistantCommentary,
            text: format!("text-{name}").into(),
        },
        provenance: ResidentRecordProvenance {
            source: ResidentRecordSource::Syndic(source_provenance(
                name,
                position,
                copy_source_range.clone(),
            )),
            projection_id: Some(projection_id),
            projection_revision: Some(ProviderRevision(0)),
            presentation_revision,
            copy_source_range,
        },
        estimated_bytes: 8,
    }
}

fn resource_record(
    presentation_revision: u64,
    name: &str,
    position: u64,
    resource_id: ResourceId,
    resource_kind: ResourceKind,
) -> ResidentPresentationRecord {
    let projection_id = projection_id(name);
    ResidentPresentationRecord {
        id: record_id(name),
        kind: ResidentPresentationRecordKind::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind,
            label: Some(format!("resource-{name}").into()),
        },
        provenance: ResidentRecordProvenance {
            source: ResidentRecordSource::Syndic(resource_source_provenance(
                name,
                position,
                &resource_id,
            )),
            projection_id: Some(projection_id),
            projection_revision: Some(ProviderRevision(0)),
            presentation_revision,
            copy_source_range: None,
        },
        estimated_bytes: 64,
    }
}

fn fallback_record(
    presentation_revision: u64,
    name: &str,
    position: u64,
) -> ResidentPresentationRecord {
    let projection_id = projection_id(name);
    ResidentPresentationRecord {
        id: record_id(name),
        kind: ResidentPresentationRecordKind::LocalUiFallback {
            reason: LocalPresentationReason::BudgetRejected,
            target: ResidentFallbackTarget::ProjectionRecord(projection_id.clone()),
        },
        provenance: ResidentRecordProvenance {
            source: ResidentRecordSource::LocalUiForSyndic(source_provenance(
                name,
                position,
                Some(position..position + 8),
            )),
            projection_id: Some(projection_id),
            projection_revision: Some(ProviderRevision(0)),
            presentation_revision,
            copy_source_range: Some(position..position + 8),
        },
        estimated_bytes: 16,
    }
}

fn missing_context_menu_provenance_record(
    presentation_revision: u64,
    name: &str,
    position: u64,
) -> ResidentPresentationRecord {
    let mut record = text_record(
        presentation_revision,
        name,
        position,
        Some(position..position + 8),
    );
    let ResidentRecordSource::Syndic(source) = &mut record.provenance.source else {
        unreachable!("test text record should have Syndic provenance");
    };
    source.projection_id = None;
    record
}

fn narrative_payload(text: &str) -> std::sync::Arc<syndic_storage::ProjectionPayload> {
    std::sync::Arc::new(
        syndic_storage::ProjectionPayload::inline_markdown(
            syndic_storage::MarkdownBlockId::from_bytes([29; 32]),
            syndic_storage::MarkdownBlockKind::Paragraph,
            1,
            syndic_storage::ProjectionSourceRange::new(20, 20 + text.len() as u64).unwrap(),
            text,
        )
        .unwrap(),
    )
}

#[test]
fn prepared_narrative_context_menu_requires_authored_nonempty_stable_source() {
    let payload = narrative_payload("**menu** text");
    let prepared = std::sync::Arc::new(
        syndic_storage::narrative::prepare_narrative(payload.as_ref().clone(), usize::MAX).unwrap(),
    );
    let mut record = text_record(13, "prepared", 20, Some(20..33));
    record.kind = ResidentPresentationRecordKind::NarrativeReady { prepared };
    let geometry = ResidentSelectionRecordGeometry::new(record.id.clone(), 0., 32.);
    let target = resident_context_menu_record(&record, geometry.clone()).unwrap();
    assert_eq!(
        target.content_kind,
        ResidentContextMenuContentKind::TextChunk
    );
    assert_eq!(
        target.source,
        source_provenance("prepared", 20, Some(20..33))
    );

    record.kind = ResidentPresentationRecordKind::NarrativePending {
        source: payload.clone(),
    };
    assert!(matches!(
        resident_context_menu_record(&record, geometry.clone()),
        Err(ResidentContextMenuUnavailable::NonContentRecord { .. })
    ));
    record.kind = ResidentPresentationRecordKind::NarrativeUnavailable {
        source: payload,
        reason: syndic_storage::narrative::NarrativePreparationErrorKind::Capacity,
    };
    assert!(matches!(
        resident_context_menu_record(&record, geometry.clone()),
        Err(ResidentContextMenuUnavailable::NonContentRecord { .. })
    ));

    record.kind = ResidentPresentationRecordKind::TextChunk {
        narrative_kind: TranscriptNarrativeKind::UserInput,
        text: "".into(),
        plain_text: None,
    };
    assert!(matches!(
        resident_context_menu_record(&record, geometry.clone()),
        Err(ResidentContextMenuUnavailable::NonContentRecord { .. })
    ));

    record.kind = ResidentPresentationRecordKind::NarrativeReady {
        prepared: std::sync::Arc::new(
            syndic_storage::narrative::prepare_narrative(
                narrative_payload("menu").as_ref().clone(),
                usize::MAX,
            )
            .unwrap(),
        ),
    };
    record.provenance.source = ResidentRecordSource::LocalUi;
    assert!(matches!(
        resident_context_menu_record(&record, geometry.clone()),
        Err(ResidentContextMenuUnavailable::NonContentRecord { .. })
    ));
    record.provenance.source =
        ResidentRecordSource::Syndic(source_provenance("prepared", 20, None));
    let ResidentRecordSource::Syndic(source) = &mut record.provenance.source else {
        unreachable!()
    };
    source.item_id = None;
    assert!(matches!(
        resident_context_menu_record(&record, geometry),
        Err(ResidentContextMenuUnavailable::MissingStableProvenance { .. })
    ));
}

#[test]
fn historical_menu_source_requires_exact_entry_identity_and_position() {
    let thread = beryl_model::SyndicThreadId::from_bytes([31; 16]);
    let turn = beryl_model::SyndicTurnId::from_bytes([32; 16]);
    let item = beryl_model::SyndicItemId::from_bytes([33; 16]);
    let mut record = text_record(13, "historical", 0, Some(0..8));
    let ResidentRecordSource::Syndic(source) = &mut record.provenance.source else {
        panic!()
    };
    source.view_id = TranscriptViewId(thread.to_string());
    source.turn_id = Some(SyndicTurnId(turn.to_string()));
    source.item_id = Some(SyndicItemId(item.to_string()));
    source.position = Some(TranscriptViewPosition(
        syndic_storage::TranscriptPosition::FIRST.get(),
    ));
    let proof = syndic_storage::CurrentTranscriptEntryProof::new(
        syndic_storage::TranscriptGeneration::FIRST,
        syndic_storage::TranscriptPosition::FIRST,
    );
    source.transcript_entry = Some(proof);
    let geometry = ResidentSelectionRecordGeometry::new(record.id.clone(), 0., 32.);
    let mut target = resident_context_menu_record(&record, geometry).unwrap();
    let captured = target.historical_target().unwrap();
    assert_eq!(
        (
            captured.thread_id,
            captured.turn_id,
            captured.item_id,
            captured.transcript
        ),
        (thread, turn, item, proof)
    );
    target.source.position = Some(TranscriptViewPosition(proof.position().get() + 1));
    assert!(target.historical_target().is_none());
    target.source.position = Some(TranscriptViewPosition(proof.position().get()));
    target.source.transcript_entry = None;
    assert!(target.historical_target().is_none());
    target.source.transcript_entry = Some(proof);
    target.source.item_id = Some(SyndicItemId("transport-item".into()));
    assert!(target.historical_target().is_none());
}

#[test]
fn inline_table_menu_keeps_exact_canonical_source_without_narrative_geometry() {
    let text = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let payload = syndic_storage::ProjectionPayload::inline_markdown(
        syndic_storage::MarkdownBlockId::from_bytes([34; 32]),
        syndic_storage::MarkdownBlockKind::Table,
        1,
        syndic_storage::ProjectionSourceRange::new(0, text.len() as u64).unwrap(),
        text,
    )
    .unwrap();
    let mut record = text_record(13, "table", 20, None);
    record.kind = ResidentPresentationRecordKind::InlineTable {
        prepared: std::sync::Arc::new(
            syndic_storage::inline_table::prepare_inline_table(payload, 131_072).unwrap(),
        ),
    };
    let geometry = ResidentSelectionRecordGeometry::new(record.id.clone(), 0., 64.);
    let target = resident_context_menu_record(&record, geometry).unwrap();
    assert_eq!(
        target.content_kind,
        ResidentContextMenuContentKind::InlineTable
    );
    assert_eq!(target.source, source_provenance("table", 20, None));
    assert_eq!(target.record_id, record.id);
    record.kind = ResidentPresentationRecordKind::InlineTableUnavailable {
        source: text.into(),
        reason: syndic_storage::inline_table::InlineTablePreparationErrorKind::Capacity,
    };
    assert!(resident_context_menu_record(&record, target.geometry).is_err());
}

#[test]
fn context_menu_user_role_requires_the_exact_current_resident_view_source() {
    let record = text_record(13, "exact-user", 20, Some(20..28));
    let target = resident_context_menu_record(
        &record,
        ResidentSelectionRecordGeometry::new(record.id.clone(), 0., 32.),
    )
    .unwrap();
    let view = TranscriptViewRecord {
        id: TranscriptViewRecordId("exact-view-record".into()),
        position: TranscriptViewPosition(20),
        projection_id: target.projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::UserInput,
        provenance: target.source.clone(),
    };
    assert_eq!(
        resident_context_menu_narrative_kind(&target, &[view.clone()]),
        Some(TranscriptNarrativeKind::UserInput)
    );
    assert_eq!(resident_context_menu_narrative_kind(&target, &[]), None);
    let mut assistant = view.clone();
    assistant.narrative_kind = TranscriptNarrativeKind::AssistantFinalAnswer;
    assert_eq!(
        resident_context_menu_narrative_kind(&target, &[assistant]),
        Some(TranscriptNarrativeKind::AssistantFinalAnswer)
    );
    let mut changed_item = view.clone();
    changed_item.provenance.item_id = Some(SyndicItemId("successor-user-item".into()));
    let mut changed_turn = view.clone();
    changed_turn.provenance.turn_id = Some(SyndicTurnId("successor-user-turn".into()));
    let mut changed_range = view.clone();
    changed_range.provenance.source_range = Some(40..48);
    let mut changed_view = view.clone();
    changed_view.provenance.view_id = TranscriptViewId("other-thread".into());
    let mut changed_position = view;
    changed_position.position = TranscriptViewPosition(21);
    for obsolete in [
        changed_item,
        changed_turn,
        changed_range,
        changed_view,
        changed_position,
    ] {
        assert_eq!(
            resident_context_menu_narrative_kind(&target, &[obsolete]),
            None
        );
    }
}

fn snapshot_with_records(
    presentation_revision: u64,
    records: Vec<ResidentPresentationRecord>,
) -> ResidentTranscriptSnapshot {
    ResidentTranscriptSnapshot {
        activation_revision: 1,
        presentation_revision,
        state: ResidentTranscriptSnapshotState::ProviderBacked {
            label: "renderer-selection-test".to_string(),
        },
        records: records.into(),
        resources: ResidentResourceSnapshot::default(),
        realized_range: None,
        visible_range: None,
    }
}

fn frame_record(
    snapshot: &ResidentTranscriptSnapshot,
    index: usize,
    top_px: f32,
    height_px: f32,
) -> RealizedFrameRecord {
    RealizedFrameRecord {
        index,
        record_id: snapshot.records[index].id.clone(),
        top_px,
        height_px,
    }
}

fn frame_window(
    presentation_revision: u64,
    records: Vec<RealizedFrameRecord>,
) -> RealizedFrameWindow {
    let end = records.iter().map(|record| record.index).max().unwrap_or(0) + records.len().min(1);
    RealizedFrameWindow {
        presentation_revision,
        records,
        visible_range: 0..end,
        overscan_range: 0..end,
        anchor: None,
        clamp: None,
        manual_delta_px: 0.0,
        manual_scroll_total_px: 0.0,
        demand_facts: Vec::new(),
    }
}

fn text_snapshot() -> ResidentTranscriptSnapshot {
    snapshot_with_records(
        7,
        vec![
            text_record(7, "a", 10, Some(10..18)),
            text_record(7, "b", 20, Some(20..28)),
            text_record(7, "c", 30, Some(30..38)),
        ],
    )
}

#[test]
fn renderer_selection_command_uses_only_realized_record_geometry() {
    let snapshot = text_snapshot();
    let frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
        ],
    );

    let command =
        resident_selection_command_for_realized_record_ids(&snapshot, &frame, &[record_id("b")])
            .expect("realized content record should produce selection command");

    assert_eq!(
        command.presentation_revision,
        snapshot.presentation_revision
    );
    assert_eq!(
        command.records,
        vec![ResidentSelectionRecordGeometry::new(
            record_id("b"),
            12.0,
            12.0
        )]
    );
    assert_eq!(
        resident_selection_command_for_realized_record_ids(&snapshot, &frame, &[record_id("c")]),
        Err(ResidentSelectionUnavailable::RecordNotRealized {
            record_id: record_id("c")
        })
    );
    assert_eq!(
        resident_selection_command_for_realized_record_ids(
            &snapshot,
            &frame,
            &[ResidentPresentationRecordId("missing".to_string())],
        ),
        Err(ResidentSelectionUnavailable::RecordNotResident {
            record_id: ResidentPresentationRecordId("missing".to_string())
        })
    );
}

#[test]
fn renderer_selection_command_rejects_stale_and_unstable_frame_facts() {
    let snapshot = text_snapshot();
    let mut frame = frame_window(
        snapshot.presentation_revision.saturating_sub(1),
        vec![frame_record(&snapshot, 0, 0.0, 12.0)],
    );

    assert_eq!(
        resident_selection_command_for_realized_record_ids(&snapshot, &frame, &[record_id("a")]),
        Err(ResidentSelectionUnavailable::StalePresentationRevision {
            observed: snapshot.presentation_revision.saturating_sub(1),
            current: snapshot.presentation_revision,
        },)
    );

    frame.presentation_revision = snapshot.presentation_revision;
    frame.records[0].height_px = 0.0;
    assert_eq!(
        resident_selection_command_for_realized_record_ids(&snapshot, &frame, &[record_id("a")]),
        Err(ResidentSelectionUnavailable::UnstableGeometry {
            record_id: record_id("a")
        })
    );

    frame.records[0].height_px = 12.0;
    frame.records[0].record_id = record_id("b");
    assert_eq!(
        resident_selection_command_for_realized_record_ids(&snapshot, &frame, &[record_id("b")]),
        Err(ResidentSelectionUnavailable::StaleRecord {
            record_id: record_id("b")
        })
    );
}

#[test]
fn renderer_selectable_ids_require_rendered_content_provenance() {
    let snapshot = snapshot_with_records(
        9,
        vec![
            text_record(9, "content", 10, Some(10..18)),
            fallback_record(9, "fallback", 20),
            text_record(9, "missing-copy", 30, None),
        ],
    );
    let frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
            frame_record(&snapshot, 2, 24.0, 12.0),
        ],
    );

    assert_eq!(
        realized_resident_selectable_record_ids(&snapshot, &frame),
        vec![record_id("content")]
    );
}

#[test]
fn active_selection_reports_frame_loss_when_geometry_stops_being_realized() {
    let snapshot = text_snapshot();
    let stable_frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
        ],
    );
    let selection = ResidentTranscriptSelection::new(
        snapshot.presentation_revision,
        vec![resident_selected_record(&snapshot.records[1]).expect("record b should be selectable")],
    );

    assert_eq!(
        resident_selection_frame_loss(&snapshot, &stable_frame, &selection),
        None
    );

    let missing_frame = frame_window(
        snapshot.presentation_revision,
        vec![frame_record(&snapshot, 0, 0.0, 12.0)],
    );
    assert_eq!(
        resident_selection_frame_loss(&snapshot, &missing_frame, &selection),
        Some(ResidentSelectionUnavailable::RecordNotRealized {
            record_id: record_id("b")
        })
    );

    let unstable_frame = frame_window(
        snapshot.presentation_revision,
        vec![frame_record(&snapshot, 1, 12.0, f32::NAN)],
    );
    assert_eq!(
        resident_selection_frame_loss(&snapshot, &unstable_frame, &selection),
        Some(ResidentSelectionUnavailable::UnstableGeometry {
            record_id: record_id("b")
        })
    );
}

#[test]
fn renderer_quote_command_uses_only_realized_record_geometry() {
    let snapshot = text_snapshot();
    let frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
        ],
    );

    let command =
        resident_quote_command_for_realized_record_ids(&snapshot, &frame, &[record_id("b")])
            .expect("realized content record should produce quote command");

    assert_eq!(
        command,
        ResidentQuoteCommand::new(
            snapshot.presentation_revision,
            vec![ResidentSelectionRecordGeometry::new(
                record_id("b"),
                12.0,
                12.0,
            )],
        )
    );
    assert_eq!(
        resident_quote_command_for_realized_record_ids(&snapshot, &frame, &[record_id("c")]),
        Err(ResidentSelectionUnavailable::RecordNotRealized {
            record_id: record_id("c")
        })
    );
    assert_eq!(
        resident_quote_command_for_realized_record_ids(
            &snapshot,
            &frame,
            &[ResidentPresentationRecordId("missing".to_string())],
        ),
        Err(ResidentSelectionUnavailable::RecordNotResident {
            record_id: ResidentPresentationRecordId("missing".to_string())
        })
    );
}

#[test]
fn renderer_quote_command_rejects_stale_unstable_and_noncontent_frame_facts() {
    let snapshot = text_snapshot();
    let mut frame = frame_window(
        snapshot.presentation_revision.saturating_sub(1),
        vec![frame_record(&snapshot, 0, 0.0, 12.0)],
    );

    assert_eq!(
        resident_quote_command_for_realized_record_ids(&snapshot, &frame, &[record_id("a")]),
        Err(ResidentSelectionUnavailable::StalePresentationRevision {
            observed: snapshot.presentation_revision.saturating_sub(1),
            current: snapshot.presentation_revision,
        },)
    );

    frame.presentation_revision = snapshot.presentation_revision;
    frame.records[0].height_px = 0.0;
    assert_eq!(
        resident_quote_command_for_realized_record_ids(&snapshot, &frame, &[record_id("a")]),
        Err(ResidentSelectionUnavailable::UnstableGeometry {
            record_id: record_id("a")
        })
    );

    let invalid_snapshot = snapshot_with_records(
        11,
        vec![
            fallback_record(11, "fallback", 10),
            text_record(11, "missing-copy", 20, None),
        ],
    );
    let invalid_frame = frame_window(
        invalid_snapshot.presentation_revision,
        vec![
            frame_record(&invalid_snapshot, 0, 0.0, 12.0),
            frame_record(&invalid_snapshot, 1, 12.0, 12.0),
        ],
    );
    assert_eq!(
        realized_resident_quotable_record_ids(&invalid_snapshot, &invalid_frame),
        Vec::<ResidentPresentationRecordId>::new()
    );
    assert!(matches!(
        resident_quote_command_for_realized_record_ids(
            &invalid_snapshot,
            &invalid_frame,
            &[record_id("fallback")],
        ),
        Err(ResidentSelectionUnavailable::NonContentRecord { .. })
    ));
    assert!(matches!(
        resident_quote_command_for_realized_record_ids(
            &invalid_snapshot,
            &invalid_frame,
            &[record_id("missing-copy")],
        ),
        Err(ResidentSelectionUnavailable::MissingCopySource { .. })
    ));
}

#[test]
fn active_quote_target_reports_frame_loss_when_geometry_stops_being_realized() {
    let snapshot = text_snapshot();
    let stable_frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
        ],
    );
    let target = ResidentTranscriptQuoteTarget::new(
        snapshot.presentation_revision,
        vec![resident_selected_record(&snapshot.records[1]).expect("record b should be quoteable")],
    );

    assert_eq!(
        resident_quote_frame_loss(&snapshot, &stable_frame, &target),
        None
    );

    let missing_frame = frame_window(
        snapshot.presentation_revision,
        vec![frame_record(&snapshot, 0, 0.0, 12.0)],
    );
    assert_eq!(
        resident_quote_frame_loss(&snapshot, &missing_frame, &target),
        Some(ResidentSelectionUnavailable::RecordNotRealized {
            record_id: record_id("b")
        })
    );

    let unstable_frame = frame_window(
        snapshot.presentation_revision,
        vec![frame_record(&snapshot, 1, 12.0, f32::NAN)],
    );
    assert_eq!(
        resident_quote_frame_loss(&snapshot, &unstable_frame, &target),
        Some(ResidentSelectionUnavailable::UnstableGeometry {
            record_id: record_id("b")
        })
    );
}

#[test]
fn renderer_context_menu_command_uses_only_realized_record_geometry() {
    let snapshot = text_snapshot();
    let frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
        ],
    );

    let command =
        resident_context_menu_command_for_realized_record_id(&snapshot, &frame, &record_id("b"))
            .expect("realized content record should produce context-menu command");

    assert_eq!(
        command,
        ResidentContextMenuCommand::new(
            snapshot.presentation_revision,
            ResidentSelectionRecordGeometry::new(record_id("b"), 12.0, 12.0),
        )
    );
    assert_eq!(
        resident_context_menu_command_for_realized_record_id(&snapshot, &frame, &record_id("c")),
        Err(ResidentContextMenuUnavailable::RecordNotRealized {
            record_id: record_id("c")
        })
    );
    assert_eq!(
        resident_context_menu_command_for_realized_record_id(
            &snapshot,
            &frame,
            &ResidentPresentationRecordId("missing".to_string()),
        ),
        Err(ResidentContextMenuUnavailable::RecordNotResident {
            record_id: ResidentPresentationRecordId("missing".to_string())
        })
    );
}

#[test]
fn renderer_context_menu_record_ids_require_rendered_content_provenance() {
    let snapshot = snapshot_with_records(
        13,
        vec![
            text_record(13, "content", 10, Some(10..18)),
            resource_record(
                13,
                "resource",
                20,
                ResourceId("image-resource".to_string()),
                ResourceKind::GeneratedImage,
            ),
            fallback_record(13, "fallback", 30),
            missing_context_menu_provenance_record(13, "missing-provenance", 40),
        ],
    );
    let frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
            frame_record(&snapshot, 2, 24.0, 12.0),
            frame_record(&snapshot, 3, 36.0, 12.0),
        ],
    );

    assert_eq!(
        realized_resident_context_menu_record_ids(&snapshot, &frame),
        vec![record_id("content"), record_id("resource")]
    );
}

#[test]
fn renderer_context_menu_command_rejects_stale_unstable_and_noncontent_frame_facts() {
    let snapshot = text_snapshot();
    let mut frame = frame_window(
        snapshot.presentation_revision.saturating_sub(1),
        vec![frame_record(&snapshot, 0, 0.0, 12.0)],
    );

    assert_eq!(
        resident_context_menu_command_for_realized_record_id(&snapshot, &frame, &record_id("a")),
        Err(ResidentContextMenuUnavailable::StalePresentationRevision {
            observed: snapshot.presentation_revision.saturating_sub(1),
            current: snapshot.presentation_revision,
        },)
    );

    frame.presentation_revision = snapshot.presentation_revision;
    frame.records[0].height_px = 0.0;
    assert_eq!(
        resident_context_menu_command_for_realized_record_id(&snapshot, &frame, &record_id("a")),
        Err(ResidentContextMenuUnavailable::UnstableGeometry {
            record_id: record_id("a")
        })
    );

    frame.records[0].height_px = 12.0;
    frame.records[0].record_id = record_id("b");
    assert_eq!(
        resident_context_menu_command_for_realized_record_id(&snapshot, &frame, &record_id("b")),
        Err(ResidentContextMenuUnavailable::RecordNotRealized {
            record_id: record_id("b")
        })
    );
    assert!(realized_resident_context_menu_record_ids(&snapshot, &frame).is_empty());
    frame.records[0].index = 1;
    assert_eq!(
        resident_context_menu_command_for_realized_record_id(&snapshot, &frame, &record_id("b")),
        Ok(ResidentContextMenuCommand::new(
            snapshot.presentation_revision,
            ResidentSelectionRecordGeometry::new(record_id("b"), 0.0, 12.0),
        ))
    );

    let invalid_snapshot = snapshot_with_records(
        15,
        vec![
            fallback_record(15, "fallback", 10),
            missing_context_menu_provenance_record(15, "missing-provenance", 20),
        ],
    );
    let invalid_frame = frame_window(
        invalid_snapshot.presentation_revision,
        vec![
            frame_record(&invalid_snapshot, 0, 0.0, 12.0),
            frame_record(&invalid_snapshot, 1, 12.0, 12.0),
        ],
    );
    assert!(matches!(
        resident_context_menu_command_for_realized_record_id(
            &invalid_snapshot,
            &invalid_frame,
            &record_id("fallback"),
        ),
        Err(ResidentContextMenuUnavailable::NonContentRecord { .. })
    ));
    assert!(matches!(
        resident_context_menu_command_for_realized_record_id(
            &invalid_snapshot,
            &invalid_frame,
            &record_id("missing-provenance"),
        ),
        Err(ResidentContextMenuUnavailable::MissingStableProvenance { .. })
    ));
}

#[test]
fn active_context_menu_target_reports_frame_loss_when_geometry_stops_being_realized() {
    let snapshot = text_snapshot();
    let stable_frame = frame_window(
        snapshot.presentation_revision,
        vec![
            frame_record(&snapshot, 0, 0.0, 12.0),
            frame_record(&snapshot, 1, 12.0, 12.0),
        ],
    );
    let target = ResidentTranscriptContextMenuTarget::new(
        snapshot.presentation_revision,
        resident_context_menu_record(
            &snapshot.records[1],
            ResidentSelectionRecordGeometry::new(record_id("b"), 12.0, 12.0),
        )
        .expect("record b should be menu targetable"),
    );

    assert_eq!(
        resident_context_menu_frame_loss(&snapshot, &stable_frame, &target),
        None
    );

    let mut replaced_snapshot = snapshot.clone();
    let mut replaced_records = replaced_snapshot.records.to_vec();
    let ResidentRecordSource::Syndic(source) = &mut replaced_records[1].provenance.source else {
        unreachable!()
    };
    source.item_id = Some(SyndicItemId("replacement-item".into()));
    replaced_snapshot.records = replaced_records.into();
    assert_eq!(
        resident_context_menu_frame_loss(&replaced_snapshot, &stable_frame, &target),
        Some(ResidentContextMenuUnavailable::StaleRecord {
            record_id: record_id("b")
        })
    );

    let mut moved_frame = stable_frame.clone();
    moved_frame.records[1].top_px += 1.;
    assert_eq!(
        resident_context_menu_frame_loss(&snapshot, &moved_frame, &target),
        Some(ResidentContextMenuUnavailable::StaleRecord {
            record_id: record_id("b")
        })
    );

    let mut revised_snapshot = snapshot.clone();
    revised_snapshot.presentation_revision += 1;
    assert!(matches!(
        resident_context_menu_frame_loss(&revised_snapshot, &stable_frame, &target),
        Some(ResidentContextMenuUnavailable::StalePresentationRevision { .. })
    ));

    let missing_frame = frame_window(
        snapshot.presentation_revision,
        vec![frame_record(&snapshot, 0, 0.0, 12.0)],
    );
    assert_eq!(
        resident_context_menu_frame_loss(&snapshot, &missing_frame, &target),
        Some(ResidentContextMenuUnavailable::RecordNotRealized {
            record_id: record_id("b")
        })
    );

    let unstable_frame = frame_window(
        snapshot.presentation_revision,
        vec![frame_record(&snapshot, 1, 12.0, f32::NAN)],
    );
    assert_eq!(
        resident_context_menu_frame_loss(&snapshot, &unstable_frame, &target),
        Some(ResidentContextMenuUnavailable::UnstableGeometry {
            record_id: record_id("b")
        })
    );
}
