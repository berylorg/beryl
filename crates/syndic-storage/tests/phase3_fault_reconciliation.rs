#![cfg(feature = "test-faults")]

mod support;

use std::{sync::Arc, thread, time::Duration};

use beryl_home_store::{
    CommandError, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::{
    ComposerAtom, ComposerPayload, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    PreparedContent, SyndicPointReadLimit, SyndicReadError, SyndicStorage, ThreadCreationStatus,
};

use support::{TestHome, draft_id, id, stage_prepared_content, timestamp};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn create_command(
    store: &HomeStore,
    storage: SyndicStorage,
    creation: CreateThread,
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.create_thread(storage.revision(store).unwrap(), creation))
        .unwrap();
    command
}

#[test]
fn creation_faults_reconcile_to_whole_old_or_whole_new_state() {
    for (name, point, expected) in [
        (
            "creation-before-commit",
            FaultPoint::BeforeCommit,
            ThreadCreationStatus::Absent,
        ),
        (
            "creation-after-persist",
            FaultPoint::AfterPersist,
            ThreadCreationStatus::Exact,
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let creation = CreateThread::ordinary(
            id(1),
            draft_id(2),
            support::exact_cas::execution_binding(),
            timestamp(1),
        );
        let command = create_command(&store, storage, creation.clone());

        faults.fail_next(point);
        match (point, store.execute(command)) {
            (
                FaultPoint::BeforeCommit,
                beryl_home_store::CommandOutcome::NotCommitted {
                    evidence: CommandError::Commit { .. },
                },
            )
            | (
                FaultPoint::AfterPersist,
                beryl_home_store::CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                },
            ) => {}
            (_, outcome) => panic!("unexpected creation fault outcome: {outcome:?}"),
        }
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        let recovery = store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        let store = recovery.publish();
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        assert_eq!(
            storage
                .thread_creation_status(&store, &creation, limit())
                .unwrap(),
            expected
        );
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        store.close().unwrap();
    }
}

#[test]
fn current_draft_read_rejects_a_revision_published_between_its_index_reads() {
    let home = TestHome::new("current-draft-race");
    let faults = FaultController::new();
    let mut store = open_with_faults(home.path(), faults.clone());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread_id = id(10);
    let creation = CreateThread::ordinary(
        thread_id,
        draft_id(11),
        support::exact_cas::execution_binding(),
        timestamp(1),
    );
    match store.execute(create_command(&store, storage, creation)) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean creation command, got {outcome:?}"),
    }
    let current = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    let payload = ComposerPayload::new(vec![ComposerAtom::text("new").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&store, storage, &content);
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };

    let block = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let store = Arc::new(store);
    let reader_store = Arc::clone(&store);
    let reader = thread::spawn(move || storage.current_draft(&reader_store, thread_id, limit()));
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.update_draft_payload(storage.revision(&store).unwrap(), update.clone()))
        .unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean payload update command, got {outcome:?}"),
    }
    block.release();

    assert!(matches!(
        reader.join().unwrap(),
        Err(SyndicReadError::ConcurrentChange {
            operation: "current-draft read"
        })
    ));
    let committed = storage
        .current_draft(&store, thread_id, limit())
        .unwrap()
        .unwrap();
    assert!(update.matches_committed(&committed));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let store = match Arc::try_unwrap(store) {
        Ok(store) => store,
        Err(_) => panic!("reader retained the Beryl home"),
    };
    store.close().unwrap();
}
