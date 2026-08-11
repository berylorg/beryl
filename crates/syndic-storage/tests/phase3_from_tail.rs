#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CommandError, CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{
    BindingRevision, DraftRevision, ProjectionRevision, SyndicDraftId, SyndicItemId,
    SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::{
    BindingHeadRecord, BindingRecord, BindingState, ComposerAtom, ComposerPayload,
    ConversationParent, CreateThread, CreateThreadError, DraftByThreadRecord, DraftPayloadUpdate,
    DraftPayloadUpdateDecision, DraftSubmissionIntent, HistorySummaryRecord, IdleSubmission,
    PreparedContent, SelectedPathProof, SourceEventPayload, SourceEventRecord, SourceEventSequence,
    SyndicMutationError, SyndicPointReadLimit, SyndicStorage, ThreadCreationStatus,
    ThreadLineageProof, ThreadRecord, TranscriptGeneration, TurnDepth, TurnEndStatus, TurnKind,
    TurnLifecycle, TurnRecord, TurnStateRevision, TurnTerminalOutcome, empty_selected_path_digest,
    root_turn_chain_digest,
};

use support::*;

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn payload(text: &str) -> ComposerPayload {
    ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap()
}

fn execute(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
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
            later_failure: None,
            ..
        } => {}
        outcome => panic!("unexpected child-thread command outcome: {outcome:?}"),
    }
}

