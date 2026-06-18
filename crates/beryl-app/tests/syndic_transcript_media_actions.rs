use std::{ops::Range, path::PathBuf};

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(89);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("media-action-view".to_string())
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
        turn_id: Some(SyndicTurnId("turn-media".to_string())),
        item_id: Some(SyndicItemId("item-media".to_string())),
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
    view_record_named(view_id, "media-record", 10, projection_id)
}

fn view_record_named(
    view_id: &TranscriptViewId,
    name: &str,
    position: u64,
    projection_id: ProjectionRecordId,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(name.to_string()),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantGeneratedMedia,
        provenance: SyndicSourceProvenance {
            view_id: view_id.clone(),
            position: Some(TranscriptViewPosition(position)),
            turn_id: Some(SyndicTurnId(format!("turn-media-{position}"))),
            item_id: Some(SyndicItemId(format!("item-media-{position}"))),
            projection_id: Some(projection_id),
            resource_id: None,
            source_range: Some(position..position + 1),
            resource_range: None,
            copy_source_range: Some(position..position + 1),
        },
    }
}

fn resource_projection_with_provenance(
    projection_id: ProjectionRecordId,
    resource_id: ResourceId,
    kind: ResourceKind,
    provenance: SyndicSourceProvenance,
) -> ProjectionRecord {
    ProjectionRecord {
        id: projection_id,
        revision: REVISION,
        kind: ProjectionRecordKind::ResourceReference,
        payload: ProjectionPayload::ResourceReference {
            resource_id,
            resource_kind: kind,
            label: Some("resident media".to_string()),
        },
        provenance,
    }
}

fn metadata(resource_id: ResourceId, kind: ResourceKind) -> ResourceMetadata {
    ResourceMetadata {
        resource_id,
        revision: REVISION,
        kind,
        media_type: Some("image/png".to_string()),
        byte_len: 0,
        digest: Some("sha256:media-action".to_string()),
        line_count: None,
        row_count: None,
        column_count: None,
        preview_range: Some(0..16),
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

fn seed_media_presentation(
    kind: ResourceKind,
    bytes: Vec<u8>,
    resource_range: Option<Range<u64>>,
) -> (
    ResidentTranscriptCore,
    InMemorySyndicTranscriptProvider,
    ResourceId,
) {
    let view_id = view_id();
    let projection_id = projection_id("media");
    let resource_id = resource_id("primary");
    let projection_provenance = provenance(&view_id, &projection_id, &resource_id, resource_range);
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(&view_id, projection_id.clone())],
        )
        .insert_projection_record(resource_projection_with_provenance(
            projection_id,
            resource_id.clone(),
            kind.clone(),
            projection_provenance,
        ))
        .insert_resource(metadata(resource_id.clone(), kind), bytes);

    let mut core = ResidentTranscriptCore::empty();
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
        .expect("resident view should need the media projection");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 1,
            rejected_count: 0
        }
    );

    (core, provider, resource_id)
}

fn seed_two_media_presentation() -> ResidentTranscriptCore {
    let view_id = view_id();
    let first_projection_id = projection_id("first-media");
    let second_projection_id = projection_id("second-media");
    let first_resource_id = resource_id("first");
    let second_resource_id = resource_id("second");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![
                view_record_named(
                    &view_id,
                    "first-media-record",
                    10,
                    first_projection_id.clone(),
                ),
                view_record_named(
                    &view_id,
                    "second-media-record",
                    20,
                    second_projection_id.clone(),
                ),
            ],
        )
        .insert_projection_record(resource_projection_with_provenance(
            first_projection_id.clone(),
            first_resource_id.clone(),
            ResourceKind::GeneratedImage,
            provenance(
                &view_id,
                &first_projection_id,
                &first_resource_id,
                Some(0..16),
            ),
        ))
        .insert_projection_record(resource_projection_with_provenance(
            second_projection_id.clone(),
            second_resource_id.clone(),
            ResourceKind::GeneratedImage,
            provenance(
                &view_id,
                &second_projection_id,
                &second_resource_id,
                Some(0..16),
            ),
        ))
        .insert_resource(
            metadata(first_resource_id.clone(), ResourceKind::GeneratedImage),
            b"0123456789abcdef".to_vec(),
        )
        .insert_resource(
            metadata(second_resource_id, ResourceKind::GeneratedImage),
            b"fedcba9876543210".to_vec(),
        );

    let mut core = ResidentTranscriptCore::empty();
    let page_request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, page_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 2 }
    );
    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("resident view should need media projections");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 2,
            rejected_count: 0
        }
    );
    while let Some(metadata_request) = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
    {
        assert_eq!(
            handle_provider_request(&mut core, &mut provider, metadata_request),
            ResidentProviderResponseEffect::ResourceMetadataAdmitted { admitted_count: 1 }
        );
    }

    core
}

