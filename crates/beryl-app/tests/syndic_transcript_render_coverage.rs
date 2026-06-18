use std::ops::Range;

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(61);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("thread-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn resource_id(name: &str) -> ResourceId {
    ResourceId(format!("resource-{name}"))
}

fn provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
    resource_id: Option<ResourceId>,
    source_range: Option<Range<u64>>,
    resource_range: Option<Range<u64>>,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(SyndicTurnId(format!("turn-{position}"))),
        item_id: Some(SyndicItemId(format!("item-{position}"))),
        projection_id: Some(projection_id.clone()),
        resource_id,
        source_range: source_range.clone(),
        resource_range,
        copy_source_range: source_range,
    }
}

fn view_record(
    view_id: &TranscriptViewId,
    position: u64,
    name: &str,
    projection_id: ProjectionRecordId,
    narrative_kind: TranscriptNarrativeKind,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(format!("record-{name}")),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind,
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            None,
            Some(position..position + 10),
            None,
        ),
    }
}

fn text_projection(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: ProjectionRecordId,
    text: impl Into<String>,
) -> ProjectionRecord {
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text { text: text.into() },
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            None,
            Some(position..position + 10),
            None,
        ),
    }
}

fn resource_projection(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: ProjectionRecordId,
    resource_id: ResourceId,
    kind: ResourceKind,
) -> ProjectionRecord {
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::ResourceReference,
        payload: ProjectionPayload::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind: kind,
            label: Some("resident resource".to_string()),
        },
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            Some(resource_id),
            Some(position..position + 1),
            Some(0..16),
        ),
    }
}

fn resource_metadata(resource_id: ResourceId, kind: ResourceKind) -> ResourceMetadata {
    ResourceMetadata {
        resource_id,
        revision: ProviderRevision(0),
        kind,
        media_type: Some("application/octet-stream".to_string()),
        byte_len: 0,
        digest: Some("sha256:resident-resource".to_string()),
        line_count: None,
        row_count: None,
        column_count: None,
        preview_range: Some(0..4),
    }
}

fn handle_provider_request(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
    request: TranscriptProviderRequest,
) -> ResidentProviderResponseEffect {
    let response = provider
        .handle_request(request)
        .expect("fixture provider request should not fail");
    core.handle_provider_response(response)
}

fn admit_start_page(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
    view_id: &TranscriptViewId,
) {
    let request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    match handle_provider_request(core, provider, request) {
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count } => {
            assert!(admitted_count > 0);
        }
        other => panic!("expected view page admission, got {other:?}"),
    }
}

fn admit_projection_records(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
) -> ResidentProviderResponseEffect {
    let request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("resident view should have projection demand");
    handle_provider_request(core, provider, request)
}

fn frame_request(
    snapshot: &ResidentTranscriptSnapshot,
    manual_delta_px: f32,
) -> RealizedFrameRequest {
    RealizedFrameRequest {
        viewport_height_px: 32.0,
        overscan_height_px: 16.0,
        default_record_height_px: 16.0,
        manual_delta_px,
        observed_presentation_revision: Some(snapshot.presentation_revision),
    }
}

fn realize_snapshot(
    snapshot: &ResidentTranscriptSnapshot,
    manual_delta_px: f32,
) -> RealizedFrameWindow {
    let mut controller = RealizedFrameScrollController::new();
    controller.realize(snapshot, frame_request(snapshot, manual_delta_px))
}

fn resident_rows_for_window<'a>(
    snapshot: &'a ResidentTranscriptSnapshot,
    window: &RealizedFrameWindow,
) -> Vec<&'a ResidentPresentationRecord> {
    let mut rows = Vec::new();
    for frame_record in &window.records {
        let record = snapshot
            .records
            .get(frame_record.index)
            .expect("realized frame record index should be resident");
        assert_eq!(record.id, frame_record.record_id);
        rows.push(record);
    }
    rows
}

fn seed_text_core(policy: ResidentTranscriptPolicy, texts: &[&str]) -> ResidentTranscriptCore {
    let view_id = view_id();
    let mut provider = InMemorySyndicTranscriptProvider::new();
    let mut view_records = Vec::new();

    provider.set_revision(REVISION);
    for (index, text) in texts.iter().enumerate() {
        let position = (index as u64 + 1) * 10;
        let id = projection_id(&format!("text-{index}"));
        view_records.push(view_record(
            &view_id,
            position,
            &format!("text-{index}"),
            id.clone(),
            TranscriptNarrativeKind::AssistantCommentary,
        ));
        provider.insert_projection_record(text_projection(&view_id, position, id, *text));
    }
    provider.insert_view_records(view_id.clone(), view_records);

    let mut core = ResidentTranscriptCore::new(policy);
    admit_start_page(&mut core, &mut provider, &view_id);
    assert_eq!(
        admit_projection_records(&mut core, &mut provider),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: texts.len(),
            rejected_count: 0
        }
    );
    core
}

