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
            Some(position..position + 4),
            None,
        ),
    }
}

fn text_projection(view_id: &TranscriptViewId, position: u64, name: &str) -> ProjectionRecord {
    let projection_id = projection_id(name);
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text {
            text: format!("text-{name}"),
        },
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            None,
            Some(position..position + 4),
            None,
        ),
    }
}

fn resource_projection(
    view_id: &TranscriptViewId,
    position: u64,
    name: &str,
    resource_id: ResourceId,
) -> ProjectionRecord {
    let projection_id = projection_id(name);
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::ResourceReference,
        payload: ProjectionPayload::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind: ResourceKind::Image,
            label: Some(format!("image-{name}")),
        },
        provenance: provenance(
            view_id,
            position,
            &projection_id,
            Some(resource_id),
            Some(position..position + 4),
            Some(0..8),
        ),
    }
}

fn metadata(resource_id: ResourceId) -> ResourceMetadata {
    ResourceMetadata {
        resource_id,
        revision: ProviderRevision(0),
        kind: ResourceKind::Image,
        media_type: Some("image/png".to_string()),
        byte_len: 0,
        digest: Some("sha256:image".to_string()),
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

fn admit_start_page_and_projections(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
    view_id: &TranscriptViewId,
) {
    let page_request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    match handle_provider_request(core, provider, page_request) {
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count } => {
            assert!(admitted_count > 0);
        }
        other => panic!("expected view page admission, got {other:?}"),
    }

    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("resident view should demand projections");
    match handle_provider_request(core, provider, projection_request) {
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted { admitted_count, .. } => {
            assert!(admitted_count > 0);
        }
        other => panic!("expected projection admission, got {other:?}"),
    }
}

fn seed_text_presentation(names: &[&str]) -> ResidentTranscriptCore {
    let view_id = view_id();
    let mut provider = InMemorySyndicTranscriptProvider::new();
    let mut view_records = Vec::new();
    provider.set_revision(REVISION);

    for (index, name) in names.iter().enumerate() {
        let position = ((index + 1) * 10) as u64;
        let projection_id = projection_id(name);
        view_records.push(view_record(
            &view_id,
            position,
            name,
            projection_id,
            TranscriptNarrativeKind::AssistantCommentary,
        ));
        provider.insert_projection_record(text_projection(&view_id, position, name));
    }
    provider.insert_view_records(view_id.clone(), view_records);

    let mut core = ResidentTranscriptCore::new(ResidentTranscriptPolicy {
        view_page_limit: names.len(),
        max_resident_view_records: names.len(),
        max_resident_projection_records: names.len(),
        max_presentation_records: names.len(),
        ..ResidentTranscriptPolicy::default()
    });
    admit_start_page_and_projections(&mut core, &mut provider, &view_id);
    core
}

fn seed_resource_presentation() -> (
    ResidentTranscriptCore,
    InMemorySyndicTranscriptProvider,
    ResourceId,
) {
    let view_id = view_id();
    let text_projection_id = projection_id("text");
    let image_projection_id = projection_id("image");
    let image_resource_id = resource_id("image");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![
                view_record(
                    &view_id,
                    10,
                    "text",
                    text_projection_id.clone(),
                    TranscriptNarrativeKind::AssistantCommentary,
                ),
                view_record(
                    &view_id,
                    20,
                    "image",
                    image_projection_id.clone(),
                    TranscriptNarrativeKind::AssistantGeneratedMedia,
                ),
            ],
        )
        .insert_projection_record(text_projection(&view_id, 10, "text"))
        .insert_projection_record(resource_projection(
            &view_id,
            20,
            "image",
            image_resource_id.clone(),
        ))
        .insert_resource(metadata(image_resource_id.clone()), b"image-bytes".to_vec());

    let mut core = ResidentTranscriptCore::new(ResidentTranscriptPolicy {
        view_page_limit: 2,
        max_resident_view_records: 2,
        max_resident_projection_records: 2,
        max_presentation_records: 2,
        max_resource_bytes: 32,
        max_resource_slice_bytes: 8,
        ..ResidentTranscriptPolicy::default()
    });
    admit_start_page_and_projections(&mut core, &mut provider, &view_id);
    (core, provider, image_resource_id)
}

