pub use beryl_app::{BerylWorkspacePersistence, WorkspacePersistenceError};

#[path = "../src/shell/syndic_ingestion.rs"]
mod syndic_ingestion;
#[path = "../src/shell/token_usage_snapshot.rs"]
mod token_usage_snapshot;
mod transcript_images {
    use beryl_backend::UserInput;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct TranscriptImageMarker;

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct TranscriptImageMarkerSpec;

    #[derive(Default)]
    pub(super) struct TranscriptImagePathResolver;

    pub(super) struct TranscriptImageParts {
        display_text: String,
    }

    impl TranscriptImageParts {
        pub(super) fn display_text(&self) -> &str {
            self.display_text.as_str()
        }

        pub(super) fn into_image_markers(self) -> Vec<TranscriptImageMarkerSpec> {
            Vec::new()
        }
    }

    pub(super) fn transcript_image_parts_for_backend_records(
        records: &[UserInput],
        _: &TranscriptImagePathResolver,
    ) -> TranscriptImageParts {
        let display_text = records
            .iter()
            .filter_map(|record| match record {
                UserInput::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        TranscriptImageParts { display_text }
    }

    pub(super) fn transcript_image_markers_from_specs(
        _: u64,
        _: Vec<TranscriptImageMarkerSpec>,
    ) -> Vec<TranscriptImageMarker> {
        Vec::new()
    }

    pub(super) fn transcript_image_marker_specs_from_markers(
        _: &[TranscriptImageMarker],
    ) -> Vec<TranscriptImageMarkerSpec> {
        Vec::new()
    }
}
#[path = "../src/shell/turn_input.rs"]
mod turn_input;

use beryl_backend::{AgentMessageItem, ProtocolPhase, TurnInfo, TurnItemsView, TurnStatus};
use beryl_model::workspace::{BerylWorkspaceId, WorkspaceId};
use syndic_ingestion::{
    SyndicLiveTurnIngestor, admit_user_turn, journal_steering_user_fragment,
    mark_steering_user_fragment_redirected, promote_steering_user_fragment,
};
use syndic_storage::{
    HistoryIncompleteReason, HistoryState, StoreOpenOptions, SyndicStore, TranscriptPageAnchor,
    TranscriptPageDirection, TurnStatus as SyndicTurnStatus,
};
use tempfile::TempDir;
use turn_input::UserInputFragment;

fn workspace() -> (
    TempDir,
    BerylWorkspacePersistence,
    BerylWorkspaceId,
    WorkspaceId,
) {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    let persistence = BerylWorkspacePersistence::new(dir.path());
    let workspace_id = BerylWorkspaceId::untitled(1);
    let execution_target = WorkspaceId::host_windows(r"C:\work\captured");
    (dir, persistence, workspace_id, execution_target)
}

fn running_turn(id: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::InProgress,
        items_view: TurnItemsView::Full,
        items: Vec::new(),
        error: None,
    }
}

fn completed_turn(id: &str, item_id: &str, text: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: TurnItemsView::Full,
        items: vec![beryl_backend::ThreadItem::AgentMessage(AgentMessageItem {
            id: item_id.to_string(),
            text: text.to_string(),
            phase: Some(ProtocolPhase::FinalAnswer),
        })],
        error: None,
    }
}

