#[path = "support/syndic_transcript_contract.rs"]
mod syndic_transcript_contract;

use syndic_transcript_contract::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_contract::*;

const REVISION: ProviderRevision = ProviderRevision(7);

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

fn read_page(
    provider: &mut InMemorySyndicTranscriptProvider,
    request_id: u64,
    view_id: &TranscriptViewId,
    anchor: TranscriptPageAnchor,
    direction: TranscriptPageDirection,
    limit: usize,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind: TranscriptProviderRequestKind::ReadViewPage(TranscriptViewPageRequest {
                view_id: view_id.clone(),
                anchor,
                direction,
                limit,
                observed_revision,
            }),
        })
        .expect("fixture provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn expect_page(kind: TranscriptProviderResponseKind) -> TranscriptViewPage {
    match kind {
        TranscriptProviderResponseKind::ViewPage(page) => page,
        other => panic!("expected view page, got {other:?}"),
    }
}

fn record_ids(page: &TranscriptViewPage) -> Vec<&str> {
    page.records
        .iter()
        .map(|record| record.id.0.as_str())
        .collect()
}

#[test]
fn forward_cursor_pages_are_bounded_and_stably_ordered() {
    let (mut provider, view_id) = seeded_provider();

    let first = expect_page(read_page(
        &mut provider,
        1,
        &view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        3,
        None,
    ));
    assert_eq!(
        record_ids(&first),
        vec!["record-1", "record-2", "record-3a"]
    );
    assert_eq!(first.revision, REVISION);
    assert_eq!(first.previous_cursor, None);
    assert_eq!(
        first.next_cursor,
        Some(InMemorySyndicTranscriptProvider::cursor_for_offset(3))
    );
    assert!(first.at_start);
    assert!(!first.at_end);

    let second = expect_page(read_page(
        &mut provider,
        2,
        &view_id,
        TranscriptPageAnchor::Cursor(first.next_cursor.expect("first page should continue")),
        TranscriptPageDirection::Forward,
        3,
        None,
    ));
    assert_eq!(record_ids(&second), vec!["record-3b", "record-4"]);
    assert_eq!(
        second.previous_cursor,
        Some(InMemorySyndicTranscriptProvider::cursor_for_offset(3))
    );
    assert_eq!(second.next_cursor, None);
    assert!(!second.at_start);
    assert!(second.at_end);
}

#[test]
fn backward_cursor_pages_are_bounded_and_stably_ordered() {
    let (mut provider, view_id) = seeded_provider();

    let last = expect_page(read_page(
        &mut provider,
        3,
        &view_id,
        TranscriptPageAnchor::End,
        TranscriptPageDirection::Backward,
        2,
        None,
    ));
    assert_eq!(record_ids(&last), vec!["record-3b", "record-4"]);
    assert_eq!(
        last.previous_cursor,
        Some(InMemorySyndicTranscriptProvider::cursor_for_offset(3))
    );
    assert_eq!(last.next_cursor, None);
    assert!(!last.at_start);
    assert!(last.at_end);

    let previous = expect_page(read_page(
        &mut provider,
        4,
        &view_id,
        TranscriptPageAnchor::Cursor(last.previous_cursor.expect("last page should go back")),
        TranscriptPageDirection::Backward,
        3,
        None,
    ));
    assert_eq!(
        record_ids(&previous),
        vec!["record-1", "record-2", "record-3a"]
    );
    assert_eq!(previous.previous_cursor, None);
    assert_eq!(
        previous.next_cursor,
        Some(InMemorySyndicTranscriptProvider::cursor_for_offset(3))
    );
    assert!(previous.at_start);
    assert!(!previous.at_end);
}

#[test]
fn empty_ranges_and_terminal_pages_are_explicit() {
    let (mut provider, view_id) = seeded_provider();

    let empty_limit = expect_page(read_page(
        &mut provider,
        5,
        &view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        0,
        None,
    ));
    assert!(empty_limit.records.is_empty());
    assert_eq!(empty_limit.previous_cursor, None);
    assert_eq!(
        empty_limit.next_cursor,
        Some(InMemorySyndicTranscriptProvider::cursor_for_offset(0))
    );
    assert!(empty_limit.at_start);
    assert!(!empty_limit.at_end);

    let empty_view = TranscriptViewId("empty-view".to_string());
    provider.insert_view_records(empty_view.clone(), Vec::new());
    let terminal = expect_page(read_page(
        &mut provider,
        6,
        &empty_view,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        4,
        None,
    ));
    assert!(terminal.records.is_empty());
    assert_eq!(terminal.previous_cursor, None);
    assert_eq!(terminal.next_cursor, None);
    assert!(terminal.at_start);
    assert!(terminal.at_end);
}

#[test]
fn missing_cursor_rejects_without_synthesizing_page() {
    let (mut provider, view_id) = seeded_provider();
    let missing_cursor = TranscriptCursor("missing-cursor".to_string());

    let rejection = match read_page(
        &mut provider,
        7,
        &view_id,
        TranscriptPageAnchor::Cursor(missing_cursor.clone()),
        TranscriptPageDirection::Forward,
        1,
        None,
    ) {
        TranscriptProviderResponseKind::Rejected(rejection) => rejection,
        other => panic!("expected missing cursor rejection, got {other:?}"),
    };

    assert_eq!(
        rejection.target,
        TranscriptProviderTarget::Cursor(missing_cursor)
    );
    assert_eq!(
        rejection.reason,
        TranscriptProviderRejectionReason::MissingCursor
    );
    assert_eq!(rejection.revision, Some(REVISION));
}

#[test]
fn observed_revision_controls_page_identity() {
    let (mut provider, view_id) = seeded_provider();

    let current = expect_page(read_page(
        &mut provider,
        8,
        &view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        1,
        Some(REVISION),
    ));
    assert_eq!(current.revision, REVISION);

    let stale = match read_page(
        &mut provider,
        9,
        &view_id,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        1,
        Some(ProviderRevision(6)),
    ) {
        TranscriptProviderResponseKind::Stale(stale) => stale,
        other => panic!("expected stale page response, got {other:?}"),
    };
    assert_eq!(stale.target, TranscriptProviderTarget::View(view_id));
    assert_eq!(stale.observed_revision, Some(ProviderRevision(6)));
    assert_eq!(stale.current_revision, REVISION);
}
