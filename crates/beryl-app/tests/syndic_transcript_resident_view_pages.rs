#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(11);

fn seeded_provider() -> (InMemorySyndicTranscriptProvider, TranscriptViewId) {
    let view_id = TranscriptViewId("thread-view".to_string());
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION).insert_view_records(
        view_id.clone(),
        vec![
            record(&view_id, 40, "record-4"),
            record(&view_id, 30, "record-3b"),
            record(&view_id, 30, "record-3a"),
            record(&view_id, 10, "record-1"),
            record(&view_id, 20, "record-2"),
        ],
    );
    (provider, view_id)
}

fn record(
    view_id: &TranscriptViewId,
    position: u64,
    id: impl Into<String>,
) -> TranscriptViewRecord {
    let id = id.into();
    let position = TranscriptViewPosition(position);
    let projection_id = ProjectionRecordId(format!("projection-{id}"));
    TranscriptViewRecord {
        id: TranscriptViewRecordId(id.clone()),
        position,
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantCommentary,
        provenance: SyndicSourceProvenance {
            view_id: view_id.clone(),
            position: Some(position),
            turn_id: Some(SyndicTurnId(format!("turn-{id}"))),
            item_id: Some(SyndicItemId(format!("item-{id}"))),
            projection_id: Some(projection_id),
            resource_id: None,
            source_range: Some(position.0..position.0 + 1),
            resource_range: None,
            copy_source_range: Some(position.0..position.0 + 1),
        },
    }
}

fn policy_with_page_limit(limit: usize) -> ResidentTranscriptPolicy {
    ResidentTranscriptPolicy {
        view_page_limit: limit,
        max_resident_view_records: 16,
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

fn resident_record_ids(snapshot: &ResidentCoreSnapshot) -> Vec<&str> {
    snapshot
        .resident
        .view_records
        .iter()
        .map(|record| record.id.0.as_str())
        .collect()
}

#[test]
fn page_demand_facts_and_requests_are_bounded() {
    let mut core = ResidentTranscriptCore::new(policy_with_page_limit(2));
    let view_id = TranscriptViewId("thread-view".to_string());

    core.push_demand_fact(DemandFact::new(
        0,
        DemandFactKind::AdjacentRange {
            anchor_index: 0,
            direction: TranscriptPageDirection::Forward,
        },
    ));

    let request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::AdjacentRange,
    );

    match request.kind {
        TranscriptProviderRequestKind::ReadViewPage(page_request) => {
            assert_eq!(page_request.view_id, view_id);
            assert_eq!(page_request.anchor, TranscriptPageAnchor::Start);
            assert_eq!(page_request.direction, TranscriptPageDirection::Forward);
            assert_eq!(page_request.limit, 2);
            assert_eq!(page_request.observed_revision, None);
        }
        other => panic!("expected view page request, got {other:?}"),
    }

    assert_eq!(core.demand_fact_snapshot().pending_count, 1);
    assert_eq!(core.provider_request_snapshot().pending_count, 1);
    let drained = core.drain_demand_facts();
    assert_eq!(drained.len(), 1);
}

#[test]
fn admitted_pages_become_ordered_resident_records() {
    let (mut provider, view_id) = seeded_provider();
    let mut core = ResidentTranscriptCore::new(policy_with_page_limit(3));

    let first_request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, first_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 3 }
    );

    let first_snapshot = core.core_snapshot();
    assert_eq!(
        resident_record_ids(&first_snapshot),
        vec!["record-1", "record-2", "record-3a"]
    );
    assert_eq!(first_snapshot.resident.view_id, Some(view_id.clone()));
    assert_eq!(first_snapshot.resident.provider_revision, Some(REVISION));
    assert!(first_snapshot.resident.at_start);
    assert!(!first_snapshot.resident.at_end);
    assert_eq!(
        first_snapshot.resident.next_cursor,
        Some(InMemorySyndicTranscriptProvider::cursor_for_offset(3))
    );
    assert!(first_snapshot.presentation.records.is_empty());

    let next_cursor = first_snapshot
        .resident
        .next_cursor
        .expect("first resident page should continue");
    let second_request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::Cursor(next_cursor),
        TranscriptPageDirection::Forward,
        ProviderRequestReason::AdjacentRange,
    );
    match &second_request.kind {
        TranscriptProviderRequestKind::ReadViewPage(page_request) => {
            assert_eq!(page_request.observed_revision, Some(REVISION));
        }
        other => panic!("expected view page request, got {other:?}"),
    }
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, second_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 2 }
    );

    let second_snapshot = core.core_snapshot();
    assert_eq!(
        resident_record_ids(&second_snapshot),
        vec!["record-1", "record-2", "record-3a", "record-3b", "record-4"]
    );
    assert!(second_snapshot.resident.at_start);
    assert!(second_snapshot.resident.at_end);
    assert_eq!(second_snapshot.resident.previous_cursor, None);
    assert_eq!(second_snapshot.resident.next_cursor, None);
    assert!(second_snapshot.presentation.records.is_empty());
}