#[test]
fn admitted_turn_binds_live_cas_events_and_reaches_terminal_state() {
    let (_dir, persistence, workspace_id, execution_target) = workspace();
    let fragment = UserInputFragment::text("hello");
    let admission = admit_user_turn(
        &persistence,
        &workspace_id,
        &execution_target,
        None,
        std::slice::from_ref(&fragment),
    )
    .expect("admission should persist");

    let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default())
        .expect("store should reopen after admission");
    assert_eq!(store.list_recovery_markers(8).unwrap().len(), 1);
    drop(store);

    let mut ingestor = SyndicLiveTurnIngestor::new(admission).unwrap();
    ingestor.bind_cas_thread("cas-thread-1").unwrap();
    let started = beryl_backend::TurnStreamEvent::TurnStarted {
        thread_id: "cas-thread-1".to_string(),
        turn: running_turn("cas-turn-1"),
    };
    ingestor.ingest_event(&started).unwrap();
    ingestor.ingest_event(&started).unwrap();
    ingestor
        .ingest_event(&beryl_backend::TurnStreamEvent::AgentMessageDelta {
            thread_id: "cas-thread-1".to_string(),
            turn_id: "cas-turn-1".to_string(),
            item_id: "assistant-1".to_string(),
            delta: "world".to_string(),
        })
        .unwrap();
    ingestor
        .ingest_event(&beryl_backend::TurnStreamEvent::TurnCompleted {
            thread_id: "cas-thread-1".to_string(),
            turn: completed_turn("cas-turn-1", "assistant-1", "world"),
        })
        .unwrap();
    drop(ingestor);

    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default()).unwrap();
    let conversation = store
        .conversation_by_external_thread("codex-app-server", Some("host-windows"), "cas-thread-1")
        .unwrap()
        .expect("CAS thread should resolve to admitted Syndic conversation");
    let page = store
        .read_transcript_page(
            &conversation.view_id,
            TranscriptPageAnchor::Start,
            TranscriptPageDirection::Forward,
            8,
            None,
        )
        .unwrap();
    assert_eq!(page.records.len(), 2);
    let projections = store
        .read_projection_records(
            &conversation.view_id,
            &page
                .records
                .iter()
                .map(|record| record.projection_id.clone())
                .collect::<Vec<_>>(),
            None,
        )
        .unwrap();
    assert!(projections.iter().any(|record| {
        matches!(
            &record.payload,
            syndic_storage::ProjectionPayload::Text { text } if text == "hello"
        )
    }));
    assert!(projections.iter().any(|record| {
        matches!(
            &record.payload,
            syndic_storage::ProjectionPayload::Text { text } if text == "world"
        )
    }));
    for projection in &projections {
        let syndic_storage::ProjectionPayload::Text { text } = &projection.payload else {
            continue;
        };
        let expected_range = Some(syndic_storage::ByteRange::new(0, text.len() as u64));
        assert_eq!(
            projection.provenance.source_range, expected_range,
            "live text projection source range should cover the stored projection text"
        );
        assert_eq!(
            projection.provenance.copy_source_range, expected_range,
            "live text projection copy range should prove resident copy and quote"
        );
    }
    let turn_id = page.records[0].provenance.turn_id.clone().unwrap();
    let turn = store.turn(&turn_id).unwrap().expect("turn should persist");
    assert!(matches!(turn.status, SyndicTurnStatus::Completed));
    assert!(store.list_recovery_markers(8).unwrap().is_empty());
}

#[test]
fn stream_loss_marks_conversation_and_turn_incomplete() {
    let (_dir, persistence, workspace_id, execution_target) = workspace();
    let fragment = UserInputFragment::text("hello");
    let admission = admit_user_turn(
        &persistence,
        &workspace_id,
        &execution_target,
        Some("cas-thread-lost"),
        &[fragment],
    )
    .unwrap();
    let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
    let mut ingestor = SyndicLiveTurnIngestor::new(admission).unwrap();
    ingestor.bind_cas_thread("cas-thread-lost").unwrap();
    ingestor
        .ingest_event(&beryl_backend::TurnStreamEvent::TurnStarted {
            thread_id: "cas-thread-lost".to_string(),
            turn: running_turn("cas-turn-lost"),
        })
        .unwrap();
    ingestor.mark_stream_lost("socket closed").unwrap();
    drop(ingestor);

    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default()).unwrap();
    let conversation = store
        .conversation_by_external_thread(
            "codex-app-server",
            Some("host-windows"),
            "cas-thread-lost",
        )
        .unwrap()
        .expect("conversation should be indexed");
    assert!(matches!(
        conversation.history_state,
        HistoryState::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            ..
        }
    ));
    let marker = store
        .list_recovery_markers(8)
        .unwrap()
        .pop()
        .expect("stream loss should leave a recovery marker");
    let turn = store
        .turn(&marker.turn_id.unwrap())
        .unwrap()
        .expect("incomplete turn should persist");
    assert!(matches!(
        turn.status,
        SyndicTurnStatus::Incomplete {
            reason: HistoryIncompleteReason::StreamLost,
            ..
        }
    ));
}