fn admit_metadata(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
) {
    let request = core
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .expect("resident media reference should need metadata");
    assert_eq!(
        handle_provider_request(core, provider, request),
        ResidentProviderResponseEffect::ResourceMetadataAdmitted { admitted_count: 1 }
    );
}

fn admit_range(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
    resource_id: ResourceId,
    range: Range<u64>,
) {
    let request = core
        .request_resource_range(resource_id, range, ProviderRequestReason::ResourceRange)
        .expect("resident metadata should allow range demand");
    assert_eq!(
        handle_provider_request(core, provider, request),
        ResidentProviderResponseEffect::ResourceRangeAdmitted {
            admitted_count: 1,
            byte_count: 16
        }
    );
}

fn realize_frame(core: &mut ResidentTranscriptCore) -> RealizedFrameWindow {
    let snapshot = core.presentation_snapshot();
    let mut controller = RealizedFrameScrollController::new();
    let window = controller.realize(
        &snapshot,
        RealizedFrameRequest {
            viewport_height_px: 120.0,
            overscan_height_px: 0.0,
            default_record_height_px: 20.0,
            manual_delta_px: 0.0,
            observed_presentation_revision: None,
        },
    );
    for fact in &window.demand_facts {
        core.push_demand_fact(fact.clone());
    }
    window
}

fn frame_record_from_snapshot(
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

fn frame_window_from_records(
    presentation_revision: u64,
    records: Vec<RealizedFrameRecord>,
) -> RealizedFrameWindow {
    let end = records
        .iter()
        .map(|record| record.index)
        .max()
        .map(|index| index + 1)
        .unwrap_or(0);
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

fn media_command_for_frame_record(
    presentation_revision: u64,
    record: &RealizedFrameRecord,
) -> ResidentMediaActionCommand {
    ResidentMediaActionCommand::from_realized_frame_record(presentation_revision, record)
}

fn seed_budget_fallback_presentation() -> ResidentTranscriptCore {
    let view_id = view_id();
    let projection_id = projection_id("rejected-media");
    let resource_id = resource_id("rejected-media");
    let projection_provenance = provenance(&view_id, &projection_id, &resource_id, Some(0..16));
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(&view_id, projection_id.clone())],
        )
        .insert_projection_record(resource_projection_with_provenance(
            projection_id.clone(),
            resource_id,
            ResourceKind::GeneratedImage,
            projection_provenance,
        ))
        .reject_projection_record(
            projection_id,
            TranscriptProviderRejectionReason::BudgetExceeded,
        );

    let mut core = ResidentTranscriptCore::empty();
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
        .expect("rejected projection should still be requested");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 0,
            rejected_count: 1
        }
    );
    core
}

