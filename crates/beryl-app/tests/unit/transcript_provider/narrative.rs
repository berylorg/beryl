use super::*;
use crate::syndic_transcript::{
    NarrativePreparationResponse, ResidentPresentationRecordKind, ResidentTranscriptCore,
    SyndicTranscriptHost, TranscriptActivationSource,
};

fn published_seed(
    fixture: &Fixture,
    cancel: &AtomicBool,
) -> (
    PreparedTranscriptActivation,
    TranscriptAttachmentSourceIdentity,
) {
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), cancel)
        .unwrap();
    let source = prepared.source_identity();
    let seed = fixture
        .reader
        .publish_if_current(prepared, &request, cancel, |seed| seed)
        .unwrap();
    (seed, source)
}

fn expose_core_records(core: &mut ResidentTranscriptCore) {
    let snapshot = core.presentation_snapshot();
    let revision = snapshot.presentation_revision;
    let range = 0..snapshot.records.len();
    core.push_demand_fact(crate::syndic_transcript::DemandFact::new(
        revision,
        crate::syndic_transcript::DemandFactKind::OverscanRange {
            range: range.clone(),
        },
    ));
    core.push_demand_fact(crate::syndic_transcript::DemandFact::new(
        revision,
        crate::syndic_transcript::DemandFactKind::VisibleRange { range },
    ));
}

fn expose_host_records(host: &mut SyndicTranscriptHost) {
    let snapshot = host.snapshot();
    let revision = snapshot.presentation_revision;
    let range = 0..snapshot.records.len();
    host.push_demand_fact(crate::syndic_transcript::DemandFact::new(
        revision,
        crate::syndic_transcript::DemandFactKind::OverscanRange {
            range: range.clone(),
        },
    ));
    host.push_demand_fact(crate::syndic_transcript::DemandFact::new(
        revision,
        crate::syndic_transcript::DemandFactKind::VisibleRange { range },
    ));
}

#[test]
fn narrative_pending_seed_prepares_only_after_publication_and_adopts_exact_source() {
    let fixture = Fixture::new();
    fixture.seed_projection("**saved** source", 1);
    let cancel = AtomicBool::new(false);
    let (seed, source) = published_seed(&fixture, &cancel);
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    expose_core_records(&mut core);
    assert!(matches!(
        core.presentation_snapshot().records[0].kind,
        ResidentPresentationRecordKind::NarrativePending { .. }
    ));
    let demand = core.narrative_demand().unwrap();
    let allowance = core.reserve_narrative_preparation(&demand).unwrap();
    assert_eq!(core.provider_request_snapshot().pending_count, 1);
    assert!(core.reserve_narrative_preparation(&demand).is_none());
    let mut attachment = fixture.request(2);
    attachment.purpose = TranscriptAttachmentPurpose::Narrative;
    let request = TranscriptNarrativeRequest {
        attachment,
        source,
        demand: demand.clone(),
        allowance,
    };
    let prepared = fixture
        .reader
        .prepare_narrative(request.clone(), &cancel)
        .unwrap();
    let prior_shared = core.shared_presentation_snapshot();
    assert!(matches!(
        prior_shared.records[0].kind,
        ResidentPresentationRecordKind::NarrativePending { .. }
    ));
    let candidate = core
        .prepare_narrative_delivery(&demand, prepared.response())
        .unwrap();
    let candidate_shared = candidate.shared_presentation_snapshot();
    assert!(!Arc::ptr_eq(&prior_shared, &candidate_shared));
    assert!(matches!(
        candidate_shared.records[0].kind,
        ResidentPresentationRecordKind::NarrativeReady { .. }
    ));
    assert!(matches!(
        core.presentation_snapshot().records[0].kind,
        ResidentPresentationRecordKind::NarrativePending { .. }
    ));
    let snapshot = candidate.presentation_snapshot();
    let ResidentPresentationRecordKind::NarrativeReady { prepared: model } =
        &snapshot.records[0].kind
    else {
        panic!("ready candidate missing")
    };
    assert_eq!(model.source(), "**saved** source");
    assert_eq!(model.display(), "saved source");
    fixture
        .reader
        .publish_narrative_if_current(prepared, &request, &cancel, |_| ())
        .unwrap();
    core.finish_narrative_preparation(&demand);
    assert_eq!(core.provider_request_snapshot().pending_count, 0);
    assert!(fixture.reader.retire());
}

