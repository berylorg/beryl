use super::*;
use beryl_home_store::{
    HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    BerylState, ResolvingIndexFault, ResolvingTransition, ResolvingTransitionStatus,
};

fn fail_child() -> ResolvingTransition {
    ResolvingTransition::ChildInputPending(
        HandoffFailureEvidence::new(HandoffFailureKind::ChildInputPending, None).unwrap(),
    )
}

#[test]
fn resolving_outcomes_require_both_mutation_records_and_preserve_attempt_identity() {
    for transition in [ResolvingTransition::Complete, fail_child()] {
        let directory = tempdir().unwrap();
        let (store, state) = open(directory.path());
        let first = admission(81, 1, 82, "resolving-closure");
        let id = first.job_id();
        admit(&store, &state, first);
        let jobs = state.durable_jobs();
        let old = job(&store, &state, id);
        let prepared = jobs
            .prepare_resolving_transition(&store, id, old.revision(), transition)
            .unwrap();
        let witness = prepared.witness().clone();
        assert_eq!(
            jobs.resolving_transition_status(&store, &witness).unwrap(),
            ResolvingTransitionStatus::ExactOld
        );
        assert!(matches!(
            execute(&store, prepared.contribution()),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert_eq!(
            jobs.resolving_transition_status(&store, &witness).unwrap(),
            ResolvingTransitionStatus::ExactNew
        );
        assert_eq!(
            jobs.request_admission(&store, old.request())
                .unwrap()
                .unwrap()
                .job_id(),
            id
        );
        assert_eq!(
            jobs.latest_attempt(&store, old.discussion_thread_id())
                .unwrap()
                .unwrap()
                .job_id(),
            id
        );
        assert!(matches!(
            execute(
                &store,
                jobs.corrupt_resolving_index_for_test(
                    jobs.revision(&store).unwrap(),
                    old.clone(),
                    ResolvingIndexFault::LiveCopy(old)
                )
            ),
            CommandOutcome::Committed { .. }
        ));
        assert_eq!(
            jobs.resolving_transition_status(&store, &witness).unwrap(),
            ResolvingTransitionStatus::Collision
        );
        store.close().unwrap();
    }
}

#[test]
fn resolving_preparation_rejects_missing_index_closure_and_stale_source() {
    for fault in [
        ResolvingIndexFault::MissingJob,
        ResolvingIndexFault::MissingLive,
        ResolvingIndexFault::MissingRequest,
        ResolvingIndexFault::MissingAttempt,
        ResolvingIndexFault::MissingLatest,
    ] {
        let directory = tempdir().unwrap();
        let (store, state) = open(directory.path());
        let first = admission(83, 1, 84, "missing-resolving-closure");
        let id = first.job_id();
        admit(&store, &state, first);
        let jobs = state.durable_jobs();
        let old = job(&store, &state, id);
        let prepared = jobs
            .prepare_resolving_transition(&store, id, old.revision(), ResolvingTransition::Complete)
            .unwrap();
        assert!(matches!(
            execute(
                &store,
                jobs.corrupt_resolving_index_for_test(
                    jobs.revision(&store).unwrap(),
                    old.clone(),
                    fault
                )
            ),
            CommandOutcome::Committed { .. }
        ));
        assert!(
            jobs.prepare_resolving_transition(
                &store,
                id,
                old.revision(),
                ResolvingTransition::Complete
            )
            .is_err()
        );
        assert!(matches!(
            execute(&store, prepared.contribution()),
            CommandOutcome::NotCommitted { .. }
        ));
        store.close().unwrap();
    }
}

#[test]
fn resolving_candidate_execution_rejects_old_handles_and_foreign_witnesses() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let first = admission(85, 1, 86, "candidate-resolving");
    let id = first.job_id();
    admit(&store, &state, first);
    let old = job(&store, &state, id);
    let stale = state
        .durable_jobs()
        .prepare_resolving_transition(&store, id, old.revision(), fail_child())
        .unwrap();
    let witness = stale.witness().clone();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    let jobs = fresh.durable_jobs();
    assert!(
        state
            .durable_jobs()
            .resolving_transition_status_candidate(&access, &witness)
            .is_err()
    );
    assert_eq!(
        jobs.resolving_transition_status_candidate(&access, &witness)
            .unwrap(),
        ResolvingTransitionStatus::ExactOld
    );
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(stale.contribution()).unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    let prepared = jobs
        .prepare_resolving_transition_candidate(&access, id, old.revision(), fail_child())
        .unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    assert!(matches!(
        access.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        jobs.resolving_transition_status_candidate(&access, &witness)
            .unwrap(),
        ResolvingTransitionStatus::ExactNew
    );
    let store = recovery.publish().unwrap();
    store.close().unwrap();
    let (reopened, state) = open(directory.path());
    assert_eq!(
        state
            .durable_jobs()
            .resolving_transition_status(&reopened, &witness)
            .unwrap(),
        ResolvingTransitionStatus::ExactNew
    );
    reopened.close().unwrap();
    let foreign = tempdir().unwrap();
    let (store, state) = open(foreign.path());
    assert!(
        state
            .durable_jobs()
            .resolving_transition_status(&store, &witness)
            .is_err()
    );
    store.close().unwrap();
}