fn source_history(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    turn_id: SyndicTurnId,
) {
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
    commit(store, storage, batch(records));
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected typed contributor rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

#[test]
fn from_tail_creates_zero_entry_stale_projection_and_reopens_exactly() {
    let home = TestHome::new("phase3-from-tail");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let source_thread = id(1);
    let source_draft = draft_id(2);
    let turn = SyndicTurnId::from_bytes([3; 16]);
    source_history(&store, storage, source_thread, source_draft, turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        CreateThread::from_tail(id(4), draft_id(5), timestamp(0), tail.clone()),
        Err(CreateThreadError::TimestampPrecedesSourceActivity)
    );
    let creation = CreateThread::from_tail(id(4), draft_id(5), timestamp(5), tail).unwrap();
    execute(
        &store,
        storage,
        storage.revision(&store).unwrap(),
        creation.clone(),
    )
    .unwrap();

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
    store.validate_registered_domains().unwrap();
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
    source_history(&store, storage, source_thread, draft_id(12), turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    let first = CreateThread::from_tail(id(13), draft_id(14), timestamp(2), tail.clone()).unwrap();
    let second = CreateThread::from_tail(id(15), draft_id(16), timestamp(2), tail).unwrap();
    let shared_revision = storage.revision(&store).unwrap();
    execute(&store, storage, shared_revision, first).unwrap();
    let conflict = execute(&store, storage, shared_revision, second.clone()).unwrap_err();
    assert!(matches!(conflict, CommandError::Conflict { .. }));
    execute(&store, storage, storage.revision(&store).unwrap(), second).unwrap();
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
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn from_tail_ordinary_submission_parents_to_the_current_tail() {
    let home = TestHome::new("phase3-from-tail-submission");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let source_thread = id(17);
    let source_draft = draft_id(18);
    let source_turn = SyndicTurnId::from_bytes([19; 16]);
    source_history(&store, storage, source_thread, source_draft, source_turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    let child_thread = id(20);
    let child_draft = draft_id(21);
    execute(
        &store,
        storage,
        storage.revision(&store).unwrap(),
        CreateThread::from_tail(child_thread, child_draft, timestamp(5), tail).unwrap(),
    )
    .unwrap();

    let content = PreparedContent::composer(&payload("submitted from shared tail")).unwrap();
    stage_prepared_content(&store, storage, &content);
    let current = storage
        .current_draft(&store, child_thread, limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(6)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    let mut save = HomeCommand::new(store.home_revision().unwrap());
    save.add(storage.update_draft_payload(storage.revision(&store).unwrap(), update))
        .unwrap();
    match store.execute(save) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean child draft save, got {outcome:?}"),
    }

    let current = storage
        .current_draft(&store, child_thread, limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, child_thread, limit())
        .unwrap()
        .unwrap();
    let submission = IdleSubmission::new(
        child_thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        draft_id(22),
        SyndicItemId::from_bytes([23; 16]),
        None,
        timestamp(7),
    );
    let submitted_turn = submission.submitted_turn_id();
    let mut submit = HomeCommand::new(store.home_revision().unwrap());
    submit
        .add(storage.submit_idle_draft(storage.revision(&store).unwrap(), submission))
        .unwrap();
    match store.execute(submit) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean child draft submission, got {outcome:?}"),
    }

    let submitted = storage
        .turn(&store, submitted_turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(submitted.parent(), ConversationParent::Turn(source_turn));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn source_activity_change_invalidates_a_captured_creation_proof() {
    let home = TestHome::new("phase3-stale-tail");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let source_thread = id(20);
    let turn = SyndicTurnId::from_bytes([21; 16]);
    source_history(&store, storage, source_thread, draft_id(22), turn);
    let tail = storage
        .thread_tail(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    let creation = CreateThread::from_tail(id(23), draft_id(24), timestamp(30), tail).unwrap();
    let current = storage
        .current_draft(&store, source_thread, limit())
        .unwrap()
        .unwrap();
    let content = PreparedContent::composer(&payload("changed")).unwrap();
    stage_prepared_content(&store, storage, &content);
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(20)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    let mut save = HomeCommand::new(store.home_revision().unwrap());
    save.add(storage.update_draft_payload(storage.revision(&store).unwrap(), update))
        .unwrap();
    match store.execute(save) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean conflicting draft save, got {outcome:?}"),
    }

    let error = execute(&store, storage, storage.revision(&store).unwrap(), creation).unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceTailConflict
    ));
    assert!(storage.thread(&store, id(23), limit()).unwrap().is_none());
    store.close().unwrap();
}

#[test]
fn draft_update_survives_a_same_draft_thread_revision_advance() {
    let home = TestHome::new("phase3-thread-revision");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread_id = id(30);
    let draft_id = draft_id(31);
    let creation = CreateThread::ordinary(
        thread_id,
        draft_id,
        exact_cas::execution_binding(),
        timestamp(1),
    );
    execute(&store, storage, storage.revision(&store).unwrap(), creation).unwrap();
    let current = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    let content = PreparedContent::composer(&payload("stale")).unwrap();
    stage_prepared_content(&store, storage, &content);
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    advance_empty_thread_revision(&store, storage, thread_id, draft_id);
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.update_draft_payload(storage.revision(&store).unwrap(), update.clone()))
        .unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean later draft save, got {outcome:?}"),
    }

    let committed = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        committed.thread().revision(),
        ThreadRevision::new(2).unwrap()
    );
    assert!(update.matches_committed(&committed));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

fn advance_empty_thread_revision(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
) {
    let thread_revision = ThreadRevision::new(2).unwrap();
    let draft_revision = DraftRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(2).unwrap();
    let digest = empty_selected_path_digest();
    let selected = SelectedPathProof::new(None, thread_revision, digest);
    let mut fixture = FixtureBatch::new();
    fixture
        .put(FixtureRecord::Thread(ThreadRecord::new(
            thread_id,
            selected,
            draft_id,
            ThreadLineageProof::new(
                None,
                None,
                syndic_storage::ThreadLineageDepth::FIRST,
                syndic_storage::root_thread_lineage_digest(thread_id),
            ),
            syndic_storage::ThreadImageLabelFrontiers::empty(),
            None,
        )))
        .unwrap();
    fixture
        .put(FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread_id,
            draft_id,
            draft_revision,
            thread_revision,
        )))
        .unwrap();
    fixture
        .put(FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread_id,
            ProjectionRevision::new(2).unwrap(),
            thread_revision,
            None,
            digest,
            true,
            timestamp(1),
        )))
        .unwrap();
    fixture
        .put(FixtureRecord::Binding(BindingRecord::new(
            thread_id,
            binding_revision,
            selected,
            BindingState::unbound("revision test").unwrap(),
        )))
        .unwrap();
    fixture
        .put(FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread_id,
            binding_revision,
            syndic_storage::BindingLifecycle::Unbound,
            digest,
        )))
        .unwrap();
    commit(store, storage, fixture);
}

#[test]
fn surfaced_post_persist_failure_reconciles_the_whole_new_draft() {
    let home = TestHome::new("phase3-post-persist");
    let faults = FaultController::new();
    let mut store = beryl_home_store::HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread_id = id(40);
    let creation = CreateThread::ordinary(
        thread_id,
        draft_id(41),
        exact_cas::execution_binding(),
        timestamp(1),
    );
    execute(&store, storage, storage.revision(&store).unwrap(), creation).unwrap();
    let current = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    let content = PreparedContent::composer(&payload("durable")).unwrap();
    stage_prepared_content(&store, storage, &content);
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.update_draft_payload(storage.revision(&store).unwrap(), update.clone()))
        .unwrap();
    faults.fail_next(FaultPoint::AfterPersist);
    assert!(matches!(
        beryl_home_store::CommandOutcome::Committed {
            receipt: _,
            later_failure: Some(CommandError::Persistence { .. })
        }
    ));
    assert_eq!(store.health().state(), HomeHealthState::Verifying);
    store.verify_health().unwrap();
    let reconciled = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert!(update.matches_committed(&reconciled));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
