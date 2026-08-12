#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CommandError, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{DraftRevision, InputGateRevision, SyndicItemId, ThreadRevision};
use syndic_storage::{
    ComposerAtom, ComposerPayload, CreateThread, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    IdleSubmission, InputAdmissionStatus, PreparedContent, SyndicPointReadLimit, SyndicStorage,
};

use support::{TestHome, draft_id, id, stage_prepared_content, timestamp};

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn execute_one(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean fault-reconciliation fixture command, got {outcome:?}"),
    }
}

fn seed_submission(store: &HomeStore, storage: SyndicStorage) -> IdleSubmission {
    let thread = id(1);
    let draft = draft_id(2);
    execute_one(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let payload = ComposerPayload::new(vec![ComposerAtom::text("durable input").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &content);
    let current = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap();
    let update = match DraftPayloadUpdate::prepare(&current, &content, timestamp(2)).unwrap() {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };
    execute_one(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
    let content = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap()
        .draft()
        .content();
    IdleSubmission::new(
        thread,
        ThreadRevision::new(1).unwrap(),
        draft,
        DraftRevision::new(2).unwrap(),
        content,
        InputGateRevision::new(1).unwrap(),
        draft_id(3),
        SyndicItemId::from_bytes([4; 16]),
        None,
        timestamp(3),
    )
}

#[test]
fn persistence_cuts_reconcile_to_wholly_absent_or_exactly_submitted() {
    for (name, point, expected) in [
        (
            "phase5-admission-before-commit",
            FaultPoint::BeforeCommit,
            InputAdmissionStatus::Absent,
        ),
        (
            "phase5-admission-after-persist",
            FaultPoint::AfterPersist,
            InputAdmissionStatus::ExactSubmitted,
        ),
        (
            "phase5-admission-after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            InputAdmissionStatus::ExactSubmitted,
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let submission = seed_submission(&store, storage);
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.submit_idle_draft(storage.revision(&store).unwrap(), submission.clone()))
            .unwrap();

        faults.fail_next(point);
        let reconciliation_installed = match (point, store.execute(command)) {
            (
                FaultPoint::BeforeCommit,
                beryl_home_store::CommandOutcome::NotCommitted {
                    evidence: CommandError::Commit { .. },
                },
            ) => false,
            (
                FaultPoint::AfterPersist,
                beryl_home_store::CommandOutcome::Committed {
                    later_failure: Some(CommandError::Persistence { .. }),
                    ..
                },
            ) => false,
            (
                FaultPoint::AfterCommitBeforePersist,
                beryl_home_store::CommandOutcome::Indeterminate {
                    failure: CommandError::Persistence { .. },
                    reconciliation,
                    ..
                },
            ) => {
                reconciliation.install();
                true
            }
            (_, outcome) => panic!("unexpected submit fault outcome: {outcome:?}"),
        };
        let (store, storage) = if reconciliation_installed {
            assert_eq!(store.health().state(), HomeHealthState::Healthy);
            (store, storage)
        } else {
            assert_eq!(store.health().state(), HomeHealthState::Failed);
            let recovery = store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            let store = recovery.publish();
            assert_eq!(store.health().state(), HomeHealthState::Healthy);
            (store, storage)
        };
        assert_eq!(
            storage
                .idle_submission_status(&store, &submission, limit())
                .unwrap(),
            expected
        );
        store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        if reconciliation_installed {
            let close_error = store
                .close()
                .expect_err("installed indeterminate custody must block orderly close");
            assert_eq!(close_error.pending_reconciliation_scopes(), Some(1));
            continue;
        }
        store.close().unwrap();

        let mut reopened = open_with_faults(home.path(), FaultController::new());
        let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
        assert_eq!(
            reopened_storage
                .idle_submission_status(&reopened, &submission, limit())
                .unwrap(),
            expected
        );
        reopened
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
        reopened.close().unwrap();
    }
}