#[test]
fn narrative_refusal_retains_original_payload_and_releases_provider_gate() {
    let fixture = Fixture::new();
    fixture.seed_projection("**bounded source**", 1);
    let cancel = AtomicBool::new(false);
    let (seed, source) = published_seed(&fixture, &cancel);
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    expose_core_records(&mut core);
    let demand = core.narrative_demand().unwrap();
    core.reserve_narrative_preparation(&demand).unwrap();
    let mut attachment = fixture.request(2);
    attachment.purpose = TranscriptAttachmentPurpose::Narrative;
    let request = TranscriptNarrativeRequest {
        attachment,
        source,
        demand: demand.clone(),
        allowance: 0,
    };
    let prepared = fixture
        .reader
        .prepare_narrative(request.clone(), &cancel)
        .unwrap();
    assert!(
        matches!(prepared.response(), NarrativePreparationResponse::Unavailable { ref source, .. } if Arc::ptr_eq(source, &demand.source))
    );
    let candidate = core
        .prepare_narrative_delivery(&demand, prepared.response())
        .unwrap();
    assert!(
        matches!(&candidate.presentation_snapshot().records[0].kind, ResidentPresentationRecordKind::NarrativeUnavailable { source, .. } if Arc::ptr_eq(source, &demand.source))
    );
    fixture
        .reader
        .publish_narrative_if_current(prepared, &request, &cancel, |_| ())
        .unwrap();
    core.finish_narrative_preparation(&demand);
    assert_eq!(core.provider_request_snapshot().pending_count, 0);
    assert!(fixture.reader.retire());
}

#[test]
fn narrative_late_completion_rejects_changed_keys_cancellation_and_mutation() {
    for changed in 0..3 {
        let fixture = Fixture::new();
        fixture.seed_projection("saved **source**", 1);
        let cancel = AtomicBool::new(false);
        let (seed, source) = published_seed(&fixture, &cancel);
        let mut core = ResidentTranscriptCore::empty();
        core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
        expose_core_records(&mut core);
        let demand = core.narrative_demand().unwrap();
        let allowance = core.reserve_narrative_preparation(&demand).unwrap();
        let mut attachment = fixture.request(2);
        attachment.purpose = TranscriptAttachmentPurpose::Narrative;
        let request = TranscriptNarrativeRequest {
            attachment,
            source,
            demand: demand.clone(),
            allowance,
        };
        let prepared = fixture
            .reader
            .prepare_narrative(request.clone(), &cancel)
            .unwrap();
        let mut expected = request.clone();
        match changed {
            0 => expected.demand.projection_revision.0 += 1,
            1 => cancel.store(true, Ordering::Release),
            _ => fixture.seed_head(2),
        }
        let called = AtomicBool::new(false);
        let result =
            fixture
                .reader
                .publish_narrative_if_current(prepared, &expected, &cancel, |_| {
                    called.store(true, Ordering::Release)
                });
        assert_eq!(
            result,
            Err(match changed {
                0 => TranscriptAttachmentError::Identity,
                1 => TranscriptAttachmentError::Cancelled,
                _ => TranscriptAttachmentError::Stale,
            })
        );
        assert!(!called.load(Ordering::Acquire));
        core.finish_narrative_preparation(&demand);
        assert_eq!(core.provider_request_snapshot().pending_count, 0);
        assert!(fixture.reader.retire());
    }
}

