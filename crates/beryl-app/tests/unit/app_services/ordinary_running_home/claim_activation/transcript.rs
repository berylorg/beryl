use crate::main_window::MainWindowShellRoot;
use crate::syndic_transcript::{
    ResidentPresentationRecordKind, ResidentRecordSource, ResidentTranscriptSnapshotState,
    TranscriptNarrativeKind,
};
use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId};
use gpui::{AsyncApp, WindowHandle};
use syndic_storage::{
    InputGateState, ProjectionLifecycle, SourceEventPayload, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp, TurnEndStatus,
};

const TARGET_TEXT: &str = "Recovered ordinary target transcript";

pub(super) fn prepare_history(home: &HomeStore, storage: &SyndicStorage, target: SyndicThreadId) {
    use crate::support::exact_cas::{
        admit_event, converge_and_release_terminal_history, correlate_user_item, establish_turn,
        submit_current_draft,
    };
    let item = SyndicItemId::from_bytes([246; 16]);
    let turn = submit_current_draft(
        home,
        storage.clone(),
        target,
        SyndicDraftId::from_bytes([244; 16]),
        item,
        TARGET_TEXT,
        SyndicTimestamp::from_unix_millis(101),
    );
    let source = establish_turn(
        home,
        storage.clone(),
        target,
        turn,
        SyndicTimestamp::from_unix_millis(102),
    );
    admit_event(
        home,
        storage.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        SyndicTimestamp::from_unix_millis(103),
    );
    correlate_user_item(
        home,
        storage.clone(),
        target,
        turn,
        item,
        &source,
        SyndicTimestamp::from_unix_millis(104),
    );
    admit_event(
        home,
        storage.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        SyndicTimestamp::from_unix_millis(105),
    );
    converge_and_release_terminal_history(home, storage.clone(), target, turn);
    let limit = SyndicPointReadLimit::new(65_536).unwrap();
    let thread = storage.thread(home, target, limit).unwrap().unwrap();
    assert_eq!(thread.committed_tail(), Some(turn));
    let head = storage
        .transcript_view_head(home, target, limit)
        .unwrap()
        .unwrap();
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    assert_eq!(head.entry_count(), 1);
    assert_eq!(head.committed_tail(), Some(turn));
    assert_eq!(head.selected_path_digest(), thread.selected_path_digest());
    assert!(
        storage
            .history_summary(home, target, limit)
            .unwrap()
            .unwrap()
            .complete()
    );
    assert_eq!(
        storage
            .input_gate(home, target, limit)
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    let page = storage
        .transcript_entries(
            home,
            target,
            head.generation(),
            None,
            CursorReadLimits::new(2, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(!page.has_more());
    let entry = &page.records()[0];
    assert_eq!(entry.item_id(), item);
    let projection = storage
        .projection(home, entry.projection_id(), limit)
        .unwrap()
        .unwrap();
    assert_eq!(projection.item_id(), item);
    assert_eq!(projection.turn_id(), turn);
    assert_eq!(projection.revision(), entry.projection_revision());
    assert!(
        matches!(projection.payload(), syndic_storage::ProjectionPayload::InlineMarkdown { source, .. } if source.as_ref() == TARGET_TEXT)
    );
}

pub(super) async fn assert_recovered(window: WindowHandle<MainWindowShellRoot>, cx: &mut AsyncApp) {
    window
        .update(cx, |root, _, app| {
            let claim = root
                .test_thread_confirmation_visible_transcript_claim()
                .expect("recovered target transcript must retain its exact selected claim");
            let target = SyndicThreadId::from_bytes([242; 16]);
            assert_eq!(claim.thread_id(), target);
            let snapshot = root.test_running_transcript_snapshot(app);
            assert_ne!(snapshot.activation_revision, 0);
            assert_ne!(snapshot.presentation_revision, 0);
            assert!(matches!(
                snapshot.state,
                ResidentTranscriptSnapshotState::ProviderBacked { .. }
            ));
            assert_eq!(snapshot.records.len(), 1);
            let record = &snapshot.records[0];
            let ResidentPresentationRecordKind::TextChunk {
                narrative_kind,
                text,
            } = &record.kind
            else {
                panic!("recovered resident transcript did not mount the authored text projection");
            };
            assert_eq!(*narrative_kind, TranscriptNarrativeKind::UserInput);
            assert_eq!(text.as_ref(), TARGET_TEXT);
            let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
                panic!("recovered target transcript used local or copied presentation provenance");
            };
            assert_eq!(source.view_id.0, target.to_string());
            assert_eq!(source.position.as_ref().map(|position| position.0), Some(1));
            assert_eq!(
                source.item_id.as_ref().map(|item| &item.0),
                Some(&SyndicItemId::from_bytes([246; 16]).to_string())
            );
            assert_eq!(
                source.turn_id.as_ref().map(|turn| &turn.0),
                Some(
                    &SyndicDraftId::from_bytes([243; 16])
                        .submitted_turn_id()
                        .to_string()
                )
            );
            assert!(source.projection_id.is_some());
            assert_eq!(source.projection_id, record.provenance.projection_id);
            assert_eq!(source.source_range, Some(0..TARGET_TEXT.len() as u64));
            assert_eq!(
                record.provenance.copy_source_range,
                Some(0..TARGET_TEXT.len() as u64)
            );
            assert_ne!(record.provenance.presentation_revision, 0);
        })
        .unwrap();
}