fn presentation_texts(snapshot: &ResidentCoreSnapshot) -> Vec<&str> {
    snapshot
        .presentation
        .records
        .iter()
        .filter_map(|record| match &record.kind {
            ResidentPresentationRecordKind::TextChunk { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn presentation_record_id_by_text(
    core: &ResidentTranscriptCore,
    expected_text: &str,
) -> ResidentPresentationRecordId {
    core.presentation_snapshot()
        .records
        .into_iter()
        .find_map(|record| match &record.kind {
            ResidentPresentationRecordKind::TextChunk { text, .. } if text == expected_text => {
                Some(record.id)
            }
            _ => None,
        })
        .expect("expected presentation text record")
}

#[test]
fn obsolete_release_preserves_visible_anchor_and_selection_pin() {
    let mut core = seed_text_presentation(&["a", "b", "c", "d"]);
    let presentation_revision = core.presentation_snapshot().presentation_revision;
    let anchor_id = presentation_record_id_by_text(&core, "text-b");
    let selection_id = presentation_record_id_by_text(&core, "text-c");
    let menu_id = presentation_record_id_by_text(&core, "text-d");

    core.push_demand_fact(DemandFact::new(
        presentation_revision,
        DemandFactKind::CurrentAnchor {
            record_id: Some(anchor_id.clone()),
            position: Some(TranscriptViewPosition(20)),
        },
    ));
    core.push_demand_fact(DemandFact::new(
        presentation_revision,
        DemandFactKind::VisibleRange { range: 1..2 },
    ));
    core.push_demand_fact(DemandFact::new(
        presentation_revision,
        DemandFactKind::ActiveSelectionPin {
            record_id: selection_id.clone(),
        },
    ));
    core.push_demand_fact(DemandFact::new(
        presentation_revision,
        DemandFactKind::OpenMenuPin {
            record_id: menu_id.clone(),
        },
    ));
    core.push_demand_fact(DemandFact::new(
        presentation_revision,
        DemandFactKind::ObsoleteRange { range: 0..4 },
    ));

    assert_eq!(core.release_obsolete_resident_data(), 1);

    let snapshot = core.core_snapshot();
    assert_eq!(
        presentation_texts(&snapshot),
        vec!["text-b", "text-c", "text-d"]
    );
    assert_eq!(snapshot.resident.view_record_count, 3);
    assert_eq!(snapshot.resident.projection_record_count, 3);
    assert_eq!(snapshot.resident.current_anchor_record_id, Some(anchor_id));
    assert_eq!(snapshot.resident.active_selection_pins, vec![selection_id]);
    assert_eq!(snapshot.resident.active_menu_pins, vec![menu_id]);
    assert_eq!(snapshot.resident.visible_range, Some(0..1));
    assert_eq!(snapshot.resident.release_decision_count, 1);

    let decision = snapshot
        .resident
        .release_decisions
        .last()
        .expect("release should record a diagnostic decision");
    assert_eq!(
        decision.reason,
        ResidentReleaseReason::ObsoleteResidentRange
    );
    assert_eq!(
        decision.target,
        ResidentReleaseTarget::PresentationRange(0..4)
    );
    assert_eq!(decision.released_presentation_record_count, 1);
    assert_eq!(decision.preserved_presentation_record_count, 3);
    assert_eq!(decision.released_view_record_count, 1);
    assert_eq!(decision.released_projection_record_count, 1);
}

#[test]
fn resource_pin_preserves_metadata_and_slice_during_obsolete_release() {
    let (mut core, mut provider, resource_id) = seed_resource_presentation();
    let metadata_request = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .expect("resource metadata should be demandable");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, metadata_request),
        ResidentProviderResponseEffect::ResourceMetadataAdmitted { admitted_count: 1 }
    );
    let range_request = core
        .request_resource_range(
            resource_id.clone(),
            0..8,
            ProviderRequestReason::ResourceRange,
        )
        .expect("resource range should be demandable");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, range_request),
        ResidentProviderResponseEffect::ResourceRangeAdmitted {
            admitted_count: 1,
            byte_count: 8
        }
    );

    let presentation_revision = core.presentation_snapshot().presentation_revision;
    core.push_demand_fact(DemandFact::new(
        presentation_revision,
        DemandFactKind::MediaPreviewPin {
            resource_id: resource_id.clone(),
        },
    ));
    core.push_demand_fact(DemandFact::new(
        presentation_revision,
        DemandFactKind::ObsoleteRange { range: 0..2 },
    ));

    assert_eq!(core.release_obsolete_resident_data(), 1);

    let snapshot = core.core_snapshot();
    assert_eq!(presentation_texts(&snapshot), Vec::<&str>::new());
    assert_eq!(snapshot.presentation.record_count(), 1);
    assert_eq!(snapshot.resident.resource_metadata_count, 1);
    assert_eq!(snapshot.resident.resource_slice_count, 1);
    assert_eq!(snapshot.resident.resource_slice_bytes, 8);
    assert_eq!(snapshot.resident.active_resource_pins, vec![resource_id]);
    assert_eq!(snapshot.resident.active_pin_count, 1);

    let decision = snapshot
        .resident
        .release_decisions
        .last()
        .expect("release should record a diagnostic decision");
    assert_eq!(decision.released_presentation_record_count, 1);
    assert_eq!(decision.preserved_presentation_record_count, 1);
    assert_eq!(decision.released_resource_metadata_count, 0);
    assert_eq!(decision.released_resource_slice_count, 0);
}

