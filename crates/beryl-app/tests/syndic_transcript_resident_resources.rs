use std::ops::Range;

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(41);

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
    projection_id: &ProjectionRecordId,
    resource_id: &ResourceId,
    resource_range: Option<Range<u64>>,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(10)),
        turn_id: Some(SyndicTurnId("turn-resource".to_string())),
        item_id: Some(SyndicItemId("item-resource".to_string())),
        projection_id: Some(projection_id.clone()),
        resource_id: Some(resource_id.clone()),
        source_range: Some(10..11),
        resource_range,
        copy_source_range: Some(10..11),
    }
}

fn view_record(
    view_id: &TranscriptViewId,
    projection_id: ProjectionRecordId,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId("resource-record".to_string()),
        position: TranscriptViewPosition(10),
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantGeneratedMedia,
        provenance: SyndicSourceProvenance {
            view_id: view_id.clone(),
            position: Some(TranscriptViewPosition(10)),
            turn_id: Some(SyndicTurnId("turn-resource".to_string())),
            item_id: Some(SyndicItemId("item-resource".to_string())),
            projection_id: Some(projection_id),
            resource_id: None,
            source_range: Some(10..11),
            resource_range: None,
            copy_source_range: Some(10..11),
        },
    }
}

fn resource_projection(
    projection_id: ProjectionRecordId,
    resource_id: ResourceId,
    kind: ResourceKind,
) -> ProjectionRecord {
    let view_id = view_id();
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::ResourceReference,
        payload: ProjectionPayload::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind: kind,
            label: Some("resident resource".to_string()),
        },
        provenance: provenance(&view_id, &projection_id, &resource_id, Some(0..16)),
    }
}

fn metadata(resource_id: ResourceId, kind: ResourceKind) -> ResourceMetadata {
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

fn seed_resource_presentation(
    policy: ResidentTranscriptPolicy,
    kind: ResourceKind,
    bytes: Vec<u8>,
) -> (
    ResidentTranscriptCore,
    InMemorySyndicTranscriptProvider,
    ResourceId,
) {
    let view_id = view_id();
    let projection_id = projection_id("resource");
    let resource_id = resource_id("primary");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(&view_id, projection_id.clone())],
        )
        .insert_projection_record(resource_projection(
            projection_id,
            resource_id.clone(),
            kind.clone(),
        ))
        .insert_resource(metadata(resource_id.clone(), kind), bytes);

    let mut core = ResidentTranscriptCore::new(policy);
    let page_request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, page_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 1 }
    );

    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("resident view should have a missing resource projection");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 1,
            rejected_count: 0
        }
    );

    (core, provider, resource_id)
}

fn seed_rejected_resource_presentation(
    reason: TranscriptProviderRejectionReason,
) -> (
    ResidentTranscriptCore,
    InMemorySyndicTranscriptProvider,
    ResourceId,
) {
    let (core, mut provider, resource_id) = seed_resource_presentation(
        ResidentTranscriptPolicy::default(),
        ResourceKind::GeneratedImage,
        Vec::new(),
    );
    provider.reject_resource_with_message(resource_id.clone(), reason, "resource rejected");
    (core, provider, resource_id)
}

fn admit_metadata(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
) {
    let request = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .expect("resident presentation should reference missing resource metadata");
    assert_eq!(
        handle_provider_request(core, provider, request),
        ResidentProviderResponseEffect::ResourceMetadataAdmitted { admitted_count: 1 }
    );
}

fn has_fallback(snapshot: &ResidentCoreSnapshot) -> bool {
    snapshot.presentation.records.iter().any(|record| {
        matches!(
            record.kind,
            ResidentPresentationRecordKind::LocalUiFallback { .. }
        )
    })
}

fn first_fallback(snapshot: &ResidentCoreSnapshot) -> &ResidentPresentationRecord {
    snapshot
        .presentation
        .records
        .iter()
        .find(|record| {
            matches!(
                record.kind,
                ResidentPresentationRecordKind::LocalUiFallback { .. }
            )
        })
        .expect("snapshot should contain a local fallback record")
}