#[test]
fn renderer_media_command_uses_only_realized_metadata_backed_record() {
    let (mut core, mut provider, resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    let window = realize_frame(&mut core);
    let snapshot = core.presentation_snapshot();
    let record_id = window.records[0].record_id.clone();

    let command =
        resident_media_action_command_for_realized_record_id(&snapshot, &window, &record_id)
            .expect("realized metadata-backed media should produce renderer command");

    assert_eq!(
        command,
        media_command_for_frame_record(window.presentation_revision, &window.records[0])
    );
    assert_eq!(
        realized_resident_media_action_record_ids(&snapshot, &window),
        vec![record_id.clone()]
    );

    let target = match core.apply_resident_media_action_target(command) {
        ResidentMediaActionOutcome::Targeted(target) => target,
        other => panic!("expected renderer-captured media target, got {other:?}"),
    };
    assert_eq!(target.record.record_id, record_id);
    assert_eq!(target.record.resource_id, resource_id);
    assert_eq!(
        target.record.range_availability,
        ResidentMediaRangeAvailability::Demandable { range: 0..16 }
    );
}

#[test]
fn renderer_media_command_rejects_stale_missing_not_realized_and_unstable_records() {
    let core = seed_two_media_presentation();
    let snapshot = core.presentation_snapshot();
    let first_record = frame_record_from_snapshot(&snapshot, 0, 0.0, 20.0);
    let second_record_id = snapshot.records[1].id.clone();
    let frame =
        frame_window_from_records(snapshot.presentation_revision, vec![first_record.clone()]);

    assert_eq!(
        resident_media_action_command_for_realized_record_id(
            &snapshot,
            &frame_window_from_records(
                snapshot.presentation_revision.saturating_sub(1),
                vec![first_record.clone()],
            ),
            &first_record.record_id,
        ),
        Err(ResidentMediaActionUnavailable::StalePresentationRevision {
            observed: snapshot.presentation_revision.saturating_sub(1),
            current: snapshot.presentation_revision,
        })
    );

    assert_eq!(
        resident_media_action_command_for_realized_record_id(
            &snapshot,
            &frame,
            &ResidentPresentationRecordId("missing-media".to_string()),
        ),
        Err(ResidentMediaActionUnavailable::RecordNotResident {
            record_id: ResidentPresentationRecordId("missing-media".to_string()),
        })
    );
    assert_eq!(
        resident_media_action_command_for_realized_record_id(&snapshot, &frame, &second_record_id),
        Err(ResidentMediaActionUnavailable::RecordNotRealized {
            record_id: second_record_id,
        })
    );

    let unstable_frame = frame_window_from_records(
        snapshot.presentation_revision,
        vec![RealizedFrameRecord {
            height_px: 0.0,
            ..first_record.clone()
        }],
    );
    assert_eq!(
        resident_media_action_command_for_realized_record_id(
            &snapshot,
            &unstable_frame,
            &first_record.record_id,
        ),
        Err(ResidentMediaActionUnavailable::UnstableGeometry {
            record_id: first_record.record_id,
        })
    );
}

#[test]
fn renderer_media_command_rejects_fallback_local_non_media_provenance_and_metadata_gaps() {
    let mut fallback_core = seed_budget_fallback_presentation();
    let fallback_window = realize_frame(&mut fallback_core);
    let fallback_snapshot = fallback_core.presentation_snapshot();
    assert!(matches!(
        resident_media_action_command_for_realized_record_id(
            &fallback_snapshot,
            &fallback_window,
            &fallback_window.records[0].record_id,
        ),
        Err(ResidentMediaActionUnavailable::NonMediaRecord { .. })
    ));
    assert_eq!(
        realized_resident_media_action_record_ids(&fallback_snapshot, &fallback_window),
        Vec::<ResidentPresentationRecordId>::new()
    );

    let local_id = ResidentPresentationRecordId("local-affordance".to_string());
    let local_snapshot = ResidentTranscriptSnapshot {
        activation_revision: 1,
        presentation_revision: 44,
        state: ResidentTranscriptSnapshotState::ProviderBacked {
            label: "local-media-reject".to_string(),
        },
        records: vec![ResidentPresentationRecord {
            id: local_id.clone(),
            kind: ResidentPresentationRecordKind::LocalAffordance,
            provenance: ResidentRecordProvenance {
                source: ResidentRecordSource::LocalUi,
                projection_id: None,
                projection_revision: None,
                presentation_revision: 44,
                copy_source_range: None,
            },
            estimated_bytes: 0,
        }],
        resources: ResidentResourceSnapshot::default(),
        realized_range: None,
        visible_range: None,
    };
    let local_frame = frame_window_from_records(
        local_snapshot.presentation_revision,
        vec![frame_record_from_snapshot(&local_snapshot, 0, 0.0, 20.0)],
    );
    assert_eq!(
        resident_media_action_command_for_realized_record_id(
            &local_snapshot,
            &local_frame,
            &local_id,
        ),
        Err(ResidentMediaActionUnavailable::NonMediaRecord {
            record_id: local_id,
        })
    );

    let (mut code_core, mut code_provider, _resource_id) =
        seed_media_presentation(ResourceKind::Code, b"code bytes".to_vec(), Some(0..10));
    admit_metadata(&mut code_core, &mut code_provider);
    let code_window = realize_frame(&mut code_core);
    let code_snapshot = code_core.presentation_snapshot();
    assert!(matches!(
        resident_media_action_command_for_realized_record_id(
            &code_snapshot,
            &code_window,
            &code_window.records[0].record_id,
        ),
        Err(ResidentMediaActionUnavailable::NonMediaRecord { .. })
    ));

    let (mut missing_range_core, mut missing_range_provider, _resource_id) =
        seed_media_presentation(
            ResourceKind::GeneratedImage,
            b"0123456789abcdef".to_vec(),
            None,
        );
    admit_metadata(&mut missing_range_core, &mut missing_range_provider);
    let missing_range_window = realize_frame(&mut missing_range_core);
    let missing_range_snapshot = missing_range_core.presentation_snapshot();
    assert!(matches!(
        resident_media_action_command_for_realized_record_id(
            &missing_range_snapshot,
            &missing_range_window,
            &missing_range_window.records[0].record_id,
        ),
        Err(ResidentMediaActionUnavailable::MissingStableProvenance { .. })
    ));

    let (mut missing_metadata_core, _provider, resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    let missing_metadata_window = realize_frame(&mut missing_metadata_core);
    let missing_metadata_snapshot = missing_metadata_core.presentation_snapshot();
    assert_eq!(
        resident_media_action_command_for_realized_record_id(
            &missing_metadata_snapshot,
            &missing_metadata_window,
            &missing_metadata_window.records[0].record_id,
        ),
        Err(ResidentMediaActionUnavailable::MissingResourceMetadata {
            record_id: missing_metadata_window.records[0].record_id.clone(),
            resource_id,
        })
    );
}

#[test]
fn renderer_media_frame_loss_tracks_realized_resident_identity() {
    let (mut core, mut provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    let stable_window = realize_frame(&mut core);
    let stable_snapshot = core.presentation_snapshot();
    let command = resident_media_action_command_for_realized_record_id(
        &stable_snapshot,
        &stable_window,
        &stable_window.records[0].record_id,
    )
    .expect("stable realized media command");
    let target = match core.apply_resident_media_action_target(command) {
        ResidentMediaActionOutcome::Targeted(target) => target,
        other => panic!("expected active media target, got {other:?}"),
    };

    assert_eq!(
        resident_media_action_frame_loss(&stable_snapshot, &stable_window, &target),
        None
    );

    let missing_frame = frame_window_from_records(stable_snapshot.presentation_revision, vec![]);
    assert_eq!(
        resident_media_action_frame_loss(&stable_snapshot, &missing_frame, &target),
        Some(ResidentMediaActionUnavailable::RecordNotRealized {
            record_id: stable_window.records[0].record_id.clone(),
        })
    );

    let unstable_frame = frame_window_from_records(
        stable_snapshot.presentation_revision,
        vec![RealizedFrameRecord {
            height_px: f32::NAN,
            ..stable_window.records[0].clone()
        }],
    );
    assert_eq!(
        resident_media_action_frame_loss(&stable_snapshot, &unstable_frame, &target),
        Some(ResidentMediaActionUnavailable::UnstableGeometry {
            record_id: stable_window.records[0].record_id.clone(),
        })
    );

    let stale_frame = frame_window_from_records(
        stable_snapshot.presentation_revision.saturating_sub(1),
        vec![stable_window.records[0].clone()],
    );
    assert_eq!(
        resident_media_action_frame_loss(&stable_snapshot, &stale_frame, &target),
        Some(ResidentMediaActionUnavailable::StalePresentationRevision {
            observed: stable_snapshot.presentation_revision.saturating_sub(1),
            current: stable_snapshot.presentation_revision,
        })
    );
}

#[test]
fn resident_media_target_uses_metadata_and_resident_bytes() {
    let (mut core, mut provider, resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    admit_range(&mut core, &mut provider, resource_id.clone(), 0..16);
    let window = realize_frame(&mut core);

    let outcome = core.apply_resident_media_action_target(media_command_for_frame_record(
        window.presentation_revision,
        &window.records[0],
    ));

    let target = match outcome {
        ResidentMediaActionOutcome::Targeted(target) => target,
        other => panic!("expected resident media target, got {other:?}"),
    };
    assert_eq!(target.record.resource_id, resource_id);
    assert_eq!(target.record.resource_kind, ResourceKind::GeneratedImage);
    assert_eq!(target.record.media_type, Some("image/png".to_string()));
    assert_eq!(target.record.byte_len, 16);
    assert_eq!(target.record.resource_range, 0..16);
    assert_eq!(
        target.record.range_availability,
        ResidentMediaRangeAvailability::Resident {
            requested_range: 0..16,
            resident_range: 0..16,
            complete: true,
        }
    );

    let payload = core
        .resident_media_action_payload()
        .expect("resident media target should produce payload bytes");
    assert_eq!(payload.presentation_revision, window.presentation_revision);
    assert_eq!(payload.range, 0..16);
    assert_eq!(payload.bytes, b"0123456789abcdef".to_vec());
    assert!(payload.complete);
    assert_eq!(payload.record.record_id, target.record.record_id);

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.active_media_pins, target.record_ids());
    assert!(snapshot.resident.active_resource_pins.is_empty());
    assert_eq!(snapshot.resident.active_pin_count, 1);
}

#[test]
fn resident_media_preview_command_uses_resident_payload_bytes() {
    let (mut core, mut provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    admit_range(&mut core, &mut provider, resource_id("primary"), 0..16);
    let window = realize_frame(&mut core);
    let target = match core.apply_resident_media_action_target(media_command_for_frame_record(
        window.presentation_revision,
        &window.records[0],
    )) {
        ResidentMediaActionOutcome::Targeted(target) => target,
        other => panic!("expected resident media target, got {other:?}"),
    };

    let command_target = ResidentMediaPreviewCommandTarget::from_resident_payload(
        core.resident_media_action_payload(),
    );

    match command_target {
        ResidentMediaPreviewCommandTarget::Targeted(payload) => {
            assert_eq!(payload.record_ids(), target.record_ids());
            assert_eq!(payload.range(), 0..16);
            assert_eq!(payload.byte_len(), 16);
            assert_eq!(payload.payload.bytes, b"0123456789abcdef".to_vec());
            assert_eq!(
                payload.payload.record.resource_id,
                target.record.resource_id
            );
        }
        other => panic!("expected preview payload target, got {other:?}"),
    }
}

#[test]
fn resident_media_target_can_be_demandable_without_resident_bytes() {
    let (mut core, mut provider, resource_id) = seed_media_presentation(
        ResourceKind::Image,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    let window = realize_frame(&mut core);

    let target = match core.apply_resident_media_action_target(media_command_for_frame_record(
        window.presentation_revision,
        &window.records[0],
    )) {
        ResidentMediaActionOutcome::Targeted(target) => target,
        other => panic!("expected demandable media target, got {other:?}"),
    };

    assert_eq!(
        target.record.range_availability,
        ResidentMediaRangeAvailability::Demandable { range: 0..16 }
    );
    assert_eq!(
        core.resident_media_action_payload(),
        Err(ResidentMediaActionUnavailable::ResourceRangeNotResident {
            resource_id,
            range: 0..16,
        })
    );
}

#[test]
fn resident_media_preview_command_is_unavailable_without_resident_payload_bytes() {
    let (mut core, mut provider, resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    let window = realize_frame(&mut core);
    assert!(matches!(
        core.apply_resident_media_action_target(media_command_for_frame_record(
            window.presentation_revision,
            &window.records[0],
        )),
        ResidentMediaActionOutcome::Targeted(_)
    ));

    assert_eq!(
        ResidentMediaPreviewCommandTarget::from_resident_payload(
            core.resident_media_action_payload()
        ),
        ResidentMediaPreviewCommandTarget::Unavailable(
            ResidentMediaActionUnavailable::ResourceRangeNotResident {
                resource_id,
                range: 0..16,
            }
        )
    );

    let (inactive_core, _provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    assert_eq!(
        ResidentMediaPreviewCommandTarget::from_resident_payload(
            inactive_core.resident_media_action_payload()
        ),
        ResidentMediaPreviewCommandTarget::Unavailable(
            ResidentMediaActionUnavailable::NoActiveMediaActionTarget
        )
    );
}

#[test]
fn resident_media_copy_command_uses_resident_payload_bytes() {
    let (mut core, mut provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    admit_range(&mut core, &mut provider, resource_id("primary"), 0..16);
    let window = realize_frame(&mut core);
    let target = match core.apply_resident_media_action_target(media_command_for_frame_record(
        window.presentation_revision,
        &window.records[0],
    )) {
        ResidentMediaActionOutcome::Targeted(target) => target,
        other => panic!("expected resident media target, got {other:?}"),
    };

    let command_target =
        ResidentMediaCopyCommandTarget::from_resident_payload(core.resident_media_action_payload());

    match command_target {
        ResidentMediaCopyCommandTarget::Targeted(payload) => {
            assert_eq!(payload.record_ids(), target.record_ids());
            assert_eq!(payload.range(), 0..16);
            assert_eq!(payload.byte_len(), 16);
            assert!(payload.complete());
            assert_eq!(payload.media_type(), Some("image/png"));
            assert_eq!(payload.bytes(), b"0123456789abcdef");
            assert_eq!(
                payload.payload.record.resource_id,
                target.record.resource_id
            );
        }
        other => panic!("expected copy payload target, got {other:?}"),
    }
}

#[test]
fn resident_media_copy_command_is_unavailable_without_resident_payload_bytes() {
    let (mut core, mut provider, resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    let window = realize_frame(&mut core);
    assert!(matches!(
        core.apply_resident_media_action_target(media_command_for_frame_record(
            window.presentation_revision,
            &window.records[0],
        )),
        ResidentMediaActionOutcome::Targeted(_)
    ));

    assert_eq!(
        ResidentMediaCopyCommandTarget::from_resident_payload(core.resident_media_action_payload()),
        ResidentMediaCopyCommandTarget::Unavailable(
            ResidentMediaActionUnavailable::ResourceRangeNotResident {
                resource_id,
                range: 0..16,
            }
        )
    );

    let (inactive_core, _provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    assert_eq!(
        ResidentMediaCopyCommandTarget::from_resident_payload(
            inactive_core.resident_media_action_payload()
        ),
        ResidentMediaCopyCommandTarget::Unavailable(
            ResidentMediaActionUnavailable::NoActiveMediaActionTarget
        )
    );
}

#[test]
fn resident_media_save_command_uses_resident_payload_bytes() {
    let (mut core, mut provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    admit_range(&mut core, &mut provider, resource_id("primary"), 0..16);
    let window = realize_frame(&mut core);
    let target = match core.apply_resident_media_action_target(media_command_for_frame_record(
        window.presentation_revision,
        &window.records[0],
    )) {
        ResidentMediaActionOutcome::Targeted(target) => target,
        other => panic!("expected resident media target, got {other:?}"),
    };

    let command_target =
        ResidentMediaSaveCommandTarget::from_resident_payload(core.resident_media_action_payload());

    match command_target {
        ResidentMediaSaveCommandTarget::Targeted(payload) => {
            assert_eq!(payload.record_ids(), target.record_ids());
            assert_eq!(payload.resource_id(), target.record.resource_id);
            assert_eq!(payload.range(), 0..16);
            assert_eq!(payload.byte_len(), 16);
            assert!(payload.complete());
            assert_eq!(payload.media_type(), Some("image/png"));
            assert_eq!(payload.bytes(), b"0123456789abcdef");
            assert_eq!(
                payload.payload.record.resource_id,
                target.record.resource_id
            );
        }
        other => panic!("expected save payload target, got {other:?}"),
    }
}

#[test]
fn resident_media_save_command_is_unavailable_without_resident_payload_bytes() {
    let (mut core, mut provider, resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    let window = realize_frame(&mut core);
    assert!(matches!(
        core.apply_resident_media_action_target(media_command_for_frame_record(
            window.presentation_revision,
            &window.records[0],
        )),
        ResidentMediaActionOutcome::Targeted(_)
    ));

    assert_eq!(
        ResidentMediaSaveCommandTarget::from_resident_payload(core.resident_media_action_payload()),
        ResidentMediaSaveCommandTarget::Unavailable(
            ResidentMediaActionUnavailable::ResourceRangeNotResident {
                resource_id,
                range: 0..16,
            }
        )
    );

    let (inactive_core, _provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    assert_eq!(
        ResidentMediaSaveCommandTarget::from_resident_payload(
            inactive_core.resident_media_action_payload()
        ),
        ResidentMediaSaveCommandTarget::Unavailable(
            ResidentMediaActionUnavailable::NoActiveMediaActionTarget
        )
    );
}

#[test]
fn resident_media_save_destination_requires_explicit_file_path() {
    let explicit_path = std::env::temp_dir().join("resident-media-save-target.png");
    let destination = ResidentMediaSaveDestination::new(explicit_path.clone())
        .expect("temp file path should be explicit");
    assert_eq!(destination.path(), explicit_path.as_path());

    assert_eq!(
        ResidentMediaSaveDestination::new(PathBuf::new()),
        Err(ResidentMediaSaveDestinationUnavailable::EmptyPath)
    );
    assert_eq!(
        ResidentMediaSaveDestination::new(PathBuf::from("relative-media.png")),
        Err(ResidentMediaSaveDestinationUnavailable::RelativePath)
    );
    let filesystem_root = std::env::temp_dir()
        .ancestors()
        .last()
        .expect("temp dir should have a filesystem root")
        .to_path_buf();
    assert_eq!(
        ResidentMediaSaveDestination::new(filesystem_root),
        Err(ResidentMediaSaveDestinationUnavailable::MissingFileName)
    );
}

#[test]
fn resident_media_payload_is_unavailable_without_active_target() {
    let (core, _provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );

    assert_eq!(
        core.resident_media_action_payload(),
        Err(ResidentMediaActionUnavailable::NoActiveMediaActionTarget)
    );
}

#[test]
fn media_target_rejects_stale_missing_not_realized_and_unstable_records() {
    let (mut core, mut provider, _resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    admit_metadata(&mut core, &mut provider);
    let window = realize_frame(&mut core);
    let stale_command = media_command_for_frame_record(
        window.presentation_revision.saturating_sub(1),
        &window.records[0],
    );

    assert_eq!(
        core.apply_resident_media_action_target(stale_command),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::StalePresentationRevision {
                observed: window.presentation_revision.saturating_sub(1),
                current: window.presentation_revision,
            }
        )
    );

    let missing_id = ResidentPresentationRecordId("missing-media-record".to_string());
    assert_eq!(
        core.apply_resident_media_action_target(ResidentMediaActionCommand::new(
            window.presentation_revision,
            ResidentSelectionRecordGeometry::new(missing_id.clone(), 0.0, 20.0),
        )),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::RecordNotResident {
                record_id: missing_id,
            }
        )
    );

    let mut not_realized_core = seed_two_media_presentation();
    let not_realized_revision = not_realized_core
        .presentation_snapshot()
        .presentation_revision;
    not_realized_core.push_demand_fact(DemandFact::new(
        not_realized_revision,
        DemandFactKind::OverscanRange { range: 0..1 },
    ));
    let not_realized_id = not_realized_core.presentation_snapshot().records[1]
        .id
        .clone();
    assert_eq!(
        not_realized_core.apply_resident_media_action_target(ResidentMediaActionCommand::new(
            not_realized_revision,
            ResidentSelectionRecordGeometry::new(not_realized_id.clone(), 0.0, 20.0),
        )),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::RecordNotRealized {
                record_id: not_realized_id,
            }
        )
    );

    assert_eq!(
        core.apply_resident_media_action_target(ResidentMediaActionCommand::new(
            window.presentation_revision,
            ResidentSelectionRecordGeometry::new(window.records[0].record_id.clone(), 0.0, 0.0),
        )),
        ResidentMediaActionOutcome::Unavailable(ResidentMediaActionUnavailable::UnstableGeometry {
            record_id: window.records[0].record_id.clone(),
        })
    );
}

#[test]
fn media_target_rejects_fallback_local_and_non_media_records() {
    let mut fallback_core = seed_budget_fallback_presentation();
    let fallback_window = realize_frame(&mut fallback_core);
    assert!(matches!(
        fallback_core.presentation_snapshot().records[0].kind,
        ResidentPresentationRecordKind::LocalUiFallback { .. }
    ));
    assert!(matches!(
        fallback_core.apply_resident_media_action_target(media_command_for_frame_record(
            fallback_window.presentation_revision,
            &fallback_window.records[0],
        )),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::NonMediaRecord { .. }
        )
    ));

    let local_record_id = ResidentPresentationRecordId("local-affordance".to_string());
    let local_record = ResidentPresentationRecord {
        id: local_record_id.clone(),
        kind: ResidentPresentationRecordKind::LocalAffordance,
        provenance: ResidentRecordProvenance {
            source: ResidentRecordSource::LocalUi,
            projection_id: None,
            projection_revision: None,
            presentation_revision: fallback_window.presentation_revision,
            copy_source_range: None,
        },
        estimated_bytes: 0,
    };
    assert_eq!(
        resident_media_reference(&local_record),
        Err(ResidentMediaActionUnavailable::NonMediaRecord {
            record_id: local_record_id,
        })
    );

    let (mut code_core, mut code_provider, _resource_id) =
        seed_media_presentation(ResourceKind::Code, b"code bytes".to_vec(), Some(0..10));
    admit_metadata(&mut code_core, &mut code_provider);
    let code_window = realize_frame(&mut code_core);
    assert!(matches!(
        code_core.apply_resident_media_action_target(media_command_for_frame_record(
            code_window.presentation_revision,
            &code_window.records[0],
        )),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::NonMediaRecord { .. }
        )
    ));
}

#[test]
fn media_target_rejects_missing_provenance_and_missing_metadata() {
    let (mut missing_range_core, mut missing_range_provider, _resource_id) =
        seed_media_presentation(
            ResourceKind::GeneratedImage,
            b"0123456789abcdef".to_vec(),
            None,
        );
    admit_metadata(&mut missing_range_core, &mut missing_range_provider);
    let missing_range_window = realize_frame(&mut missing_range_core);
    assert!(matches!(
        missing_range_core.apply_resident_media_action_target(media_command_for_frame_record(
            missing_range_window.presentation_revision,
            &missing_range_window.records[0],
        )),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::MissingStableProvenance { .. }
        )
    ));

    let (mut missing_metadata_core, _provider, resource_id) = seed_media_presentation(
        ResourceKind::GeneratedImage,
        b"0123456789abcdef".to_vec(),
        Some(0..16),
    );
    let missing_metadata_window = realize_frame(&mut missing_metadata_core);
    assert_eq!(
        missing_metadata_core.apply_resident_media_action_target(media_command_for_frame_record(
            missing_metadata_window.presentation_revision,
            &missing_metadata_window.records[0],
        )),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::MissingResourceMetadata {
                record_id: missing_metadata_window.records[0].record_id.clone(),
                resource_id,
            }
        )
    );
}

#[test]
fn media_target_rejects_rejected_range_facts() {
    let (mut rejected_range_core, mut rejected_range_provider, range_resource_id) =
        seed_media_presentation(
            ResourceKind::GeneratedImage,
            b"0123456789abcdef".to_vec(),
            Some(0..16),
        );
    admit_metadata(&mut rejected_range_core, &mut rejected_range_provider);
    rejected_range_provider.reject_resource_with_message(
        range_resource_id.clone(),
        TranscriptProviderRejectionReason::RangeOutOfBounds,
        "range rejected",
    );
    let range_request = rejected_range_core
        .request_resource_range(
            range_resource_id.clone(),
            0..16,
            ProviderRequestReason::ResourceRange,
        )
        .expect("resident metadata should allow range request before rejection");
    assert_eq!(
        handle_provider_request(
            &mut rejected_range_core,
            &mut rejected_range_provider,
            range_request,
        ),
        ResidentProviderResponseEffect::Rejected
    );
    let rejected_range_window = realize_frame(&mut rejected_range_core);
    let rejected_range_snapshot = rejected_range_core.presentation_snapshot();
    assert_eq!(
        realized_resident_media_action_record_ids(&rejected_range_snapshot, &rejected_range_window),
        Vec::<ResidentPresentationRecordId>::new()
    );
    assert!(matches!(
        resident_media_action_command_for_realized_record_id(
            &rejected_range_snapshot,
            &rejected_range_window,
            &rejected_range_window.records[0].record_id,
        ),
        Err(ResidentMediaActionUnavailable::NonMediaRecord { .. })
    ));
    assert!(matches!(
        rejected_range_core.apply_resident_media_action_target(media_command_for_frame_record(
            rejected_range_window.presentation_revision,
            &rejected_range_window.records[0],
        )),
        ResidentMediaActionOutcome::Unavailable(
            ResidentMediaActionUnavailable::NonMediaRecord { .. }
        )
    ));
    assert!(
        rejected_range_core
            .core_snapshot()
            .resident
            .resource_rejections
            .iter()
            .any(|rejection| {
                rejection.target
                    == TranscriptProviderTarget::ResourceRange {
                        resource_id: range_resource_id.clone(),
                        range: 0..16,
                    }
                    && rejection.reason == TranscriptProviderRejectionReason::RangeOutOfBounds
            })
    );
}
