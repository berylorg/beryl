#[path = "support/syndic_transcript_contract.rs"]
mod syndic_transcript_contract;

use tempfile::TempDir;

use syndic_storage as storage;
use syndic_transcript_contract::storage_provider::StorageSyndicTranscriptProvider;
use syndic_transcript_contract::*;

const REVISION: storage::ProviderRevision = storage::ProviderRevision(9);

fn view_id() -> storage::ThreadViewId {
    storage::ThreadViewId::from("thread-view")
}

fn provider_view_id() -> TranscriptViewId {
    TranscriptViewId("thread-view".to_string())
}

fn seed_store(
    build: impl FnOnce(storage::SyndicWriteBatch) -> storage::SyndicWriteBatch,
) -> (TempDir, StorageSyndicTranscriptProvider) {
    let dir = TempDir::new().expect("tempdir should be created");
    {
        let store = storage::SyndicStore::open(dir.path(), storage::StoreOpenOptions::default())
            .expect("store should open");
        store
            .commit(build(storage::SyndicWriteBatch::new()))
            .expect("seed batch should commit");
    }
    let provider =
        StorageSyndicTranscriptProvider::open(dir.path()).expect("provider should open store");
    (dir, provider)
}

fn conversation(
    view_id: &storage::ThreadViewId,
    state: storage::HistoryState,
) -> storage::ConversationRecord {
    storage::ConversationRecord {
        id: storage::ConversationId::from(format!("conversation-{}", view_id)),
        view_id: view_id.clone(),
        parent_view_id: None,
        branch_source_turn_id: None,
        title: None,
        created_at_ms: 1,
        updated_at_ms: 2,
        current_revision: REVISION,
        source: None,
        history_state: state,
    }
}

fn turn_id(position: u64) -> storage::TurnId {
    storage::TurnId::from(format!("turn-{position}"))
}

fn item_id(position: u64) -> storage::ItemId {
    storage::ItemId::from(format!("item-{position}"))
}

fn projection_id(name: &str) -> storage::ProjectionRecordId {
    storage::ProjectionRecordId::from(format!("projection-{name}"))
}

fn resource_id(name: &str) -> storage::ResourceId {
    storage::ResourceId::from(format!("resource-{name}"))
}

fn provenance(
    view_id: &storage::ThreadViewId,
    position: u64,
    projection_id: &storage::ProjectionRecordId,
    resource_id: Option<storage::ResourceId>,
) -> storage::SyndicSourceProvenance {
    storage::SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(storage::TranscriptViewPosition(position)),
        turn_id: Some(turn_id(position)),
        item_id: Some(item_id(position)),
        source_event_id: None,
        projection_id: Some(projection_id.clone()),
        resource_id,
        source_range: Some(storage::ByteRange::new(position, position + 10)),
        resource_range: None,
        copy_source_range: Some(storage::ByteRange::new(position, position + 10)),
    }
}

fn view_record(
    view_id: &storage::ThreadViewId,
    position: u64,
    id: &str,
    projection_id: storage::ProjectionRecordId,
) -> storage::TranscriptViewRecord {
    storage::TranscriptViewRecord {
        id: storage::TranscriptViewRecordId::from(id.to_string()),
        view_id: view_id.clone(),
        position: storage::TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind: storage::TranscriptNarrativeKind::AssistantFinalAnswer,
        provenance: provenance(view_id, position, &projection_id, None),
    }
}

fn text_projection(
    view_id: &storage::ThreadViewId,
    position: u64,
    id: storage::ProjectionRecordId,
    status: storage::ProjectionStatus,
) -> storage::ProjectionRecord {
    storage::ProjectionRecord {
        id: id.clone(),
        view_id: view_id.clone(),
        turn_id: turn_id(position),
        item_id: item_id(position),
        revision: REVISION,
        kind: storage::ProjectionRecordKind::TextChunk,
        status,
        payload: storage::ProjectionPayload::Text {
            text: format!("projection text {position}"),
        },
        provenance: provenance(view_id, position, &id, None),
    }
}

