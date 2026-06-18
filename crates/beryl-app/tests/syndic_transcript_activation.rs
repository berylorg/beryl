#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(31);

fn view_id(name: &str) -> TranscriptViewId {
    TranscriptViewId(name.to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(SyndicTurnId(format!("turn-{position}"))),
        item_id: Some(SyndicItemId(format!("item-{position}"))),
        projection_id: Some(projection_id.clone()),
        resource_id: None,
        source_range: Some(position..position + 10),
        resource_range: None,
        copy_source_range: Some(position..position + 10),
    }
}

fn view_record(view_id: &TranscriptViewId, position: u64, name: &str) -> TranscriptViewRecord {
    let projection_id = projection_id(name);
    TranscriptViewRecord {
        id: TranscriptViewRecordId(format!("record-{name}")),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantCommentary,
        provenance: provenance(view_id, position, &projection_id),
    }
}

fn text_projection(view_id: &TranscriptViewId, position: u64, name: &str) -> ProjectionRecord {
    let projection_id = projection_id(name);
    ProjectionRecord {
        id: projection_id.clone(),
        revision: REVISION,
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text {
            text: format!("text-{name}"),
        },
        provenance: provenance(view_id, position, &projection_id),
    }
}

fn policy_with_limits(
    view_page_limit: usize,
    max_resident_view_records: usize,
) -> ResidentTranscriptPolicy {
    ResidentTranscriptPolicy {
        view_page_limit,
        max_resident_view_records,
        ..ResidentTranscriptPolicy::default()
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

fn activation_seed(view_id: TranscriptViewId) -> TranscriptActivationSeed {
    TranscriptActivationSeed::new(
        view_id,
        TranscriptActivationSource::Test,
        TranscriptActivationPlacement::Tail,
    )
}

#[test]
fn no_previous_activation_publishes_empty_state_and_seed_request() {
    let mut core = ResidentTranscriptCore::new(policy_with_limits(8, 3));
    let view_id = view_id("next-view");

    let outcome = core.begin_activation(activation_seed(view_id.clone()));

    assert_eq!(outcome.activation_revision, 1);
    assert_eq!(outcome.presentation_revision, 1);
    assert_eq!(outcome.state, ResidentTranscriptSnapshotState::Empty);
    assert!(!outcome.retained_previous_snapshot);
    let request = outcome
        .provider_request
        .expect("view-backed activation should reserve a seed request");
    match request.kind {
        TranscriptProviderRequestKind::ReadViewPage(request) => {
            assert_eq!(request.view_id, view_id);
            assert_eq!(request.anchor, TranscriptPageAnchor::End);
            assert_eq!(request.direction, TranscriptPageDirection::Backward);
            assert_eq!(request.limit, 3);
            assert_eq!(request.observed_revision, None);
        }
        other => panic!("expected activation view-page request, got {other:?}"),
    }

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.generation, ResidentGeneration(1));
    assert!(snapshot.presentation.records.is_empty());
    assert_eq!(
        snapshot.presentation.state,
        ResidentTranscriptSnapshotState::Empty
    );
    assert_eq!(snapshot.provider_requests.pending_count, 1);
}

#[test]
fn unavailable_activation_publishes_unavailable_state_without_provider_request() {
    let mut core = ResidentTranscriptCore::empty();

    let outcome = core.begin_activation(TranscriptActivationSeed::unavailable(
        TranscriptActivationSource::BackendReopen,
        TranscriptActivationPlacement::Tail,
    ));

    assert_eq!(outcome.activation_revision, 1);
    assert_eq!(outcome.presentation_revision, 1);
    assert!(!outcome.retained_previous_snapshot);
    assert!(outcome.provider_request.is_none());
    assert!(matches!(
        outcome.state,
        ResidentTranscriptSnapshotState::Unavailable { .. }
    ));

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.generation, ResidentGeneration(1));
    assert!(snapshot.presentation.records.is_empty());
    assert!(matches!(
        snapshot.presentation.state,
        ResidentTranscriptSnapshotState::Unavailable { .. }
    ));
    assert_eq!(snapshot.provider_requests.pending_count, 0);
}

