#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandError, CursorReadLimits, HomeCommand};
use beryl_model::{
    ProjectionRevision, SyndicDraftId, SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    ConversationParent, CreateThread, CreateThreadError, DraftByThreadRecord, DraftRecord,
    DraftSubmissionIntent, HistorySummaryRecord, SourceEventPayload, SourceEventRecord,
    SourceEventSequence, SyndicMutationError, SyndicPointReadLimit, SyndicStorage,
    ThreadAttributesRecord, ThreadCatalogSourceWitnesses, ThreadCatalogSummaryRecord,
    ThreadCreationStatus, ThreadExecutionRecord, TranscriptGeneration, TurnDepth, TurnEndStatus,
    TurnKind, TurnLifecycle, TurnRecord, TurnStateRevision, TurnTerminalOutcome,
    root_turn_chain_digest,
};

use support::*;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn execute(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    expected_domain_revision: beryl_model::DomainRevision,
    creation: CreateThread,
) -> beryl_home_store::CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(expected_domain_revision, creation))
        .unwrap();
    store.execute(command)
}

fn assert_committed(outcome: beryl_home_store::CommandOutcome) {
    match outcome {
        beryl_home_store::CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => drop(receipt),
        beryl_home_store::CommandOutcome::Committed {
            receipt,
            later_failure: Some(failure),
        } => {
            drop(receipt);
            panic!("expected clean child-thread command, got later failure: {failure:?}")
        }
        beryl_home_store::CommandOutcome::NotCommitted { evidence } => {
            panic!("expected clean child-thread command, got definitive non-commit: {evidence:?}")
        }
        beryl_home_store::CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean child-thread command, got indeterminate outcome: {failure:?}")
        }
    }
}

fn assert_not_committed(outcome: beryl_home_store::CommandOutcome) -> CommandError {
    match outcome {
        beryl_home_store::CommandOutcome::NotCommitted { evidence } => evidence,
        beryl_home_store::CommandOutcome::Committed {
            receipt,
            later_failure,
        } => {
            drop(receipt);
            panic!(
                "expected definitive child-thread rejection, got committed outcome with later failure: {later_failure:?}"
            )
        }
        beryl_home_store::CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!(
                "expected definitive child-thread rejection, got indeterminate outcome: {failure:?}"
            )
        }
    }
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected typed contributor rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn source_history(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    turn_id: SyndicTurnId,
) {
    seed_canonical_empty_thread(store, storage.clone(), thread_id, draft_id);
    let digest = root_turn_chain_digest(turn_id);
    let mut records = vec![
        FixtureRecord::Turn(TurnRecord::new(
            turn_id,
            thread_id,
            TurnKind::OrdinaryUser,
            ConversationParent::Root,
            None,
            TurnDepth::new(1).unwrap(),
            digest,
            timestamp(1),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            turn_id,
            TurnStateRevision::FIRST,
            TurnLifecycle::Interrupted,
            1,
            0,
            timestamp(1),
        )),
        FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn_id,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                ),
            )
            .unwrap(),
        ),
    ];
    records.extend(thread_records(thread_id, draft_id, Some(turn_id), digest));
    records.extend(item_free_transcript_build_records(
        thread_id,
        ThreadRevision::new(1).unwrap(),
        &[(turn_id, digest, TurnLifecycle::Interrupted, 1, timestamp(1))],
    ));
    commit(store, storage.clone(), batch(records));
}

fn publish_source_activity(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
) {
    let thread = storage.thread(store, thread_id, limit()).unwrap().unwrap();
    let current_draft = storage
        .current_draft(store, thread_id, limit())
        .unwrap()
        .unwrap();
    let current_history = storage
        .history_summary(store, thread_id, limit())
        .unwrap()
        .unwrap();
    let draft_revision = current_draft.draft().revision().checked_next().unwrap();
    let history_revision = current_history.revision().checked_next().unwrap();
    let draft = DraftRecord::new(
        current_draft.draft().id(),
        thread_id,
        draft_revision,
        current_draft.draft().submission_intent(),
        current_draft.draft().root_history(),
        current_draft.draft().created_at(),
        timestamp(2),
    );
    let draft_by_thread =
        DraftByThreadRecord::new(thread_id, draft.id(), draft_revision, thread.revision());
    let history = HistorySummaryRecord::new(
        thread_id,
        history_revision,
        thread.revision(),
        thread.committed_tail(),
        thread.selected_path_digest(),
        current_history.complete(),
        timestamp(2),
    );
    let execution = ThreadExecutionRecord::new(thread_id, exact_cas::execution_binding());
    let attributes = ThreadAttributesRecord::ordinary(thread_id);
    let catalog = ThreadCatalogSummaryRecord::new(
        thread_id,
        ProjectionRevision::new(2).unwrap(),
        None,
        execution.execution().clone(),
        attributes.archive(),
        history.last_activity_at(),
        history.complete(),
        thread.parent_thread_id(),
        thread.lineage_depth(),
        thread.lineage_digest(),
        ThreadCatalogSourceWitnesses::new(
            attributes.revision(),
            history.revision(),
            history.thread_revision(),
            history.selected_path_digest(),
            thread.revision(),
        ),
    );
    commit(
        store,
        storage.clone(),
        batch([
            FixtureRecord::Draft(draft),
            FixtureRecord::DraftByThread(draft_by_thread),
            FixtureRecord::HistorySummary(history),
            FixtureRecord::ThreadCatalogSummary(catalog),
        ]),
    );
}