fn resource_record(
    id: storage::ResourceId,
    kind: storage::ResourceKind,
    state: storage::ResourceState,
    bytes: Vec<u8>,
) -> storage::ResourceRecord {
    storage::ResourceRecord {
        metadata: storage::ResourceMetadataRecord {
            id,
            revision: REVISION,
            kind,
            state,
            media_type: Some("application/octet-stream".to_string()),
            byte_len: bytes.len() as u64,
            digest: Some("sha256:test".to_string()),
            line_count: None,
            row_count: None,
            column_count: None,
            preview_range: Some(storage::ByteRange::new(0, bytes.len().min(8) as u64)),
        },
        bytes,
    }
}

fn read(
    provider: &mut StorageSyndicTranscriptProvider,
    request_id: u64,
    kind: TranscriptProviderRequestKind,
) -> TranscriptProviderResponseKind {
    let response = provider
        .handle_request(TranscriptProviderRequest {
            id: ProviderRequestId(request_id),
            kind,
        })
        .expect("storage provider request should not fail");
    assert_eq!(response.request_id, ProviderRequestId(request_id));
    response.kind
}

fn read_page(
    provider: &mut StorageSyndicTranscriptProvider,
    request_id: u64,
    anchor: TranscriptPageAnchor,
    direction: TranscriptPageDirection,
    limit: usize,
    observed_revision: Option<ProviderRevision>,
) -> TranscriptProviderResponseKind {
    read(
        provider,
        request_id,
        TranscriptProviderRequestKind::ReadViewPage(TranscriptViewPageRequest {
            view_id: provider_view_id(),
            anchor,
            direction,
            limit,
            observed_revision,
        }),
    )
}

fn expect_page(kind: TranscriptProviderResponseKind) -> TranscriptViewPage {
    match kind {
        TranscriptProviderResponseKind::ViewPage(page) => page,
        other => panic!("expected view page, got {other:?}"),
    }
}

fn expect_projection_set(kind: TranscriptProviderResponseKind) -> ProjectionRecordSet {
    match kind {
        TranscriptProviderResponseKind::ProjectionRecords(set) => set,
        other => panic!("expected projection record set, got {other:?}"),
    }
}

fn expect_metadata(kind: TranscriptProviderResponseKind) -> ResourceMetadata {
    match kind {
        TranscriptProviderResponseKind::ResourceMetadata(metadata) => metadata,
        other => panic!("expected resource metadata, got {other:?}"),
    }
}

fn expect_range(kind: TranscriptProviderResponseKind) -> ResourceRangeResponse {
    match kind {
        TranscriptProviderResponseKind::ResourceRange(range) => range,
        other => panic!("expected resource range, got {other:?}"),
    }
}

fn expect_rejection(kind: TranscriptProviderResponseKind) -> TranscriptProviderRejection {
    match kind {
        TranscriptProviderResponseKind::Rejected(rejection) => rejection,
        other => panic!("expected rejection, got {other:?}"),
    }
}

fn expect_stale(kind: TranscriptProviderResponseKind) -> TranscriptProviderStale {
    match kind {
        TranscriptProviderResponseKind::Stale(stale) => stale,
        other => panic!("expected stale response, got {other:?}"),
    }
}

#[test]
fn storage_provider_reads_bounded_cursor_pages_from_syndic_store() {
    let view_id = view_id();
    let (_dir, mut provider) = seed_store(|mut batch| {
        batch = batch.put_conversation(conversation(&view_id, storage::HistoryState::Complete));
        for position in [10, 20, 30, 40, 50] {
            let projection_id = projection_id(&position.to_string());
            batch = batch.put_projection(text_projection(
                &view_id,
                position,
                projection_id.clone(),
                storage::ProjectionStatus::Current,
            ));
            batch = batch.put_view_record(view_record(
                &view_id,
                position,
                &format!("record-{position}"),
                projection_id,
            ));
        }
        batch
    });

    let first = expect_page(read_page(
        &mut provider,
        1,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        2,
        None,
    ));
    assert_eq!(first.revision, ProviderRevision(9));
    assert_eq!(
        first.history_state,
        TranscriptProviderHistoryState::Complete
    );
    assert_eq!(first.records.len(), 2);
    assert_eq!(first.records[0].id.0, "record-10");
    assert_eq!(first.records[1].id.0, "record-20");
    assert!(first.at_start);
    assert!(!first.at_end);

    let second = expect_page(read_page(
        &mut provider,
        2,
        TranscriptPageAnchor::Cursor(first.next_cursor.expect("first page should continue")),
        TranscriptPageDirection::Forward,
        2,
        Some(ProviderRevision(9)),
    ));
    assert_eq!(second.records[0].id.0, "record-30");
    assert_eq!(second.records[1].id.0, "record-40");
    assert!(!second.at_start);
    assert!(!second.at_end);
}

