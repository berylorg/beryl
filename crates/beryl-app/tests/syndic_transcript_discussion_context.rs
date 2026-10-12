#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;
use std::sync::Arc;
use syndic_transcript_core::*;

fn prepared(chunk_count: usize) -> PreparedTranscriptActivation {
    let view = TranscriptViewId("discussion".into());
    let mut views = Vec::new();
    let mut projections = Vec::new();
    for position in 1..=2 {
        let id = ProjectionRecordId(format!("authored-{position}"));
        let source = SyndicSourceProvenance {
            view_id: view.clone(),
            position: Some(TranscriptViewPosition(position)),
            turn_id: Some(SyndicTurnId(format!("turn-{position}"))),
            item_id: Some(SyndicItemId(format!("item-{position}"))),
            block_id: None,
            projection_id: Some(id.clone()),
            resource_id: None,
            source_range: Some(0..8),
            resource_range: None,
            copy_source_range: Some(0..8),
        };
        views.push(TranscriptViewRecord {
            id: TranscriptViewRecordId(format!("view-{position}")),
            position: TranscriptViewPosition(position),
            projection_id: id.clone(),
            narrative_kind: TranscriptNarrativeKind::UserInput,
            provenance: source.clone(),
        });
        projections.push(ProjectionRecord {
            id,
            revision: ProviderRevision(1),
            kind: ProjectionRecordKind::TextChunk,
            payload: ProjectionPayload::Text {
                text: "authored".into(),
                plain_text: Some("authored".into()),
            },
            provenance: source,
        });
    }
    let mut prepared = PreparedTranscriptActivation::new(
        view.clone(),
        TranscriptActivationPlacement::Start,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: view.clone(),
            revision: ProviderRevision(1),
            history_state: TranscriptProviderHistoryState::Complete,
            records: views,
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        Some(TranscriptProviderResponseKind::ProjectionRecords(
            ProjectionRecordSet {
                view_id: view.clone(),
                revision: ProviderRevision(1),
                records: projections,
                rejections: Vec::new(),
            },
        )),
    );
    prepared.discussion_context = Some(SyntheticContextGroup {
        provenance: Arc::new(SyntheticContextProvenance {
            view_id: view,
            group_id: "immutable-owner-and-parent".into(),
            insertion_after: 1,
            insertion_parent: beryl_model::SyndicTurnId::from_bytes([4; 16]),
            descriptor: None,
            envelope_revision: 1,
            chunk_count: chunk_count.max(1),
        }),
        chunks: (0..chunk_count)
            .map(|index| SyntheticContextChunk {
                range: (index * 12) as u64..((index + 1) * 12) as u64,
                text: "**selected**".into(),
                plain_text: "selected".into(),
            })
            .collect(),
    });
    prepared
}

fn realize(core: &mut ResidentTranscriptCore, height: f32) -> RealizedFrameWindow {
    let mut controller = RealizedFrameScrollController::new();
    let frame = controller.realize(
        &core.presentation_snapshot(),
        RealizedFrameRequest {
            viewport_height_px: height,
            overscan_height_px: 0.0,
            default_record_height_px: 20.0,
            manual_delta_px: 0.0,
            observed_presentation_revision: None,
        },
    );
    for fact in &frame.demand_facts {
        core.push_demand_fact(fact.clone());
    }
    frame
}

#[test]
fn synthetic_selection_has_both_copy_forms_and_rejects_mixed_quote_and_menu() {
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(prepared(1), TranscriptActivationSource::RunningThread);
    let snapshot = core.presentation_snapshot();
    assert!(matches!(
        snapshot.records[1].provenance.source,
        ResidentRecordSource::LocalContext { .. }
    ));
    assert!(matches!(
        snapshot.records[2].provenance.source,
        ResidentRecordSource::Syndic(_)
    ));
    let frame = realize(&mut core, 200.0);
    assert!(matches!(
        core.apply_resident_selection(ResidentSelectionCommand::from_realized_frame_records(
            frame.presentation_revision,
            &frame.records[1..2]
        )),
        ResidentSelectionOutcome::Selected(_)
    ));
    let copied = core.resident_copy_payload().unwrap();
    assert_eq!(copied.markdown, "**selected**");
    assert_eq!(copied.plain_text.as_deref(), Some("selected"));
    assert!(copied.records[0].projection_id.is_none());
    assert!(matches!(
        core.apply_resident_quote_target(ResidentQuoteCommand::from_realized_frame_records(
            frame.presentation_revision,
            &frame.records[0..2]
        )),
        ResidentQuoteOutcome::Unavailable(_)
    ));
    assert_eq!(
        realized_resident_quotable_record_ids(&snapshot, &frame).len(),
        2
    );
    assert!(
        resident_context_menu_command_for_realized_record_id(
            &snapshot,
            &frame,
            &snapshot.records[1].id
        )
        .is_err()
    );
}