#[test]
fn steered_fragment_is_journaled_before_transcript_promotion() {
    let (_dir, persistence, workspace_id, execution_target) = workspace();
    let first = UserInputFragment::text("initial");
    let admission = admit_user_turn(
        &persistence,
        &workspace_id,
        &execution_target,
        Some("cas-thread-steer"),
        std::slice::from_ref(&first),
    )
    .unwrap();
    let identity = admission.identity();
    let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
    let steered = UserInputFragment::text("follow up");

    journal_steering_user_fragment(
        &identity,
        &steered,
        "cas-thread-steer",
        Some("cas-turn-steer"),
    )
    .unwrap();
    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default()).unwrap();
    assert_eq!(store.list_recovery_markers(8).unwrap().len(), 2);
    drop(store);

    promote_steering_user_fragment(&identity, &steered, "cas-thread-steer", "cas-turn-steer")
        .unwrap();
    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default()).unwrap();
    assert_eq!(store.list_recovery_markers(8).unwrap().len(), 1);
    let conversation = store
        .conversation_by_external_thread(
            "codex-app-server",
            Some("host-windows"),
            "cas-thread-steer",
        )
        .unwrap()
        .expect("conversation should resolve by CAS thread");
    let page = store
        .read_transcript_page(
            &conversation.view_id,
            TranscriptPageAnchor::Start,
            TranscriptPageDirection::Forward,
            8,
            None,
        )
        .unwrap();
    let projections = store
        .read_projection_records(
            &conversation.view_id,
            &page
                .records
                .iter()
                .map(|record| record.projection_id.clone())
                .collect::<Vec<_>>(),
            None,
        )
        .unwrap();
    assert!(projections.iter().any(|record| {
        matches!(
            &record.payload,
            syndic_storage::ProjectionPayload::Text { text } if text == "initial"
        )
    }));
    assert!(projections.iter().any(|record| {
        matches!(
            &record.payload,
            syndic_storage::ProjectionPayload::Text { text } if text == "follow up"
        )
    }));
    for projection in &projections {
        let syndic_storage::ProjectionPayload::Text { text } = &projection.payload else {
            continue;
        };
        assert_eq!(
            projection.provenance.copy_source_range,
            Some(syndic_storage::ByteRange::new(0, text.len() as u64)),
            "promoted live user fragments should remain resident-copy eligible"
        );
    }
}

#[test]
fn redirected_steering_fragment_does_not_fabricate_transcript_history() {
    let (_dir, persistence, workspace_id, execution_target) = workspace();
    let first = UserInputFragment::text("initial");
    let admission = admit_user_turn(
        &persistence,
        &workspace_id,
        &execution_target,
        Some("cas-thread-redirect"),
        std::slice::from_ref(&first),
    )
    .unwrap();
    let identity = admission.identity();
    let storage_dir = persistence.workspace_syndic_storage_dir(&workspace_id);
    let steered = UserInputFragment::text("next turn instead");

    journal_steering_user_fragment(
        &identity,
        &steered,
        "cas-thread-redirect",
        Some("cas-turn-redirect"),
    )
    .unwrap();
    mark_steering_user_fragment_redirected(
        &identity,
        steered.id,
        "cas-thread-redirect",
        Some("cas-turn-redirect"),
        "not steerable",
    )
    .unwrap();

    let store = SyndicStore::open(&storage_dir, StoreOpenOptions::default()).unwrap();
    assert_eq!(store.list_recovery_markers(8).unwrap().len(), 1);
    let conversation = store
        .conversation_by_external_thread(
            "codex-app-server",
            Some("host-windows"),
            "cas-thread-redirect",
        )
        .unwrap()
        .expect("conversation should resolve by CAS thread");
    let page = store
        .read_transcript_page(
            &conversation.view_id,
            TranscriptPageAnchor::Start,
            TranscriptPageDirection::Forward,
            8,
            None,
        )
        .unwrap();
    assert_eq!(page.records.len(), 1);
    let projections = store
        .read_projection_records(
            &conversation.view_id,
            &[page.records[0].projection_id.clone()],
            None,
        )
        .unwrap();
    assert!(matches!(
        &projections[0].payload,
        syndic_storage::ProjectionPayload::Text { text } if text == "initial"
    ));
    let turn_id = page.records[0].provenance.turn_id.clone().unwrap();
    let source_events = store.read_source_events(&turn_id, 0, 8).unwrap();
    assert!(source_events.records.iter().any(|record| {
        record.payload.kind == "steeringFragmentAccepted"
            && record.visibility == syndic_storage::SourceEventVisibility::Operational
    }));
    assert!(source_events.records.iter().any(|record| {
        record.payload.kind == "steeringFragmentRedirected"
            && record.visibility == syndic_storage::SourceEventVisibility::Operational
    }));
}