#[test]
fn stale_page_results_do_not_create_presentation_content() {
    let (mut provider, view_id) = seeded_provider();
    let mut core = ResidentTranscriptCore::new(policy_with_page_limit(1));

    let first_request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, first_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 1 }
    );
    provider.advance_revision();

    let stale_request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::End,
        TranscriptPageDirection::Backward,
        ProviderRequestReason::AdjacentRange,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, stale_request),
        ResidentProviderResponseEffect::Stale
    );

    let snapshot = core.core_snapshot();
    assert_eq!(resident_record_ids(&snapshot), vec!["record-1"]);
    assert_eq!(snapshot.provider_requests.pending_count, 0);
    assert_eq!(snapshot.provider_requests.stale_result_count, 1);
    assert_eq!(snapshot.provider_requests.rejected_result_count, 0);
    assert!(snapshot.presentation.records.is_empty());
}

#[test]
fn missing_cursor_rejection_stays_out_of_resident_content() {
    let (mut provider, view_id) = seeded_provider();
    let mut core = ResidentTranscriptCore::new(policy_with_page_limit(2));

    let request = core.request_view_page(
        view_id,
        TranscriptPageAnchor::Cursor(TranscriptCursor("missing-cursor".to_string())),
        TranscriptPageDirection::Forward,
        ProviderRequestReason::AdjacentRange,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, request),
        ResidentProviderResponseEffect::Rejected
    );

    let snapshot = core.core_snapshot();
    assert!(snapshot.resident.view_records.is_empty());
    assert_eq!(snapshot.resident.view_record_count, 0);
    assert_eq!(snapshot.provider_requests.pending_count, 0);
    assert_eq!(snapshot.provider_requests.rejected_result_count, 1);
    assert_eq!(snapshot.provider_requests.stale_result_count, 0);
    assert!(snapshot.presentation.records.is_empty());
}

#[test]
fn terminal_page_state_is_resident_state_not_presentation_content() {
    let view_id = TranscriptViewId("short-view".to_string());
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION).insert_view_records(
        view_id.clone(),
        vec![
            record(&view_id, 20, "record-2"),
            record(&view_id, 10, "record-1"),
        ],
    );
    let mut core = ResidentTranscriptCore::new(policy_with_page_limit(8));

    let request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 2 }
    );

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.view_id, Some(view_id));
    assert_eq!(resident_record_ids(&snapshot), vec!["record-1", "record-2"]);
    assert_eq!(snapshot.resident.previous_cursor, None);
    assert_eq!(snapshot.resident.next_cursor, None);
    assert!(snapshot.resident.at_start);
    assert!(snapshot.resident.at_end);
    assert!(snapshot.presentation.records.is_empty());
}