#[test]
fn from_tail_creates_zero_entry_stale_projection_and_reopens_exactly() {
    let home = TestHome::new("phase3-from-tail");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let source_thread = id(1);
    let source_draft = draft_id(2);
    let turn = SyndicTurnId::from_bytes([3; 16]);
    source_history(&store, &storage, source_thread, source_draft, turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        CreateThread::from_tail(
            id(4),
            draft_id(5),
            timestamp(0),
            syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            tail.clone(),
        ),
        Err(CreateThreadError::TimestampPrecedesSourceActivity)
    );
    let creation = CreateThread::from_tail(
        id(4),
        draft_id(5),
        timestamp(5),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        tail,
    )
    .unwrap();
    assert_committed(execute(
        &store,
        &storage,
        storage.revision(&store).unwrap(),
        creation.clone(),
    ));

    let current = storage
        .current_draft(&store, id(4), limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.thread().committed_tail(), Some(turn));
    assert_eq!(
        current.draft().submission_intent(),
        DraftSubmissionIntent::Ordinary
    );
    assert_eq!(current.draft().created_at(), timestamp(5));
    let head = storage
        .transcript_view_head(&store, id(4), limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.entry_count(), 0);
    assert_eq!(head.lifecycle(), syndic_storage::ProjectionLifecycle::Stale);
    let page = storage
        .transcript_entries(
            &store,
            id(4),
            TranscriptGeneration::FIRST,
            None,
            CursorReadLimits::new(1, 1_024).unwrap(),
        )
        .unwrap();
    assert!(page.records().is_empty());
    let summary = storage
        .history_summary(&store, id(4), limit())
        .unwrap()
        .unwrap();
    assert!(!summary.complete());
    assert_eq!(summary.last_activity_at(), timestamp(5));
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .thread_creation_status(&reopened, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Exact
    );
    reopened.close().unwrap();
}

#[test]
fn shared_tail_creation_conflicts_then_retries_without_copying_history() {
    let home = TestHome::new("phase3-shared-tail");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let source_thread = id(10);
    let turn = SyndicTurnId::from_bytes([11; 16]);
    source_history(&store, &storage, source_thread, draft_id(12), turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    let first = CreateThread::from_tail(
        id(13),
        draft_id(14),
        timestamp(2),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        tail.clone(),
    )
    .unwrap();
    let second = CreateThread::from_tail(
        id(15),
        draft_id(16),
        timestamp(2),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        tail,
    )
    .unwrap();
    let shared_revision = storage.revision(&store).unwrap();
    assert_committed(execute(&store, &storage, shared_revision, first));
    let conflict = assert_not_committed(execute(&store, &storage, shared_revision, second.clone()));
    assert!(matches!(conflict, CommandError::Conflict { .. }));
    assert_committed(execute(
        &store,
        &storage,
        storage.revision(&store).unwrap(),
        second,
    ));
    for thread in [id(13), id(15)] {
        let current = storage
            .current_draft(&store, thread, limit())
            .unwrap()
            .unwrap();
        assert_eq!(current.thread().committed_tail(), Some(turn));
        assert_eq!(
            current.draft().submission_intent(),
            DraftSubmissionIntent::Ordinary
        );
    }
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn source_activity_change_invalidates_a_captured_creation_proof() {
    let home = TestHome::new("phase3-stale-tail");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let source_thread = id(20);
    let turn = SyndicTurnId::from_bytes([21; 16]);
    source_history(&store, &storage, source_thread, draft_id(22), turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    let creation = CreateThread::from_tail(
        id(23),
        draft_id(24),
        timestamp(30),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        tail,
    )
    .unwrap();

    publish_source_activity(&store, &storage, source_thread);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let error = assert_not_committed(execute(
        &store,
        &storage,
        storage.revision(&store).unwrap(),
        creation.clone(),
    ));
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceTailConflict
    ));
    assert_eq!(
        storage
            .thread_creation_status(&store, &creation, limit())
            .unwrap(),
        ThreadCreationStatus::Absent
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
