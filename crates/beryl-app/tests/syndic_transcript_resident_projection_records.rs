use std::ops::Range;

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(23);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("thread-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
    resource_id: Option<ResourceId>,
    source_range: Option<Range<u64>>,
    resource_range: Option<Range<u64>>,
    copy_source_range: Option<Range<u64>>,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(SyndicTurnId(format!("turn-{position}"))),
        item_id: Some(SyndicItemId(format!("item-{position}"))),
        projection_id: Some(projection_id.clone()),
        resource_id,
        source_range,
        resource_range,
        copy_source_range,
    }
}

fn text_projection(
    id: ProjectionRecordId,
    text: impl Into<String>,
    provenance: SyndicSourceProvenance,
) -> ProjectionRecord {
    ProjectionRecord {
        id,
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text { text: text.into() },
        provenance,
    }
}

fn resource_projection(
    id: ProjectionRecordId,
    resource_id: ResourceId,
    resource_kind: ResourceKind,
    label: Option<&str>,
    provenance: SyndicSourceProvenance,
) -> ProjectionRecord {
    ProjectionRecord {
        id,
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::ResourceReference,
        payload: ProjectionPayload::ResourceReference {
            resource_id,
            resource_kind,
            label: label.map(str::to_string),
        },
        provenance,
    }
}

fn view_record(
    view_id: &TranscriptViewId,
    position: u64,
    id: &str,
    projection_id: ProjectionRecordId,
    narrative_kind: TranscriptNarrativeKind,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(id.to_string()),
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
            Some(position..position + 10),
        ),
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
    let effect = handle_provider_request(core, provider, request);
    match effect {
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count } => {
            assert!(admitted_count > 0);
        }
        other => panic!("expected resident view page admission, got {other:?}"),
    }
}

fn request_resident_projections(core: &mut ResidentTranscriptCore) -> TranscriptProviderRequest {
    core.request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("resident view should have missing projection records")
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

#[test]
fn projection_records_build_presentation_records_with_syndic_provenance() {
    let view_id = view_id();
    let text_id = projection_id("text");
    let image_id = projection_id("image");
    let image_resource_id = ResourceId("generated-image-1".to_string());
    let text_provenance = provenance(
        &view_id,
        10,
        &text_id,
        None,
        Some(10..30),
        None,
        Some(12..30),
    );
    let image_provenance = provenance(
        &view_id,
        20,
        &image_id,
        Some(image_resource_id.clone()),
        Some(20..21),
        Some(0..4096),
        Some(20..21),
    );
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![
                view_record(
                    &view_id,
                    20,
                    "image-record",
                    image_id.clone(),
                    TranscriptNarrativeKind::AssistantGeneratedMedia,
                ),
                view_record(
                    &view_id,
                    10,
                    "text-record",
                    text_id.clone(),
                    TranscriptNarrativeKind::AssistantFinalAnswer,
                ),
            ],
        )
        .insert_projection_record(resource_projection(
            image_id.clone(),
            image_resource_id.clone(),
            ResourceKind::GeneratedImage,
            Some("generated image"),
            image_provenance.clone(),
        ))
        .insert_projection_record(text_projection(
            text_id.clone(),
            "assistant text chunk",
            text_provenance.clone(),
        ));

    let mut core = ResidentTranscriptCore::empty();
    admit_start_page(&mut core, &mut provider, &view_id);

    let projection_request = request_resident_projections(&mut core);
    match &projection_request.kind {
        TranscriptProviderRequestKind::ReadProjectionRecords(request) => {
            assert_eq!(request.view_id, view_id);
            assert_eq!(request.projection_ids, vec![text_id.clone(), image_id]);
            assert_eq!(request.observed_revision, Some(REVISION));
        }
        other => panic!("expected projection records request, got {other:?}"),
    }
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 2,
            rejected_count: 0
        }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.projection_record_count, 2);
    assert_eq!(snapshot.resident.projection_rejection_count, 0);
    assert_eq!(snapshot.presentation.record_count(), 2);
    assert!(matches!(
        snapshot.presentation.state,
        ResidentTranscriptSnapshotState::ProviderBacked { .. }
    ));

    let text_record = &snapshot.presentation.records[0];
    assert_eq!(
        text_record.id,
        ResidentPresentationRecordId("view:text-record:projection:projection-text".to_string())
    );
    assert_eq!(text_record.estimated_bytes, "assistant text chunk".len());
    assert_eq!(
        text_record.kind,
        ResidentPresentationRecordKind::TextChunk {
            narrative_kind: TranscriptNarrativeKind::AssistantFinalAnswer,
            text: "assistant text chunk".to_string()
        }
    );
    assert_eq!(
        text_record.provenance.source,
        ResidentRecordSource::Syndic(text_provenance.clone())
    );
    assert_eq!(text_record.provenance.projection_id, Some(text_id));
    assert_eq!(text_record.provenance.projection_revision, Some(REVISION));
    assert_eq!(
        text_record.provenance.presentation_revision,
        snapshot.presentation.presentation_revision
    );
    assert_eq!(text_record.provenance.copy_source_range, Some(12..30));

    assert_eq!(
        snapshot.presentation.records[1].kind,
        ResidentPresentationRecordKind::ResourceReference {
            resource_id: image_resource_id,
            resource_kind: ResourceKind::GeneratedImage,
            label: Some("generated image".to_string())
        }
    );
    assert_eq!(
        snapshot.presentation.records[1].provenance.source,
        ResidentRecordSource::Syndic(image_provenance)
    );
}