#[test]
fn narrative_same_source_refresh_reuses_model_and_growth_keeps_old_binding() {
    let fixture = Fixture::new();
    fixture.seed_projection("**saved**", 1);
    let cancel = AtomicBool::new(false);
    let (seed, _) = published_seed(&fixture, &cancel);
    let mut host = SyndicTranscriptHost::empty();
    host.publish_coherent_activation(seed.clone());
    expose_host_records(&mut host);
    let demand = host.narrative_demand().unwrap();
    let allowance = host.reserve_narrative_preparation(&demand).unwrap();
    let model = Arc::new(
        syndic_storage::narrative::prepare_narrative(
            demand.source.as_ref().clone(),
            allowance - 16,
        )
        .unwrap(),
    );
    host = host
        .prepare_narrative_delivery(&demand, NarrativePreparationResponse::Ready(model.clone()))
        .unwrap();
    let (same, _) = host.prepare_coherent_activation(seed);
    let snapshot = same.snapshot();
    let ResidentPresentationRecordKind::NarrativeReady { prepared } = &snapshot.records[0].kind
    else {
        panic!("reused model missing")
    };
    assert!(Arc::ptr_eq(prepared, &model));
    assert!(same.narrative_demand().is_none());
    fixture.seed_projection("**saved** growth", 1);
    let (grown_seed, _) = published_seed(&fixture, &cancel);
    let (mut grown, _) = host.prepare_coherent_activation(grown_seed);
    let before = host.snapshot();
    let after = grown.snapshot();
    assert_eq!(
        after.records[0].provenance.source,
        before.records[0].provenance.source
    );
    let ResidentPresentationRecordKind::NarrativeReady { prepared } = &after.records[0].kind else {
        panic!("growth blanked old presentation")
    };
    assert!(Arc::ptr_eq(prepared, &model));
    expose_host_records(&mut grown);
    let replacement = grown.narrative_demand().unwrap();
    assert_ne!(replacement.source, demand.source);
    grown.reserve_narrative_preparation(&replacement).unwrap();
    grown.cancel_narrative_preparations();
    assert!(grown.narrative_demand().is_some());
}