#[test]
fn resource_metadata_demand_is_requested_and_admitted_without_bytes() {
    let bytes = b"resident image bytes".to_vec();
    let (mut core, mut provider, resource_id) = seed_resource_presentation(
        ResidentTranscriptPolicy::default(),
        ResourceKind::GeneratedImage,
        bytes.clone(),
    );

    let before_metadata = core.core_snapshot();
    assert_eq!(before_metadata.resident.resource_metadata_count, 0);
    assert_eq!(before_metadata.resident.resource_slice_count, 0);
    assert_eq!(before_metadata.resident.resource_bytes, 0);

    let request = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .expect("resource reference should demand metadata");
    match &request.kind {
        TranscriptProviderRequestKind::ReadResourceMetadata(metadata_request) => {
            assert_eq!(metadata_request.resource_id, resource_id);
            assert_eq!(metadata_request.observed_revision, Some(REVISION));
        }
        other => panic!("expected resource metadata request, got {other:?}"),
    }

    assert_eq!(
        handle_provider_request(&mut core, &mut provider, request),
        ResidentProviderResponseEffect::ResourceMetadataAdmitted { admitted_count: 1 }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.resource_metadata_count, 1);
    assert_eq!(snapshot.resident.resource_slice_count, 0);
    assert_eq!(snapshot.resident.resource_bytes, 0);
    assert_eq!(
        snapshot.resident.resource_metadata[0].byte_len,
        bytes.len() as u64
    );
    assert_eq!(snapshot.resident.resource_metadata[0].revision, REVISION);
    assert_eq!(snapshot.presentation.resources.metadata.len(), 1);
    assert_eq!(
        snapshot.presentation.resources.metadata[0],
        snapshot.resident.resource_metadata[0]
    );
    assert!(snapshot.presentation.resources.slices.is_empty());
    assert!(matches!(
        snapshot.presentation.records[0].kind,
        ResidentPresentationRecordKind::ResourceReference { .. }
    ));
    assert!(!has_fallback(&snapshot));
}

#[test]
fn oversized_image_metadata_creates_budget_fallback_with_syndic_provenance() {
    let policy = ResidentTranscriptPolicy {
        max_resource_bytes: 4,
        ..ResidentTranscriptPolicy::default()
    };
    let (mut core, mut provider, resource_id) = seed_resource_presentation(
        policy,
        ResourceKind::GeneratedImage,
        b"large image".to_vec(),
    );

    admit_metadata(&mut core, &mut provider);

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.resource_metadata_count, 1);
    assert_eq!(snapshot.resident.resource_slice_count, 0);
    assert_eq!(snapshot.resident.fallback_record_count, 1);
    assert_eq!(snapshot.resident.budget_rejection_count, 1);
    assert_eq!(
        snapshot.resident.fallback_records[0].rejected_bytes,
        Some(11)
    );
    assert_eq!(snapshot.resident.fallback_records[0].limit_bytes, Some(4));
    assert_eq!(
        snapshot.resident.fallback_records[0].target,
        ResidentFallbackTarget::Resource(resource_id.clone())
    );

    let fallback = first_fallback(&snapshot);
    assert_eq!(
        fallback.kind,
        ResidentPresentationRecordKind::LocalUiFallback {
            reason: LocalPresentationReason::BudgetRejected,
            target: ResidentFallbackTarget::Resource(resource_id.clone())
        }
    );
    match &fallback.provenance.source {
        ResidentRecordSource::LocalUiForSyndic(source) => {
            assert_eq!(source.resource_id, Some(resource_id.clone()));
            assert_eq!(
                source.projection_id,
                Some(ProjectionRecordId("projection-resource".to_string()))
            );
        }
        other => panic!("fallback should be local UI tied to Syndic provenance, got {other:?}"),
    }
    assert!(
        core.request_resource_range(resource_id, 0..4, ProviderRequestReason::ResourceRange)
            .is_none()
    );

    assert_eq!(snapshot.resident.resource_slice_bytes, 0);
    assert!(snapshot.resident.projection_bytes > 0);
    assert!(snapshot.resident.presentation_bytes > 0);
    assert!(snapshot.resident.geometry_bytes > 0);
}

