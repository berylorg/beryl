use super::*;

fn demand_core(byte_len: usize) -> (ResidentTranscriptCore, ResourceId) {
    let (mut core, _, id) = seed_resource_presentation(
        ResidentTranscriptPolicy::default(),
        ResourceKind::Code,
        vec![b'x'; byte_len],
    );
    core.push_demand_fact(DemandFact::new(
        core.presentation_snapshot().presentation_revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    (core, id)
}

fn descriptor(id: ResourceId, byte_len: u64) -> ResourceMetadata {
    ResourceMetadata {
        revision: ProviderRevision(900),
        byte_len,
        preview_range: Some(200..byte_len),
        ..metadata(id, ResourceKind::Code)
    }
}

#[test]
fn metadata_precedes_ranges_and_resource_revisions_do_not_replace_history() {
    let (mut core, id) = demand_core(1000);
    assert!(
        core.request_resource_range(id.clone(), 0..10, ProviderRequestReason::ResourceRange)
            .is_none()
    );
    let demand = core.resource_demand().unwrap();
    assert!(demand.metadata.is_none());
    let before = core.shared_presentation_snapshot();
    core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(descriptor(id.clone(), 1000)),
        )
        .unwrap();
    assert_eq!(
        core.core_snapshot().resident.provider_revision,
        Some(REVISION)
    );
    assert!(before.resources.metadata.is_empty());
    let demand = core.resource_demand().unwrap();
    assert_eq!(demand.range, Some(200..1000));
    assert_eq!(demand.metadata.unwrap().revision, ProviderRevision(900));
    assert!(
        core.request_resource_range(id, 0..1001, ProviderRequestReason::ResourceRange)
            .is_none()
    );
}

#[test]
fn bounded_continuations_keep_relative_offsets_and_split_utf8_bytes_exact() {
    let (core, id) = demand_core(150_000);
    let demand = core.resource_demand().unwrap();
    let mut core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(descriptor(id.clone(), 150_000)),
        )
        .unwrap();
    let mut expected = vec![b'x'; 149_800];
    expected[65_535] = 0xc3;
    expected[65_536] = 0xa9;
    let mut start = 200;
    let mut offset = 0;
    while let Some(demand) = core.resource_demand() {
        let range = demand.range.clone().unwrap();
        assert_eq!(range, start..150_000);
        let length = (range.end - range.start).min(65_536) as usize;
        let end = start + length as u64;
        core = core
            .prepare_resource_response(
                &demand,
                TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
                    resource_id: id.clone(),
                    revision: ProviderRevision(900),
                    kind: ResourceKind::Code,
                    range: start..end,
                    bytes: expected[offset..offset + length].to_vec(),
                    complete: end == range.end,
                }),
            )
            .unwrap();
        offset += length;
        start = end;
    }
    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.resource_slices.len(), 3);
    let actual: Vec<u8> = snapshot
        .resident
        .resource_slices
        .iter()
        .flat_map(|slice| slice.bytes.iter().copied())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(snapshot.resident.provider_revision, Some(REVISION));
}

#[test]
fn wrong_resource_revision_kind_extent_count_and_completion_are_not_admitted() {
    let (core, id) = demand_core(1000);
    let demand = core.resource_demand().unwrap();
    let core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(descriptor(id.clone(), 1000)),
        )
        .unwrap();
    for fault in 0..7 {
        let mut candidate = core.clone();
        let request = candidate
            .request_resource_range(id.clone(), 200..210, ProviderRequestReason::ResourceRange)
            .unwrap();
        let mut range = ResourceRangeResponse {
            resource_id: id.clone(),
            revision: ProviderRevision(900),
            kind: ResourceKind::Code,
            range: 200..210,
            bytes: vec![0; 10],
            complete: true,
        };
        match fault {
            0 => range.resource_id = resource_id("wrong"),
            1 => range.revision = ProviderRevision(901),
            2 => range.kind = ResourceKind::Table,
            3 => range.range = 201..211,
            4 => range.bytes.pop().map(|_| ()).unwrap(),
            5 => range.complete = false,
            _ => range.range = 200..1200,
        }
        assert_eq!(
            candidate.handle_provider_response(TranscriptProviderResponse {
                request_id: request.id,
                kind: TranscriptProviderResponseKind::ResourceRange(range),
            }),
            ResidentProviderResponseEffect::Ignored
        );
        assert!(
            candidate
                .core_snapshot()
                .resident
                .resource_slices
                .is_empty()
        );
        assert_eq!(candidate.provider_request_snapshot().pending_count, 0);
    }
}