#[test]
fn unchanged_refresh_renews_preparation_observation_without_replacing_ready_model() {
    let fixture = Fixture::new();
    fixture.seed_projection("saved **source**", 1);
    let cancel = AtomicBool::new(false);
    let (seed, source) = published_seed(&fixture, &cancel);
    let mut core = ResidentTranscriptCore::empty();
    core.apply_prepared_activation(seed.clone(), TranscriptActivationSource::RunningThread);
    expose_core_records(&mut core);
    let demand = core.narrative_demand().unwrap();
    let allowance = core.reserve_narrative_preparation(&demand).unwrap();
    let mut attachment = fixture.request(2);
    attachment.purpose = TranscriptAttachmentPurpose::Narrative;
    let request = TranscriptNarrativeRequest {
        attachment,
        source: source.clone(),
        demand: demand.clone(),
        allowance,
    };
    let prepared = fixture
        .reader
        .prepare_narrative(request.clone(), &cancel)
        .unwrap();
    let current = core
        .prepare_narrative_delivery(&demand, prepared.response())
        .unwrap();
    fixture
        .reader
        .publish_narrative_if_current(prepared, &request, &cancel, |_| ())
        .unwrap();
    core.finish_narrative_preparation(&demand);
    let snapshot = current.presentation_snapshot();
    let ResidentPresentationRecordKind::NarrativeReady { prepared: model } =
        &snapshot.records[0].kind
    else {
        panic!("ready model missing")
    };

    let binding = ExecutionBinding::new(
        RuntimeId::from_bytes([2; 16]),
        RootId::from_bytes([3; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\Transcript",
        )
        .unwrap(),
    );
    execute(
        &fixture.home,
        fixture.storage.create_thread(
            fixture.storage.revision(&fixture.home).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([31; 16]),
                SyndicDraftId::from_bytes([32; 16]),
                binding,
                SyndicTimestamp::from_unix_millis(2),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    assert!(!fixture.reader.narrative_source_current(&source));
    assert!(matches!(
        fixture.reader.prepare_narrative(request.clone(), &cancel),
        Err(TranscriptAttachmentError::Stale)
    ));

    let mut refresh = fixture.request(3);
    refresh.purpose = TranscriptAttachmentPurpose::Refresh;
    let refreshed = fixture
        .reader
        .prepare_attachment(refresh.clone(), &cancel)
        .unwrap();
    assert!(source.matches_authority(&refreshed));
    assert!(source.matches_selected_metadata(&refreshed));
    fixture
        .reader
        .publish_if_current(refreshed, &refresh, &cancel, |_| ())
        .unwrap();
    assert!(fixture.reader.narrative_source_current(&source));
    let current_snapshot = current.presentation_snapshot();
    let ResidentPresentationRecordKind::NarrativeReady { prepared: retained } =
        &current_snapshot.records[0].kind
    else {
        panic!("ready model replaced")
    };
    assert!(Arc::ptr_eq(model, retained));
    let prepared = fixture
        .reader
        .prepare_narrative(request.clone(), &cancel)
        .unwrap();
    fixture
        .reader
        .publish_narrative_if_current(prepared, &request, &cancel, |_| ())
        .unwrap();
    assert!(fixture.reader.retire());
}

#[test]
fn narrative_preparation_uses_each_response_allowance_after_multiple_pages_and_prioritizes_realized_rows()
 {
    use crate::syndic_transcript::*;
    let fixture = Fixture::new();
    let text = "x".repeat(18_000);
    fixture.seed_projection(&text, 1);
    let cancel = AtomicBool::new(false);
    let (seed, _) = published_seed(&fixture, &cancel);
    let Some(TranscriptProviderResponseKind::ViewPage(template_page)) =
        seed.view_page_response.clone()
    else {
        panic!("view page missing")
    };
    let Some(TranscriptProviderResponseKind::ProjectionRecords(template_set)) =
        seed.projection_records_response.clone()
    else {
        panic!("projection set missing")
    };
    let mut policy = ResidentTranscriptPolicy::default();
    policy.max_pending_provider_requests = 2;
    let mut core = ResidentTranscriptCore::new(policy);
    for positions in [1..3, 3..5] {
        let mut page = template_page.clone();
        let mut set = template_set.clone();
        page.records.clear();
        set.records.clear();
        for position in positions.clone() {
            let mut view = template_page.records[0].clone();
            let mut projection = template_set.records[0].clone();
            let id = ProjectionRecordId(format!("projection-{position}"));
            projection.id = id.clone();
            projection.provenance.projection_id = Some(id.clone());
            projection.provenance.item_id = Some(SyndicItemId(format!("item-{position}")));
            projection.provenance.position = Some(TranscriptViewPosition(position));
            projection.payload = crate::syndic_transcript::ProjectionPayload::NarrativePending {
                source: Arc::new(
                    syndic_storage::ProjectionPayload::inline_markdown(
                        syndic_storage::MarkdownBlockId::from_bytes([position as u8; 32]),
                        syndic_storage::MarkdownBlockKind::Paragraph,
                        1,
                        syndic_storage::ProjectionSourceRange::new(0, text.len() as u64).unwrap(),
                        text.as_str(),
                    )
                    .unwrap(),
                ),
            };
            view.id = TranscriptViewRecordId(format!("view-{position}"));
            view.position = TranscriptViewPosition(position);
            view.projection_id = id;
            view.provenance = projection.provenance.clone();
            page.records.push(view);
            set.records.push(projection);
        }
        page.at_start = positions.start == 1;
        page.at_end = positions.end == 5;
        let request = core.request_view_page(
            page.view_id.clone(),
            TranscriptPageAnchor::End,
            TranscriptPageDirection::Forward,
            ProviderRequestReason::VisibleRange,
        );
        core.handle_provider_response(TranscriptProviderResponse {
            request_id: request.id,
            kind: TranscriptProviderResponseKind::ViewPage(page),
        });
        let request = core
            .request_projection_records_for_resident_view(
                ProviderRequestReason::ProjectionAdmission,
            )
            .unwrap();
        core.handle_provider_response(TranscriptProviderResponse {
            request_id: request.id,
            kind: TranscriptProviderResponseKind::ProjectionRecords(set),
        });
    }
    assert!(core.core_snapshot().resident.projection_bytes > ATTACHMENT_MAX_BYTES);
    assert!(core.narrative_demand().is_none());
    let snapshot = core.presentation_snapshot();
    let revision = snapshot.presentation_revision;
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::OverscanRange { range: 2..4 },
    ));
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::VisibleRange { range: 3..4 },
    ));
    let demand = core.narrative_demand().unwrap();
    assert_eq!(demand.record_id, snapshot.records[3].id);
    let allowance = core.reserve_narrative_preparation(&demand).unwrap();
    let prepared = Arc::new(
        syndic_storage::narrative::prepare_narrative(
            demand.source.as_ref().clone(),
            allowance - 16,
        )
        .unwrap(),
    );
    assert!(
        core.prepare_narrative_delivery(&demand, NarrativePreparationResponse::Ready(prepared))
            .is_some()
    );
    assert!(core.reserve_narrative_preparation(&demand).is_none());
    let adjacent = core.narrative_demand().unwrap();
    assert_eq!(adjacent.record_id, snapshot.records[2].id);
    core.reserve_narrative_preparation(&adjacent).unwrap();
    assert_eq!(core.provider_request_snapshot().pending_count, 2);
    core.push_demand_fact(DemandFact::new(
        revision,
        DemandFactKind::OverscanRange { range: 0..4 },
    ));
    let third = core.narrative_demand().unwrap();
    assert!(core.reserve_narrative_preparation(&third).is_none());
    core.finish_narrative_preparation(&demand);
    assert!(core.reserve_narrative_preparation(&third).is_some());
    core.finish_narrative_preparation(&third);
    core.finish_narrative_preparation(&adjacent);
    assert_eq!(core.provider_request_snapshot().pending_count, 0);
}