#[test]
fn resource_range_demand_is_bounded_and_admitted() {
    let policy = ResidentTranscriptPolicy {
        max_resource_slice_bytes: 5,
        max_resource_bytes: 16,
        ..ResidentTranscriptPolicy::default()
    };
    let (mut core, mut provider, resource_id) =
        seed_resource_presentation(policy, ResourceKind::Code, b"0123456789abcdef".to_vec());
    admit_metadata(&mut core, &mut provider);

    core.push_demand_fact(DemandFact::new(
        core.presentation_snapshot().presentation_revision,
        DemandFactKind::ResourceRange {
            resource_id: resource_id.clone(),
            range: 0..16,
        },
    ));
    let request = core
        .request_resource_range(
            resource_id.clone(),
            0..16,
            ProviderRequestReason::ResourceRange,
        )
        .expect("resource range demand should reserve a bounded request");

    match &request.kind {
        TranscriptProviderRequestKind::ReadResourceRange(range_request) => {
            assert_eq!(range_request.resource_id, resource_id);
            assert_eq!(range_request.range, 0..5);
            assert_eq!(range_request.observed_revision, Some(REVISION));
        }
        other => panic!("expected resource range request, got {other:?}"),
    }

    assert_eq!(
        handle_provider_request(&mut core, &mut provider, request),
        ResidentProviderResponseEffect::ResourceRangeAdmitted {
            admitted_count: 1,
            byte_count: 5
        }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.demand_facts.pending_count, 1);
    assert_eq!(snapshot.resident.resource_slice_count, 1);
    assert_eq!(snapshot.resident.resource_slices[0].range, 0..5);
    assert_eq!(
        snapshot.resident.resource_slices[0].bytes,
        b"01234".to_vec()
    );
    assert_eq!(snapshot.presentation.resources.metadata.len(), 1);
    assert_eq!(snapshot.presentation.resources.slices.len(), 1);
    assert_eq!(
        snapshot.presentation.resources.slices[0],
        snapshot.resident.resource_slices[0]
    );
    assert!(!snapshot.resident.resource_slices[0].complete);
    assert_eq!(snapshot.resident.resource_bytes, 5);
    assert!(snapshot.resident.estimated_resident_bytes >= 5);
    assert!(!has_fallback(&snapshot));
}

#[test]
fn zero_resource_slice_budget_creates_rejected_demand_fallback() {
    let policy = ResidentTranscriptPolicy {
        max_resource_slice_bytes: 0,
        ..ResidentTranscriptPolicy::default()
    };
    let (mut core, mut provider, resource_id) =
        seed_resource_presentation(policy, ResourceKind::Code, b"0123456789".to_vec());
    admit_metadata(&mut core, &mut provider);

    assert!(
        core.request_resource_range(
            resource_id.clone(),
            0..4,
            ProviderRequestReason::ResourceRange
        )
        .is_none()
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.provider_requests.pending_count, 0);
    assert_eq!(snapshot.provider_requests.rejected_result_count, 0);
    assert_eq!(snapshot.resident.resource_slice_count, 0);
    assert_eq!(snapshot.resident.fallback_record_count, 1);
    assert_eq!(snapshot.resident.budget_rejection_count, 1);
    assert_eq!(snapshot.resident.fallback_records[0].limit_bytes, Some(0));
    assert_eq!(
        first_fallback(&snapshot).kind,
        ResidentPresentationRecordKind::LocalUiFallback {
            reason: LocalPresentationReason::BudgetRejected,
            target: ResidentFallbackTarget::ResourceRange {
                resource_id,
                range: 0..4,
            }
        }
    );
}

#[test]
fn media_preview_pin_tracks_referenced_resource_without_loading_bytes() {
    let (mut core, _provider, resource_id) = seed_resource_presentation(
        ResidentTranscriptPolicy::default(),
        ResourceKind::Image,
        b"image bytes".to_vec(),
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
        DemandFactKind::MediaPreviewPin {
            resource_id: resource_id.clone(),
        },
    ));

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.active_resource_pins, vec![resource_id]);
    assert_eq!(snapshot.resident.active_pin_count, 1);
    assert_eq!(snapshot.resident.pin_bytes, 64);
    assert_eq!(snapshot.resident.resource_bytes, 0);
    assert_eq!(snapshot.demand_facts.pending_count, 2);
}