#[test]
fn invalid_metadata_and_wrong_request_kind_leave_previous_snapshot_intact() {
    let (core, id) = demand_core(1000);
    let demand = core.resource_demand().unwrap();
    let mut invalid = descriptor(id, 1000);
    invalid.preview_range = Some(900..1001);
    assert!(
        core.prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(invalid)
        )
        .is_none()
    );
    let mut candidate = core.clone();
    let request = candidate
        .request_resource_metadata_for_presentation_records(ProviderRequestReason::ResourceMetadata)
        .unwrap();
    assert_eq!(
        candidate.handle_provider_response(TranscriptProviderResponse {
            request_id: request.id,
            kind: TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
                resource_id: demand.resource_id,
                revision: ProviderRevision(900),
                kind: ResourceKind::Code,
                range: 0..1,
                bytes: vec![0],
                complete: true,
            }),
        }),
        ResidentProviderResponseEffect::Ignored
    );
    assert!(
        candidate
            .core_snapshot()
            .resident
            .resource_metadata
            .is_empty()
    );
}

#[test]
fn empty_preview_is_settled_and_capacity_refusal_is_local() {
    let (core, id) = demand_core(1000);
    let demand = core.resource_demand().unwrap();
    let mut empty = descriptor(id.clone(), 1000);
    empty.preview_range = Some(200..200);
    let settled = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(empty),
        )
        .unwrap();
    assert!(settled.resource_demand().is_none());
    let (mut limited, mut provider, id) = seed_resource_presentation(
        ResidentTranscriptPolicy {
            max_resource_bytes: 2,
            ..ResidentTranscriptPolicy::default()
        },
        ResourceKind::Code,
        vec![0; 1000],
    );
    limited.push_demand_fact(DemandFact::new(
        limited.presentation_snapshot().presentation_revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    admit_metadata(&mut limited, &mut provider);
    let demand = limited.resource_demand().unwrap();
    let refused = limited
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
                resource_id: id,
                revision: REVISION,
                kind: ResourceKind::Code,
                range: 0..4,
                bytes: vec![0; 4],
                complete: true,
            }),
        )
        .unwrap();
    assert!(refused.resource_demand().is_none());
    assert_eq!(refused.core_snapshot().resident.resource_bytes, 0);
    assert_eq!(refused.core_snapshot().resident.budget_rejection_count, 1);
}

#[test]
fn removal_releases_metadata_slices_and_pending_requests_without_mutating_prior_snapshot() {
    let (core, id) = demand_core(1000);
    let demand = core.resource_demand().unwrap();
    let mut core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(descriptor(id.clone(), 1000)),
        )
        .unwrap();
    let demand = core.resource_demand().unwrap();
    core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
                resource_id: id.clone(),
                revision: ProviderRevision(900),
                kind: ResourceKind::Code,
                range: 200..210,
                bytes: vec![0; 10],
                complete: false,
            }),
        )
        .unwrap();
    let before = core.shared_presentation_snapshot();
    let pending = core
        .request_resource_range(id, 210..220, ProviderRequestReason::ResourceRange)
        .unwrap();
    let revision = before.presentation_revision;
    for kind in [
        DemandFactKind::VisibleRange { range: 0..0 },
        DemandFactKind::OverscanRange { range: 0..0 },
        DemandFactKind::CurrentAnchor {
            record_id: None,
            position: None,
        },
    ] {
        core.push_demand_fact(DemandFact::new(revision, kind));
    }
    core.reconcile_resource_demand();
    assert!(core.resource_demand().is_none());
    assert!(core.core_snapshot().resident.resource_metadata.is_empty());
    assert!(core.core_snapshot().resident.resource_slices.is_empty());
    assert_eq!(core.provider_request_snapshot().pending_count, 0);
    assert_eq!(before.resources.metadata.len(), 1);
    assert_eq!(before.resources.slices.len(), 1);
    assert_eq!(
        core.handle_provider_response(TranscriptProviderResponse {
            request_id: pending.id,
            kind: TranscriptProviderResponseKind::ResourceMetadata(descriptor(
                resource_id("wrong"),
                1000
            )),
        }),
        ResidentProviderResponseEffect::UnknownRequest
    );
}