#[test]
fn narrative_record_old_and_candidate_capacity_refusal_preserves_source_and_count_release() {
    let fixture = Fixture::new();
    fixture.seed_projection(&"x".repeat(36_000), 1);
    let cancel = AtomicBool::new(false);
    let (seed, source) = published_seed(&fixture, &cancel);
    let mut policy = crate::syndic_transcript::ResidentTranscriptPolicy::default();
    policy.max_pending_provider_requests = 1;
    let mut core = ResidentTranscriptCore::new(policy);
    core.apply_prepared_activation(seed, TranscriptActivationSource::RunningThread);
    expose_core_records(&mut core);
    let demand = core.narrative_demand().unwrap();
    let allowance = core.reserve_narrative_preparation(&demand).unwrap();
    assert!(allowance < crate::syndic_transcript::narrative_source_bytes(&demand.source));
    assert!(core.reserve_narrative_preparation(&demand).is_none());
    let mut attachment = fixture.request(2);
    attachment.purpose = TranscriptAttachmentPurpose::Narrative;
    let request = TranscriptNarrativeRequest {
        attachment,
        source,
        demand: demand.clone(),
        allowance,
    };
    let prepared = fixture
        .reader
        .prepare_narrative(request.clone(), &cancel)
        .unwrap();
    assert!(
        matches!(prepared.response(), NarrativePreparationResponse::Unavailable { ref source, .. } if Arc::ptr_eq(source, &demand.source))
    );
    fixture
        .reader
        .publish_narrative_if_current(prepared, &request, &cancel, |_| ())
        .unwrap();
    core.finish_narrative_preparation(&demand);
    assert_eq!(core.provider_request_snapshot().pending_count, 0);
    assert!(core.reserve_narrative_preparation(&demand).is_some());
    core.cancel_narrative_preparations();
    assert_eq!(core.provider_request_snapshot().pending_count, 0);
}