#[test]
fn prepared_activation_admits_view_and_projection_without_pending_request() {
    let view_id = view_id("prepared-view");
    let mut core = ResidentTranscriptCore::empty();
    let prepared = PreparedTranscriptActivation::new(
        view_id.clone(),
        TranscriptActivationPlacement::Tail,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: view_id.clone(),
            revision: REVISION,
            history_state: TranscriptProviderHistoryState::Complete,
            records: vec![view_record(&view_id, 10, "prepared")],
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        Some(TranscriptProviderResponseKind::ProjectionRecords(
            ProjectionRecordSet {
                view_id: view_id.clone(),
                revision: REVISION,
                records: vec![text_projection(&view_id, 10, "prepared")],
                rejections: Vec::new(),
            },
        )),
    );

    let outcome = core.apply_prepared_activation(prepared, TranscriptActivationSource::Test);

    assert!(outcome.provider_request.is_none());
    assert!(!outcome.retained_previous_snapshot);
    assert!(matches!(
        outcome.state,
        ResidentTranscriptSnapshotState::ProviderBacked { .. }
    ));
    let snapshot = core.core_snapshot();
    assert_eq!(presentation_texts(&snapshot), vec!["text-prepared"]);
    assert_eq!(snapshot.provider_requests.pending_count, 0);
    assert_eq!(snapshot.provider_requests.completed_count, 2);
    assert_eq!(snapshot.resident.view_record_count, 1);
    assert_eq!(snapshot.resident.projection_record_count, 1);
}

#[test]
fn prepared_uncaptured_activation_publishes_incomplete_state_without_pending_request() {
    let view_id = view_id("uncaptured-view");
    let mut core = ResidentTranscriptCore::empty();
    let prepared = PreparedTranscriptActivation::new(
        view_id.clone(),
        TranscriptActivationPlacement::Tail,
        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id,
            revision: REVISION,
            history_state: TranscriptProviderHistoryState::Incomplete {
                reason: TranscriptProviderHistoryReason::NotCaptured,
                detail: Some("no captured Syndic history".to_string()),
            },
            records: Vec::new(),
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        }),
        None,
    );

    let outcome = core.apply_prepared_activation(prepared, TranscriptActivationSource::Test);

    assert!(outcome.provider_request.is_none());
    assert!(!outcome.retained_previous_snapshot);
    assert!(matches!(
        outcome.state,
        ResidentTranscriptSnapshotState::Incomplete { .. }
    ));
    let snapshot = core.core_snapshot();
    assert!(snapshot.presentation.records.is_empty());
    assert_eq!(snapshot.provider_requests.pending_count, 0);
    assert_eq!(snapshot.provider_requests.completed_count, 1);
    assert!(matches!(
        snapshot.presentation.state,
        ResidentTranscriptSnapshotState::Incomplete { .. }
    ));
}

fn seed_text_presentation(names: &[&str]) -> ResidentTranscriptCore {
    let view_id = view_id("current-view");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION).insert_view_records(
        view_id.clone(),
        names
            .iter()
            .enumerate()
            .map(|(index, name)| view_record(&view_id, (index as u64 + 1) * 10, name))
            .collect(),
    );
    for (index, name) in names.iter().enumerate() {
        provider.insert_projection_record(text_projection(&view_id, (index as u64 + 1) * 10, name));
    }

    let mut core = ResidentTranscriptCore::empty();
    let page_request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert!(matches!(
        handle_provider_request(&mut core, &mut provider, page_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { .. }
    ));
    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("seed page should require projection records");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: names.len(),
            rejected_count: 0
        }
    );
    core
}