#[test]
fn malformed_oversized_response_cannot_install_capacity_fallback() {
    let (core, id) = demand_core(150_000);
    let demand = core.resource_demand().unwrap();
    let mut core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceMetadata(descriptor(id, 150_000)),
        )
        .unwrap();
    let demand = core.resource_demand().unwrap();
    // A forged payload exceeds the slice cap as well as disagreeing with its byte interval.
    let response = TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
        resource_id: resource_id("forged"),
        revision: ProviderRevision(901),
        kind: ResourceKind::Code,
        range: 200..150_000,
        bytes: vec![0; 1_048_577],
        complete: true,
    });
    assert!(core.prepare_resource_response(&demand, response).is_none());
    assert_eq!(core.core_snapshot().resident.budget_rejection_count, 0);
    assert!(core.core_snapshot().resident.resource_slices.is_empty());
    let pending = core
        .request_resource_range(
            demand.resource_id,
            200..210,
            ProviderRequestReason::ResourceRange,
        )
        .unwrap();
    assert_eq!(core.provider_request_snapshot().pending_count, 1);
    core.finish_provider_request(pending.id, ProviderRequestOutcome::Error);
}

#[test]
fn zero_length_short_and_absent_previews_do_not_guess_resource_extents() {
    for (length, preview, expected) in [
        (0, None, None),
        (3, None, Some(0..3)),
        (3, Some(1..2), Some(1..2)),
        (3, Some(2..2), None),
    ] {
        let (core, id) = demand_core(length as usize);
        let demand = core.resource_demand().unwrap();
        let mut metadata = descriptor(id, length);
        metadata.preview_range = preview;
        let core = core
            .prepare_resource_response(
                &demand,
                TranscriptProviderResponseKind::ResourceMetadata(metadata),
            )
            .unwrap();
        assert_eq!(
            core.resource_demand().and_then(|demand| demand.range),
            expected
        );
    }
}

#[test]
fn terminal_capacity_refusal_releases_partially_loaded_bytes_but_preserves_prior_snapshot() {
    let (mut core, mut provider, id) = seed_resource_presentation(
        ResidentTranscriptPolicy {
            max_resource_bytes: 3,
            ..ResidentTranscriptPolicy::default()
        },
        ResourceKind::Code,
        vec![0; 1000],
    );
    core.push_demand_fact(DemandFact::new(
        core.presentation_snapshot().presentation_revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    admit_metadata(&mut core, &mut provider);
    let demand = core.resource_demand().unwrap();
    core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
                resource_id: id.clone(),
                revision: REVISION,
                kind: ResourceKind::Code,
                range: 0..2,
                bytes: vec![0; 2],
                complete: false,
            }),
        )
        .unwrap();
    let before = core.shared_presentation_snapshot();
    let demand = core.resource_demand().unwrap();
    assert_eq!(demand.range, Some(2..4));
    let core = core
        .prepare_resource_response(
            &demand,
            TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
                resource_id: id,
                revision: REVISION,
                kind: ResourceKind::Code,
                range: 2..4,
                bytes: vec![0; 2],
                complete: true,
            }),
        )
        .unwrap();
    assert_eq!(core.core_snapshot().resident.resource_bytes, 0);
    assert_eq!(core.core_snapshot().resident.budget_rejection_count, 1);
    assert!(core.resource_demand().is_none());
    assert_eq!(core.core_snapshot().resident.resource_metadata.len(), 1);
    assert_eq!(before.resources.slices[0].bytes.len(), 2);
}