#[test]
fn presentation_order_follows_resident_view_not_projection_response_order() {
    let view_id = view_id();
    let first_id = projection_id("first");
    let second_id = projection_id("second");
    let third_id = projection_id("third");
    let first_projection = text_projection(
        first_id.clone(),
        "first",
        provenance(
            &view_id,
            10,
            &first_id,
            None,
            Some(10..15),
            None,
            Some(10..15),
        ),
    );
    let second_projection = text_projection(
        second_id.clone(),
        "second",
        provenance(
            &view_id,
            20,
            &second_id,
            None,
            Some(20..26),
            None,
            Some(20..26),
        ),
    );
    let third_projection = text_projection(
        third_id.clone(),
        "third",
        provenance(
            &view_id,
            30,
            &third_id,
            None,
            Some(30..35),
            None,
            Some(30..35),
        ),
    );
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION).insert_view_records(
        view_id.clone(),
        vec![
            view_record(
                &view_id,
                30,
                "third-record",
                third_id.clone(),
                TranscriptNarrativeKind::AssistantCommentary,
            ),
            view_record(
                &view_id,
                10,
                "first-record",
                first_id.clone(),
                TranscriptNarrativeKind::AssistantCommentary,
            ),
            view_record(
                &view_id,
                20,
                "second-record",
                second_id.clone(),
                TranscriptNarrativeKind::AssistantCommentary,
            ),
        ],
    );

    let mut core = ResidentTranscriptCore::empty();
    admit_start_page(&mut core, &mut provider, &view_id);
    let request = request_resident_projections(&mut core);
    let request_id = request.id;

    let response = TranscriptProviderResponse {
        request_id,
        kind: TranscriptProviderResponseKind::ProjectionRecords(ProjectionRecordSet {
            view_id,
            revision: REVISION,
            records: vec![third_projection, first_projection, second_projection],
            rejections: Vec::new(),
        }),
    };
    assert_eq!(
        core.handle_provider_response(response),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 3,
            rejected_count: 0
        }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(
        presentation_texts(&snapshot),
        vec!["first", "second", "third"]
    );
}