fn presentation_texts(records: &[&ResidentPresentationRecord]) -> Vec<String> {
    records
        .iter()
        .filter_map(|record| match &record.kind {
            ResidentPresentationRecordKind::TextChunk { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn has_demand_fact(facts: &[DemandFact], predicate: impl Fn(&DemandFactKind) -> bool) -> bool {
    facts.iter().any(|fact| predicate(&fact.kind))
}

#[test]
fn empty_resident_snapshot_realizes_no_transcript_rows() {
    let core = ResidentTranscriptCore::empty();
    let snapshot = core.presentation_snapshot();

    let window = realize_snapshot(&snapshot, 0.0);
    let rows = resident_rows_for_window(&snapshot, &window);

    assert!(matches!(
        snapshot.state,
        ResidentTranscriptSnapshotState::Empty
    ));
    assert!(snapshot.records.is_empty());
    assert!(window.records.is_empty());
    assert!(rows.is_empty());
    assert!(window.demand_facts.iter().any(|fact| {
        fact.kind
            == DemandFactKind::CurrentAnchor {
                record_id: None,
                position: None,
            }
    }));
}

#[test]
fn fixture_backed_snapshot_realizes_only_resident_presentation_records() {
    let core = seed_text_core(
        ResidentTranscriptPolicy::default(),
        &["resident alpha", "resident beta"],
    );
    let snapshot = core.presentation_snapshot();

    let window = realize_snapshot(&snapshot, 0.0);
    let rows = resident_rows_for_window(&snapshot, &window);

    assert!(matches!(
        snapshot.state,
        ResidentTranscriptSnapshotState::ProviderBacked { .. }
    ));
    assert_eq!(rows.len(), snapshot.records.len());
    assert_eq!(
        rows.iter()
            .map(|record| record.id.clone())
            .collect::<Vec<_>>(),
        snapshot
            .records
            .iter()
            .map(|record| record.id.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        presentation_texts(&rows),
        vec!["resident alpha".to_string(), "resident beta".to_string()]
    );
    assert!(
        rows.iter()
            .all(|record| matches!(record.provenance.source, ResidentRecordSource::Syndic(_)))
    );
}

#[test]
fn bounded_snapshot_realizes_only_policy_admitted_records() {
    let policy = ResidentTranscriptPolicy {
        max_presentation_records: 2,
        ..ResidentTranscriptPolicy::default()
    };
    let core = seed_text_core(
        policy,
        &[
            "resident zero",
            "resident one",
            "not presentation two",
            "not presentation three",
        ],
    );
    let snapshot = core.presentation_snapshot();

    let window = realize_snapshot(&snapshot, 0.0);
    let rows = resident_rows_for_window(&snapshot, &window);

    assert_eq!(snapshot.record_count(), 2);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        presentation_texts(&rows),
        vec!["resident zero".to_string(), "resident one".to_string()]
    );
    assert!(
        !presentation_texts(&rows)
            .iter()
            .any(|text| text.contains("not presentation"))
    );
}

#[test]
fn rejected_projection_realizes_local_fallback_without_synthetic_content() {
    let view_id = view_id();
    let present_id = projection_id("present");
    let rejected_id = projection_id("rejected");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![
                view_record(
                    &view_id,
                    10,
                    "present",
                    present_id.clone(),
                    TranscriptNarrativeKind::AssistantCommentary,
                ),
                view_record(
                    &view_id,
                    20,
                    "rejected",
                    rejected_id.clone(),
                    TranscriptNarrativeKind::AssistantCommentary,
                ),
            ],
        )
        .insert_projection_record(text_projection(
            &view_id,
            10,
            present_id,
            "resident visible text",
        ))
        .reject_projection_record_with_message(
            rejected_id.clone(),
            TranscriptProviderRejectionReason::BudgetExceeded,
            "blocked projection body",
        );

    let mut core = ResidentTranscriptCore::empty();
    admit_start_page(&mut core, &mut provider, &view_id);
    assert_eq!(
        admit_projection_records(&mut core, &mut provider),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 1,
            rejected_count: 1
        }
    );
    let snapshot = core.presentation_snapshot();

    let window = realize_snapshot(&snapshot, 0.0);
    let rows = resident_rows_for_window(&snapshot, &window);

    assert_eq!(snapshot.record_count(), 2);
    assert_eq!(
        presentation_texts(&rows),
        vec!["resident visible text".to_string()]
    );
    assert!(rows.iter().any(|record| {
        record.kind
            == ResidentPresentationRecordKind::LocalUiFallback {
                reason: LocalPresentationReason::BudgetRejected,
                target: ResidentFallbackTarget::ProjectionRecord(rejected_id.clone()),
            }
    }));
    let fallback = rows
        .iter()
        .find(|record| {
            matches!(
                record.kind,
                ResidentPresentationRecordKind::LocalUiFallback { .. }
            )
        })
        .expect("rejected projection should render a local fallback row");
    match &fallback.provenance.source {
        ResidentRecordSource::LocalUiForSyndic(source) => {
            assert_eq!(source.projection_id, Some(rejected_id));
            assert_eq!(source.position, Some(TranscriptViewPosition(20)));
        }
        other => panic!("fallback should be local UI tied to Syndic provenance, got {other:?}"),
    }
}

#[test]
fn resource_budget_fallback_realizes_local_ui_record_without_resource_bytes() {
    let view_id = view_id();
    let projection_id = projection_id("image");
    let resource_id = resource_id("large-image");
    let policy = ResidentTranscriptPolicy {
        max_resource_bytes: 4,
        ..ResidentTranscriptPolicy::default()
    };
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(
                &view_id,
                10,
                "image",
                projection_id.clone(),
                TranscriptNarrativeKind::AssistantGeneratedMedia,
            )],
        )
        .insert_projection_record(resource_projection(
            &view_id,
            10,
            projection_id,
            resource_id.clone(),
            ResourceKind::GeneratedImage,
        ))
        .insert_resource(
            resource_metadata(resource_id.clone(), ResourceKind::GeneratedImage),
            b"oversized image bytes".to_vec(),
        );

    let mut core = ResidentTranscriptCore::new(policy);
    admit_start_page(&mut core, &mut provider, &view_id);
    assert_eq!(
        admit_projection_records(&mut core, &mut provider),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 1,
            rejected_count: 0
        }
    );
    let metadata_request = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .expect("resource reference should demand metadata before budget fallback");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, metadata_request),
        ResidentProviderResponseEffect::ResourceMetadataAdmitted { admitted_count: 1 }
    );
    let snapshot = core.presentation_snapshot();

    let window = realize_snapshot(&snapshot, 0.0);
    let rows = resident_rows_for_window(&snapshot, &window);

    assert_eq!(rows.len(), 1);
    assert_eq!(snapshot.resources.metadata.len(), 1);
    assert!(snapshot.resources.slices.is_empty());
    assert!(matches!(
        rows[0].kind,
        ResidentPresentationRecordKind::LocalUiFallback { .. }
    ));
    assert!(!rows.iter().any(|record| matches!(
        record.kind,
        ResidentPresentationRecordKind::ResourceReference { .. }
    )));
    match &rows[0].provenance.source {
        ResidentRecordSource::LocalUiForSyndic(source) => {
            assert_eq!(source.resource_id, Some(resource_id));
        }
        other => panic!("resource fallback should keep Syndic provenance, got {other:?}"),
    }
}