#[test]
fn provider_invalidation_bumps_generation_and_discards_late_response() {
    let view_id = view_id();
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(
                &view_id,
                10,
                "a",
                projection_id("a"),
                TranscriptNarrativeKind::AssistantCommentary,
            )],
        )
        .insert_projection_record(text_projection(&view_id, 10, "a"));

    let mut core = ResidentTranscriptCore::new(ResidentTranscriptPolicy::default());
    let request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    let generation = core.notice_provider_invalidation(ProviderRevision(99));
    assert_eq!(generation, ResidentGeneration(1));

    let response = provider
        .handle_request(request)
        .expect("fixture provider request should not fail");
    assert_eq!(
        core.handle_provider_response(response),
        ResidentProviderResponseEffect::Ignored
    );

    let snapshot = core.core_snapshot();
    assert!(snapshot.resident.view_records.is_empty());
    assert_eq!(
        snapshot.resident.provider_revision,
        Some(ProviderRevision(99))
    );
    assert_eq!(snapshot.provider_requests.pending_count, 0);
    assert_eq!(snapshot.provider_requests.stale_result_count, 1);
    assert_eq!(snapshot.provider_requests.completed_count, 0);
    assert_eq!(snapshot.resident.release_decision_count, 1);
    assert_eq!(
        snapshot.resident.release_decisions[0].reason,
        ResidentReleaseReason::ProviderInvalidation
    );
}

#[test]
fn stale_renderer_fact_records_diagnostic_without_changing_residency() {
    let mut core = seed_text_presentation(&["a", "b"]);
    let presentation_revision = core.presentation_snapshot().presentation_revision;

    core.push_demand_fact(DemandFact::new(
        presentation_revision.saturating_sub(1),
        DemandFactKind::VisibleRange { range: 0..1 },
    ));

    let snapshot = core.core_snapshot();
    assert_eq!(presentation_texts(&snapshot), vec!["text-a", "text-b"]);
    assert_eq!(snapshot.resident.visible_range, None);
    assert_eq!(snapshot.presentation.visible_range, None);
    assert_eq!(snapshot.resident.release_decision_count, 1);
    assert_eq!(
        snapshot.resident.release_decisions[0].reason,
        ResidentReleaseReason::StaleMeasurement
    );
    assert_eq!(
        snapshot.resident.release_decisions[0].target,
        ResidentReleaseTarget::PresentationRevision {
            observed: presentation_revision.saturating_sub(1),
            current: presentation_revision,
        }
    );
}