#[test]
fn previous_coherent_snapshot_is_retained_until_replacement_seed_is_admitted() {
    let mut core = seed_text_presentation(&["a", "b"]);
    let before = core.core_snapshot();
    let previous_activation_revision = before.presentation.activation_revision;
    let previous_presentation_revision = before.presentation.presentation_revision;

    let outcome = core.begin_activation(activation_seed(view_id("replacement-view")));

    assert!(outcome.retained_previous_snapshot);
    assert_eq!(
        outcome.activation_revision,
        previous_activation_revision.saturating_add(1)
    );
    assert_eq!(
        outcome.presentation_revision,
        previous_presentation_revision.saturating_add(1)
    );
    assert!(matches!(
        outcome.state,
        ResidentTranscriptSnapshotState::ProviderBacked { .. }
    ));
    assert!(outcome.provider_request.is_some());

    let snapshot = core.core_snapshot();
    assert_eq!(presentation_texts(&snapshot), vec!["text-a", "text-b"]);
    assert_eq!(snapshot.generation, ResidentGeneration(1));
    assert_eq!(snapshot.provider_requests.pending_count, 1);
    assert_eq!(
        snapshot.presentation.activation_revision,
        outcome.activation_revision
    );
    assert_eq!(
        snapshot.presentation.presentation_revision,
        outcome.presentation_revision
    );
    assert!(
        snapshot
            .presentation
            .records
            .iter()
            .all(|record| record.provenance.presentation_revision == outcome.presentation_revision)
    );
    assert_eq!(snapshot.resident.view_record_count, 2);
    assert_eq!(snapshot.resident.projection_record_count, 2);
}

#[test]
fn stale_renderer_fact_after_activation_does_not_mutate_retained_snapshot() {
    let mut core = seed_text_presentation(&["a", "b"]);
    let stale_revision = core.presentation_snapshot().presentation_revision;
    let outcome = core.begin_activation(activation_seed(view_id("replacement-view")));

    core.push_demand_fact(DemandFact::new(
        stale_revision,
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
            observed: stale_revision,
            current: outcome.presentation_revision,
        }
    );
}

#[test]
fn activation_generation_change_ignores_older_provider_response() {
    let old_view = view_id("old-view");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(old_view.clone(), vec![view_record(&old_view, 10, "old")]);
    let mut core = ResidentTranscriptCore::empty();
    let old_request = core.request_view_page(
        old_view,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::AdjacentRange,
    );

    let outcome = core.begin_activation(activation_seed(view_id("new-view")));
    assert_eq!(outcome.activation_revision, 1);
    assert_eq!(core.provider_request_snapshot().pending_count, 2);

    let old_response = provider
        .handle_request(old_request)
        .expect("fixture provider request should not fail");
    assert_eq!(
        core.handle_provider_response(old_response),
        ResidentProviderResponseEffect::Ignored
    );

    let snapshot = core.core_snapshot();
    assert!(snapshot.resident.view_records.is_empty());
    assert!(snapshot.presentation.records.is_empty());
    assert_eq!(snapshot.provider_requests.pending_count, 1);
    assert_eq!(snapshot.provider_requests.stale_result_count, 1);
    assert_eq!(snapshot.provider_requests.completed_count, 0);
}

#[test]
fn activation_seed_request_respects_start_and_position_placement() {
    let mut core = ResidentTranscriptCore::new(policy_with_limits(4, 16));
    let start = core.begin_activation(TranscriptActivationSeed::new(
        view_id("start-view"),
        TranscriptActivationSource::ThreadSelector,
        TranscriptActivationPlacement::Start,
    ));
    let start_request = start
        .provider_request
        .expect("start activation should reserve a seed request");
    match start_request.kind {
        TranscriptProviderRequestKind::ReadViewPage(request) => {
            assert_eq!(request.anchor, TranscriptPageAnchor::Start);
            assert_eq!(request.direction, TranscriptPageDirection::Forward);
            assert_eq!(request.limit, 4);
        }
        other => panic!("expected start activation view-page request, got {other:?}"),
    }

    let position = TranscriptViewPosition(42);
    let positioned = core.begin_activation(TranscriptActivationSeed::new(
        view_id("position-view"),
        TranscriptActivationSource::ThreadGraph,
        TranscriptActivationPlacement::Position(position),
    ));
    let positioned_request = positioned
        .provider_request
        .expect("position activation should reserve a seed request");
    match positioned_request.kind {
        TranscriptProviderRequestKind::ReadViewPage(request) => {
            assert_eq!(request.anchor, TranscriptPageAnchor::Position(position));
            assert_eq!(request.direction, TranscriptPageDirection::Forward);
            assert_eq!(request.limit, 4);
        }
        other => panic!("expected position activation view-page request, got {other:?}"),
    }
}