#[test]
fn rejected_projection_records_are_itemized_and_not_presentation_content() {
    let view_id = view_id();
    let present_id = projection_id("present");
    let missing_id = projection_id("missing");
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
                    "present-record",
                    present_id.clone(),
                    TranscriptNarrativeKind::AssistantCommentary,
                ),
                view_record(
                    &view_id,
                    20,
                    "missing-record",
                    missing_id.clone(),
                    TranscriptNarrativeKind::AssistantCommentary,
                ),
                view_record(
                    &view_id,
                    30,
                    "rejected-record",
                    rejected_id.clone(),
                    TranscriptNarrativeKind::AssistantCommentary,
                ),
            ],
        )
        .insert_projection_record(text_projection(
            present_id.clone(),
            "present projection",
            provenance(
                &view_id,
                10,
                &present_id,
                None,
                Some(10..28),
                None,
                Some(10..28),
            ),
        ))
        .reject_projection_record_with_message(
            rejected_id.clone(),
            TranscriptProviderRejectionReason::BudgetExceeded,
            "projection budget exceeded",
        );

    let mut core = ResidentTranscriptCore::empty();
    admit_start_page(&mut core, &mut provider, &view_id);
    let request = request_resident_projections(&mut core);
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 1,
            rejected_count: 2
        }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.projection_record_count, 1);
    assert_eq!(snapshot.resident.projection_rejection_count, 2);
    assert_eq!(snapshot.resident.fallback_record_count, 1);
    assert_eq!(snapshot.resident.budget_rejection_count, 1);
    assert_eq!(presentation_texts(&snapshot), vec!["present projection"]);
    let fallback = snapshot
        .presentation
        .records
        .iter()
        .find(|record| {
            matches!(
                record.kind,
                ResidentPresentationRecordKind::LocalUiFallback { .. }
            )
        })
        .expect("budget-rejected projection should create a local fallback");
    assert_eq!(
        fallback.kind,
        ResidentPresentationRecordKind::LocalUiFallback {
            reason: LocalPresentationReason::BudgetRejected,
            target: ResidentFallbackTarget::ProjectionRecord(rejected_id.clone())
        }
    );
    match &fallback.provenance.source {
        ResidentRecordSource::LocalUiForSyndic(source) => {
            assert_eq!(source.position, Some(TranscriptViewPosition(30)));
            assert_eq!(source.projection_id, Some(rejected_id.clone()));
        }
        other => panic!("fallback should be local UI tied to Syndic provenance, got {other:?}"),
    }
    assert_eq!(
        snapshot.resident.projection_rejections[0].target,
        TranscriptProviderTarget::ProjectionRecord(missing_id)
    );
    assert_eq!(
        snapshot.resident.projection_rejections[0].reason,
        TranscriptProviderRejectionReason::MissingProjectionRecord
    );
    assert_eq!(
        snapshot.resident.projection_rejections[1].target,
        TranscriptProviderTarget::ProjectionRecord(rejected_id)
    );
    assert_eq!(
        snapshot.resident.projection_rejections[1]
            .message
            .as_deref(),
        Some("projection budget exceeded")
    );
}

#[test]
fn markdown_like_projection_text_is_not_parsed_by_beryl_presentation() {
    let view_id = view_id();
    let markdown_id = projection_id("markdown-like");
    let markdown_like_text =
        "Paragraph\n\n```rust\nfn main() {}\n```\n\n| column |\n| --- |\n| value |";
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(
                &view_id,
                10,
                "markdown-record",
                markdown_id.clone(),
                TranscriptNarrativeKind::AssistantFinalAnswer,
            )],
        )
        .insert_projection_record(text_projection(
            markdown_id,
            markdown_like_text,
            provenance(
                &view_id,
                10,
                &projection_id("markdown-like"),
                None,
                Some(10..75),
                None,
                Some(10..75),
            ),
        ));

    let mut core = ResidentTranscriptCore::empty();
    admit_start_page(&mut core, &mut provider, &view_id);
    let request = request_resident_projections(&mut core);
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 1,
            rejected_count: 0
        }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.presentation.record_count(), 1);
    match &snapshot.presentation.records[0].kind {
        ResidentPresentationRecordKind::TextChunk { text, .. } => {
            assert_eq!(text, markdown_like_text);
        }
        other => panic!("markdown-like text should stay a text chunk, got {other:?}"),
    }
    assert!(!snapshot.presentation.records.iter().any(|record| matches!(
        record.kind,
        ResidentPresentationRecordKind::ResourceReference { .. }
    )));
}
