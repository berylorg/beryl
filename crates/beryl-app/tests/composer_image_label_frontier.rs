#[path = "../src/shell/composer_image_label_frontier_worker.rs"]
mod composer_image_label_frontier_worker;

use beryl_backend::UserInput;
use serde_json::json;
use syndic_storage::{
    CanonicalItemKind, CanonicalItemRecord, CanonicalItemVisibility, ConversationId,
    ConversationRecord, ExternalSourceMetadata, HistoryIncompleteReason, HistoryState, ItemId,
    ProjectionPayload, ProjectionRecord, ProjectionRecordId, ProjectionRecordKind,
    ProjectionStatus, ProviderRevision, SourceEventId, SourceEventPayload, SourceEventRecord,
    SourceEventVisibility, StoreOpenOptions, SyndicSourceProvenance, SyndicStore, SyndicWriteBatch,
    ThreadViewId, TranscriptNarrativeKind, TranscriptViewPosition, TranscriptViewRecord,
    TranscriptViewRecordId, TurnId, TurnKind, TurnRecord, TurnStatus,
};

fn source() -> ExternalSourceMetadata {
    ExternalSourceMetadata {
        provider: "codex-app-server".to_string(),
        runtime_target: Some("host-windows".to_string()),
        external_thread_id: Some("thread-1".to_string()),
        external_turn_id: Some("turn-1".to_string()),
        external_item_id: None,
        external_event_id: None,
    }
}

fn provenance(
    view_id: &ThreadViewId,
    turn_id: &TurnId,
    item_id: &ItemId,
    event_id: &SourceEventId,
    projection_id: &ProjectionRecordId,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(0)),
        turn_id: Some(turn_id.clone()),
        item_id: Some(item_id.clone()),
        source_event_id: Some(event_id.clone()),
        projection_id: Some(projection_id.clone()),
        resource_id: None,
        source_range: Some(syndic_storage::ByteRange::new(0, 12)),
        resource_range: None,
        copy_source_range: Some(syndic_storage::ByteRange::new(0, 12)),
    }
}

fn seed_store(history_state: HistoryState, backend_input: Vec<UserInput>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir should be created");
    let store =
        SyndicStore::open(dir.path(), StoreOpenOptions::default()).expect("store should open");
    let view_id = ThreadViewId::from("view-1");
    let turn_id = TurnId::from("turn-1");
    let event_id = SourceEventId::from("event-1");
    let item_id = ItemId::from("item-1");
    let projection_id = ProjectionRecordId::from("projection-1");
    let provenance = provenance(&view_id, &turn_id, &item_id, &event_id, &projection_id);

    store
        .commit(
            SyndicWriteBatch::new()
                .put_conversation(ConversationRecord {
                    id: ConversationId::from("conversation-1"),
                    view_id: view_id.clone(),
                    parent_view_id: None,
                    branch_source_turn_id: None,
                    title: Some("captured".to_string()),
                    created_at_ms: 1,
                    updated_at_ms: 2,
                    current_revision: ProviderRevision(1),
                    source: Some(source()),
                    history_state,
                })
                .put_turn(TurnRecord {
                    id: turn_id.clone(),
                    conversation_id: ConversationId::from("conversation-1"),
                    view_id: view_id.clone(),
                    parent_turn_id: None,
                    kind: TurnKind::User,
                    status: TurnStatus::Completed,
                    source: Some(source()),
                    created_at_ms: 1,
                    started_at_ms: Some(1),
                    completed_at_ms: Some(2),
                    terminal_error: None,
                    projection_revision: ProviderRevision(1),
                })
                .put_source_event(SourceEventRecord {
                    id: event_id.clone(),
                    turn_id: turn_id.clone(),
                    sequence: 0,
                    captured_at_ms: 2,
                    source: source(),
                    visibility: SourceEventVisibility::TranscriptVisible,
                    payload: SourceEventPayload {
                        kind: "acceptedUserInput".to_string(),
                        body: json!({
                            "fragmentId": 1,
                            "text": "[A] and [C]",
                            "backendInput": backend_input,
                            "imageMarkerCount": 2,
                        }),
                    },
                })
                .put_item(CanonicalItemRecord {
                    id: item_id.clone(),
                    turn_id: turn_id.clone(),
                    source_event_id: event_id,
                    kind: CanonicalItemKind::UserInput,
                    visibility: CanonicalItemVisibility::Transcript,
                    source: Some(source()),
                    payload: json!({ "text": "[A] and [C]" }),
                })
                .put_projection(ProjectionRecord {
                    id: projection_id.clone(),
                    view_id: view_id.clone(),
                    turn_id: turn_id.clone(),
                    item_id: item_id.clone(),
                    revision: ProviderRevision(1),
                    kind: ProjectionRecordKind::TextChunk,
                    status: ProjectionStatus::Current,
                    payload: ProjectionPayload::Text {
                        text: "[A] and [C]".to_string(),
                    },
                    provenance: provenance.clone(),
                })
                .put_view_record(TranscriptViewRecord {
                    id: TranscriptViewRecordId::from("view-record-1"),
                    view_id,
                    position: TranscriptViewPosition(0),
                    projection_id,
                    narrative_kind: TranscriptNarrativeKind::UserInput,
                    provenance,
                }),
        )
        .expect("records should commit");
    dir
}

#[test]
fn syndic_frontier_scan_discovers_labels_from_captured_backend_input() {
    let dir = seed_store(
        HistoryState::Complete,
        vec![
            UserInput::text("Image A:"),
            UserInput::local_image("a.png"),
            UserInput::text("Image C:"),
            UserInput::local_image("c.png"),
        ],
    );

    let labels = composer_image_label_frontier_worker::scan_composer_image_label_frontier(
        dir.path(),
        "view-1",
    )
    .expect("complete captured history should scan");

    assert_eq!(labels, vec!["A".to_string(), "C".to_string()]);
}

#[test]
fn syndic_frontier_scan_rejects_incomplete_history() {
    let dir = seed_store(
        HistoryState::Incomplete {
            reason: HistoryIncompleteReason::NotCaptured,
            detail: Some("older turns were not captured".to_string()),
        },
        vec![UserInput::text("Image A:"), UserInput::local_image("a.png")],
    );

    let error = composer_image_label_frontier_worker::scan_composer_image_label_frontier(
        dir.path(),
        "view-1",
    )
    .expect_err("incomplete captured history must not unblock image labels");

    assert!(error.contains("older turns were not captured"));
}