#[test]
fn context_realization_is_bounded_and_eviction_releases_payload_and_does_not_resurrect() {
    let mut core = ResidentTranscriptCore::empty();
    let seed = prepared(33);
    let witness = Arc::downgrade(&seed.discussion_context.as_ref().unwrap().chunks[0].text);
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    let frame = realize(&mut core, 60.0);
    assert!(frame.records.len() <= 4);
    assert_eq!(core.presentation_snapshot().records.len(), 35);
    drop(frame);
    let revision = core.presentation_snapshot().presentation_revision;
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::VisibleRange { range: 34..35 },
    ));
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::OverscanRange { range: 34..35 },
    ));
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::CurrentAnchor {
            record_id: None,
            position: None,
        },
    ));
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::ObsoleteRange { range: 0..34 },
    ));
    core.release_obsolete_resident_data();
    assert!(witness.upgrade().is_none());
    assert_eq!(
        core.core_snapshot().resident.presentation_bytes,
        core.presentation_snapshot().records[0].estimated_bytes
    );
    let mut replacement = prepared(0);
    replacement.discussion_context = None;
    core.apply_prepared_activation(replacement, TranscriptActivationSource::RunningThread);
    assert!(
        core.presentation_snapshot()
            .records
            .iter()
            .all(|record| !matches!(
                record.provenance.source,
                ResidentRecordSource::LocalContext { .. }
            ))
    );
    core.apply_prepared_activation(
        PreparedTranscriptActivation::unavailable(TranscriptActivationPlacement::Start),
        TranscriptActivationSource::RunningThread,
    );
    assert!(core.presentation_snapshot().records.is_empty());
    assert_eq!(core.core_snapshot().resident.estimated_resident_bytes, 0);
}

#[test]
fn protecting_one_context_chunk_preserves_whole_group_and_adjacent_authored_paging_row() {
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(prepared(33), TranscriptActivationSource::RunningThread);
    let revision = core.presentation_snapshot().presentation_revision;
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::VisibleRange { range: 16..17 },
    ));
    let protected = core.protected_presentation_record_ids();
    assert_eq!(protected.len(), 34);
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::ObsoleteRange { range: 0..35 },
    ));
    core.release_obsolete_resident_data();
    assert_eq!(core.presentation_snapshot().records.len(), 34);
    assert_eq!(core.core_snapshot().resident.view_record_count, 1);
    assert_eq!(
        core.presentation_snapshot()
            .records
            .iter()
            .filter(|record| matches!(
                record.provenance.source,
                ResidentRecordSource::LocalContext { .. }
            ))
            .count(),
        33
    );
    let revision = core.presentation_snapshot().presentation_revision;
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::ObsoleteRange { range: 1..2 },
    ));
    core.release_obsolete_resident_data();
    assert_eq!(core.presentation_snapshot().records.len(), 34);
}

#[test]
fn context_protection_keeps_exact_boundary_fallback_when_projection_is_rejected() {
    let mut seed = prepared(33);
    if let Some(TranscriptProviderResponseKind::ProjectionRecords(set)) =
        &mut seed.projection_records_response
    {
        let rejected = set.records.remove(0);
        set.rejections.push(TranscriptProviderRejection {
            target: TranscriptProviderTarget::ProjectionRecord(rejected.id),
            reason: TranscriptProviderRejectionReason::BudgetExceeded,
            revision: Some(ProviderRevision(1)),
            message: None,
        });
    }
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    let snapshot = core.presentation_snapshot();
    assert!(matches!(
        snapshot.records[0].provenance.source,
        ResidentRecordSource::LocalUiForSyndic(_)
    ));
    let revision = snapshot.presentation_revision;
    drop(snapshot);
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::VisibleRange { range: 16..17 },
    ));
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::ObsoleteRange { range: 0..35 },
    ));
    core.release_obsolete_resident_data();
    assert_eq!(core.core_snapshot().resident.view_record_count, 1);
    assert_eq!(core.presentation_snapshot().record_count(), 34);
    assert!(matches!(
        core.presentation_snapshot().records[0].provenance.source,
        ResidentRecordSource::LocalUiForSyndic(_)
    ));
}