#[test]
fn storage_provider_itemizes_projection_records_and_projection_gaps() {
    let view_id = view_id();
    let current_id = projection_id("current");
    let stale_id = projection_id("stale");
    let incomplete_id = projection_id("incomplete");
    let missing_id = ProjectionRecordId("projection-missing".to_string());
    let (_dir, mut provider) = seed_store(|batch| {
        batch
            .put_conversation(conversation(&view_id, storage::HistoryState::Complete))
            .put_projection(text_projection(
                &view_id,
                10,
                current_id.clone(),
                storage::ProjectionStatus::Current,
            ))
            .put_projection(text_projection(
                &view_id,
                20,
                stale_id.clone(),
                storage::ProjectionStatus::Stale {
                    reason: storage::HistoryIncompleteReason::ProjectionStale,
                    detail: Some("projection rebuild pending".to_string()),
                },
            ))
            .put_projection(text_projection(
                &view_id,
                30,
                incomplete_id.clone(),
                storage::ProjectionStatus::Incomplete {
                    reason: storage::HistoryIncompleteReason::MissedEvents,
                    detail: Some("source event gap".to_string()),
                },
            ))
    });

    let set = expect_projection_set(read(
        &mut provider,
        3,
        TranscriptProviderRequestKind::ReadProjectionRecords(ProjectionRecordsRequest {
            view_id: provider_view_id(),
            projection_ids: vec![
                ProjectionRecordId(current_id.to_string()),
                ProjectionRecordId(stale_id.to_string()),
                ProjectionRecordId(incomplete_id.to_string()),
                missing_id.clone(),
            ],
            observed_revision: Some(ProviderRevision(9)),
        }),
    ));

    assert_eq!(set.revision, ProviderRevision(9));
    assert_eq!(set.records.len(), 1);
    assert_eq!(set.records[0].id.0, current_id.to_string());
    assert_eq!(set.rejections.len(), 3);
    assert_eq!(
        set.rejections[0].reason,
        TranscriptProviderRejectionReason::ProjectionStale
    );
    assert_eq!(
        set.rejections[0].message.as_deref(),
        Some("projection rebuild pending")
    );
    assert_eq!(
        set.rejections[1].reason,
        TranscriptProviderRejectionReason::ProjectionIncomplete
    );
    assert_eq!(
        set.rejections[1].message.as_deref(),
        Some("source event gap")
    );
    assert_eq!(
        set.rejections[2].target,
        TranscriptProviderTarget::ProjectionRecord(missing_id)
    );
    assert_eq!(
        set.rejections[2].reason,
        TranscriptProviderRejectionReason::MissingProjectionRecord
    );
}

#[test]
fn storage_provider_stale_revisions_are_provider_responses() {
    let view_id = view_id();
    let (_dir, mut provider) = seed_store(|batch| {
        batch.put_conversation(conversation(&view_id, storage::HistoryState::Complete))
    });

    let stale = expect_stale(read_page(
        &mut provider,
        4,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        4,
        Some(ProviderRevision(8)),
    ));
    assert_eq!(
        stale.target,
        TranscriptProviderTarget::View(provider_view_id())
    );
    assert_eq!(stale.observed_revision, Some(ProviderRevision(8)));
    assert_eq!(stale.current_revision, ProviderRevision(9));
}