#[test]
fn stale_resource_responses_create_local_fallbacks_without_resident_content() {
    let (mut core, mut provider, resource_id) = seed_resource_presentation(
        ResidentTranscriptPolicy::default(),
        ResourceKind::Attachment,
        b"attachment bytes".to_vec(),
    );
    provider.advance_revision();

    let metadata_request = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .expect("resource reference should demand stale metadata");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, metadata_request),
        ResidentProviderResponseEffect::Stale
    );

    let range_request = core
        .request_resource_range(
            resource_id.clone(),
            0..4,
            ProviderRequestReason::ResourceRange,
        )
        .expect("resource reference should demand stale range");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, range_request),
        ResidentProviderResponseEffect::Stale
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.provider_requests.stale_result_count, 2);
    assert_eq!(snapshot.provider_requests.rejected_result_count, 0);
    assert_eq!(snapshot.resident.resource_metadata_count, 0);
    assert_eq!(snapshot.resident.resource_slice_count, 0);
    assert_eq!(snapshot.resident.resource_rejection_count, 0);
    assert_eq!(snapshot.resident.fallback_record_count, 2);
    assert_eq!(snapshot.resident.budget_rejection_count, 0);
    assert_eq!(snapshot.resident.resource_bytes, 0);
    let fallback = first_fallback(&snapshot);
    match &fallback.kind {
        ResidentPresentationRecordKind::LocalUiFallback { reason, target } => {
            assert_eq!(*reason, LocalPresentationReason::PendingCoherentData);
            assert!(matches!(
                target,
                ResidentFallbackTarget::Resource(_) | ResidentFallbackTarget::ResourceRange { .. }
            ));
        }
        other => panic!("expected pending local fallback, got {other:?}"),
    }
    assert!(matches!(
        &fallback.provenance.source,
        ResidentRecordSource::LocalUiForSyndic(_)
    ));
}

#[test]
fn rejected_resource_metadata_creates_terminal_local_fallback() {
    let (mut core, mut provider, resource_id) =
        seed_rejected_resource_presentation(TranscriptProviderRejectionReason::BudgetExceeded);

    let metadata_request = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .expect("resource reference should demand metadata");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, metadata_request),
        ResidentProviderResponseEffect::Rejected
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.provider_requests.rejected_result_count, 1);
    assert_eq!(snapshot.resident.resource_metadata_count, 0);
    assert_eq!(snapshot.resident.resource_slice_count, 0);
    assert_eq!(snapshot.resident.resource_rejection_count, 1);
    assert_eq!(snapshot.resident.fallback_record_count, 1);
    assert_eq!(snapshot.resident.budget_rejection_count, 1);
    assert_eq!(
        snapshot.resident.resource_rejections[0].target,
        TranscriptProviderTarget::Resource(resource_id.clone())
    );
    let fallback = first_fallback(&snapshot);
    assert_eq!(
        fallback.kind,
        ResidentPresentationRecordKind::LocalUiFallback {
            reason: LocalPresentationReason::BudgetRejected,
            target: ResidentFallbackTarget::Resource(resource_id.clone())
        }
    );
    assert!(matches!(
        &fallback.provenance.source,
        ResidentRecordSource::LocalUiForSyndic(_)
    ));
    assert!(
        core.request_resource_range(resource_id, 0..4, ProviderRequestReason::ResourceRange)
            .is_none()
    );
}

#[test]
fn rejected_resource_range_creates_local_fallback_with_rejection_diagnostic() {
    let (mut core, mut provider, resource_id) = seed_resource_presentation(
        ResidentTranscriptPolicy::default(),
        ResourceKind::Attachment,
        b"attachment bytes".to_vec(),
    );
    admit_metadata(&mut core, &mut provider);
    provider.reject_resource_with_message(
        resource_id.clone(),
        TranscriptProviderRejectionReason::UnsupportedResourceKind,
        "unsupported attachment",
    );

    let range_request = core
        .request_resource_range(
            resource_id.clone(),
            0..4,
            ProviderRequestReason::ResourceRange,
        )
        .expect("resident metadata should allow a range demand before rejection");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, range_request),
        ResidentProviderResponseEffect::Rejected
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.provider_requests.rejected_result_count, 1);
    assert_eq!(snapshot.resident.resource_metadata_count, 1);
    assert_eq!(snapshot.resident.resource_slice_count, 0);
    assert_eq!(snapshot.resident.resource_rejection_count, 1);
    assert_eq!(snapshot.resident.fallback_record_count, 1);
    assert_eq!(
        snapshot.resident.fallback_records[0].provider_rejection_reason,
        Some(TranscriptProviderRejectionReason::UnsupportedResourceKind)
    );
    assert_eq!(
        snapshot.resident.resource_rejections[0].target,
        TranscriptProviderTarget::ResourceRange {
            resource_id: resource_id.clone(),
            range: 0..4,
        }
    );
    let fallback = first_fallback(&snapshot);
    assert_eq!(
        fallback.kind,
        ResidentPresentationRecordKind::LocalUiFallback {
            reason: LocalPresentationReason::Unsupported,
            target: ResidentFallbackTarget::ResourceRange {
                resource_id,
                range: 0..4,
            }
        }
    );
    assert!(matches!(
        &fallback.provenance.source,
        ResidentRecordSource::LocalUiForSyndic(_)
    ));
}