#[test]
fn realized_frame_demand_facts_update_resident_ranges_and_adjacent_state() {
    let mut core = seed_text_core(
        ResidentTranscriptPolicy::default(),
        &["zero", "one", "two", "three", "four", "five"],
    );
    let mut controller = RealizedFrameScrollController::new();
    let snapshot = core.presentation_snapshot();

    let first_window = controller.realize(&snapshot, frame_request(&snapshot, 0.0));
    for fact in &first_window.demand_facts {
        core.push_demand_fact(fact.clone());
    }
    let first_snapshot = core.core_snapshot();
    assert_eq!(first_snapshot.presentation.visible_range, Some(0..2));
    assert_eq!(first_snapshot.presentation.realized_range, Some(0..3));
    assert!(has_demand_fact(&first_window.demand_facts, |kind| {
        matches!(kind, DemandFactKind::MeasuredRecord { .. })
    }));

    let snapshot = core.presentation_snapshot();
    let second_window = controller.realize(&snapshot, frame_request(&snapshot, 200.0));
    for fact in &second_window.demand_facts {
        core.push_demand_fact(fact.clone());
    }
    let second_snapshot = core.core_snapshot();

    assert!(has_demand_fact(&second_window.demand_facts, |kind| {
        matches!(kind, DemandFactKind::MissingAfter { .. })
    }));
    assert!(has_demand_fact(&second_window.demand_facts, |kind| {
        matches!(
            kind,
            DemandFactKind::AdjacentRange {
                direction: TranscriptPageDirection::Forward,
                ..
            }
        )
    }));
    assert!(has_demand_fact(&second_window.demand_facts, |kind| {
        matches!(kind, DemandFactKind::ObsoleteRange { range } if range == &(0..3))
    }));
    assert_eq!(second_snapshot.resident.obsolete_ranges, vec![0..3]);
}