#[test]
fn storage_provider_exposes_resource_metadata_missing_state_and_bounded_ranges() {
    let ready_id = resource_id("ready");
    let missing_id = resource_id("missing");
    let ready_bytes = b"0123456789abcdef".to_vec();
    let (_dir, mut provider) = seed_store(|batch| {
        batch
            .put_resource(resource_record(
                ready_id.clone(),
                storage::ResourceKind::Code,
                storage::ResourceState::Ready,
                ready_bytes.clone(),
            ))
            .put_resource(resource_record(
                missing_id.clone(),
                storage::ResourceKind::GeneratedImage,
                storage::ResourceState::Missing {
                    reason: storage::HistoryIncompleteReason::ResourceMissing,
                    detail: Some("generated image payload was not captured".to_string()),
                },
                Vec::new(),
            ))
    });

    let metadata = expect_metadata(read(
        &mut provider,
        5,
        TranscriptProviderRequestKind::ReadResourceMetadata(ResourceMetadataRequest {
            resource_id: ResourceId(ready_id.to_string()),
            observed_revision: Some(ProviderRevision(9)),
        }),
    ));
    assert_eq!(metadata.resource_id, ResourceId(ready_id.to_string()));
    assert_eq!(metadata.revision, ProviderRevision(9));
    assert_eq!(metadata.kind, ResourceKind::Code);
    assert_eq!(metadata.byte_len, ready_bytes.len() as u64);
    assert_eq!(metadata.preview_range, Some(0..8));

    let range = expect_range(read(
        &mut provider,
        6,
        TranscriptProviderRequestKind::ReadResourceRange(ResourceRangeRequest {
            resource_id: ResourceId(ready_id.to_string()),
            range: 2..6,
            observed_revision: Some(ProviderRevision(9)),
        }),
    ));
    assert_eq!(range.bytes, b"2345".to_vec());
    assert_eq!(range.range, 2..6);
    assert!(!range.complete);

    let missing = expect_rejection(read(
        &mut provider,
        7,
        TranscriptProviderRequestKind::ReadResourceMetadata(ResourceMetadataRequest {
            resource_id: ResourceId(missing_id.to_string()),
            observed_revision: Some(ProviderRevision(9)),
        }),
    ));
    assert_eq!(
        missing.reason,
        TranscriptProviderRejectionReason::MissingResource
    );
    assert_eq!(
        missing.message.as_deref(),
        Some("generated image payload was not captured")
    );

    let out_of_bounds = expect_rejection(read(
        &mut provider,
        8,
        TranscriptProviderRequestKind::ReadResourceRange(ResourceRangeRequest {
            resource_id: ResourceId(ready_id.to_string()),
            range: 20..24,
            observed_revision: Some(ProviderRevision(9)),
        }),
    ));
    assert_eq!(
        out_of_bounds.reason,
        TranscriptProviderRejectionReason::RangeOutOfBounds
    );
}

#[test]
fn storage_provider_surfaces_uncaptured_and_stream_lost_history_explicitly() {
    let dir = TempDir::new().expect("tempdir should be created");
    let mut uncaptured_provider = StorageSyndicTranscriptProvider::open(dir.path())
        .expect("provider should open empty store");

    let uncaptured = expect_page(read_page(
        &mut uncaptured_provider,
        9,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        4,
        None,
    ));
    assert!(uncaptured.records.is_empty());
    assert_eq!(uncaptured.revision, ProviderRevision(0));
    assert_eq!(
        uncaptured.history_state,
        TranscriptProviderHistoryState::Incomplete {
            reason: TranscriptProviderHistoryReason::NotCaptured,
            detail: Some("Syndic has no captured transcript history for this view".to_string())
        }
    );

    let view_id = view_id();
    let (_dir, mut stream_lost_provider) = seed_store(|batch| {
        batch.put_conversation(conversation(
            &view_id,
            storage::HistoryState::Incomplete {
                reason: storage::HistoryIncompleteReason::StreamLost,
                detail: Some("foreground stream disconnected".to_string()),
            },
        ))
    });

    let stream_lost = expect_page(read_page(
        &mut stream_lost_provider,
        10,
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        4,
        Some(ProviderRevision(9)),
    ));
    assert!(stream_lost.records.is_empty());
    assert_eq!(
        stream_lost.history_state,
        TranscriptProviderHistoryState::Incomplete {
            reason: TranscriptProviderHistoryReason::StreamLost,
            detail: Some("foreground stream disconnected".to_string())
        }
    );
}