#[test]
fn resource_slice_retention_stays_under_policy_byte_budget() {
    let policy = ResidentTranscriptPolicy {
        max_resource_slice_bytes: 6,
        max_resource_bytes: 10,
        ..ResidentTranscriptPolicy::default()
    };
    let (mut core, mut provider, resource_id) =
        seed_resource_presentation(policy, ResourceKind::Table, b"abcdefghijklmnop".to_vec());
    admit_metadata(&mut core, &mut provider);

    let first_request = core
        .request_resource_range(
            resource_id.clone(),
            0..6,
            ProviderRequestReason::ResourceRange,
        )
        .expect("first table slice should be demandable");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, first_request),
        ResidentProviderResponseEffect::ResourceRangeAdmitted {
            admitted_count: 1,
            byte_count: 6
        }
    );

    let second_request = core
        .request_resource_range(resource_id, 6..12, ProviderRequestReason::ResourceRange)
        .expect("second table slice should be demandable");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, second_request),
        ResidentProviderResponseEffect::ResourceRangeAdmitted {
            admitted_count: 1,
            byte_count: 6
        }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.resource_bytes, 6);
    assert!(snapshot.resident.resource_bytes <= policy.max_resource_bytes);
    assert_eq!(snapshot.resident.resource_slice_count, 1);
    assert_eq!(snapshot.resident.resource_slices[0].range, 6..12);
    assert_eq!(
        snapshot.resident.resource_slices[0].bytes,
        b"ghijkl".to_vec()
    );
}

#[test]
fn resident_budget_accounting_reports_core_categories() {
    let policy = ResidentTranscriptPolicy {
        max_resource_slice_bytes: 4,
        max_resource_bytes: 16,
        ..ResidentTranscriptPolicy::default()
    };
    let (mut core, mut provider, resource_id) =
        seed_resource_presentation(policy, ResourceKind::Table, b"abcdefghijkl".to_vec());
    admit_metadata(&mut core, &mut provider);

    let range_request = core
        .request_resource_range(
            resource_id.clone(),
            0..8,
            ProviderRequestReason::ResourceRange,
        )
        .expect("bounded table slice should be demandable");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, range_request),
        ResidentProviderResponseEffect::ResourceRangeAdmitted {
            admitted_count: 1,
            byte_count: 4
        }
    );
    core.push_demand_fact(DemandFact::new(
        core.presentation_snapshot().presentation_revision,
        DemandFactKind::MediaPreviewPin {
            resource_id: resource_id.clone(),
        },
    ));

    let snapshot = core.core_snapshot();
    assert!(snapshot.resident.projection_bytes > 0);
    assert!(snapshot.resident.presentation_bytes > 0);
    assert_eq!(snapshot.resident.resource_slice_bytes, 4);
    assert_eq!(snapshot.resident.resource_bytes, 4);
    assert_eq!(snapshot.resident.decoded_or_uploaded_media_bytes, 0);
    assert!(snapshot.resident.geometry_bytes > 0);
    assert_eq!(snapshot.resident.pin_bytes, 64);
    assert_eq!(snapshot.resident.active_pin_count, 1);
    assert!(snapshot.resident.estimated_resident_bytes >= 72);

    assert_eq!(snapshot.resident.resource_slice_bytes, 4);
    assert_eq!(snapshot.resident.pin_bytes, 64);
    assert_eq!(snapshot.resident.fallback_record_count, 0);
    assert_eq!(snapshot.resident.budget_rejection_count, 0);
}
