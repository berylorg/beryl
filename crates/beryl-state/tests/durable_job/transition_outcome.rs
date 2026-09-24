use super::*;
use beryl_home_store::{
    HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    BerylState, HandoffJobIndexFault, HandoffJobTransition, HandoffJobTransitionStatus,
};

fn fail_child() -> HandoffJobTransition {
    HandoffJobTransition::ChildInputPending(
        HandoffFailureEvidence::new(HandoffFailureKind::ChildInputPending, None).unwrap(),
    )
}

fn fail_parent() -> HandoffJobTransition {
    HandoffJobTransition::ParentArchived(
        HandoffFailureEvidence::new(HandoffFailureKind::ParentArchived, None).unwrap(),
    )
}

fn parent_identity() -> ParentHandoffIdentity {
    ParentHandoffIdentity::new(
        SyndicAcceptedInputId::from_bytes([91; 16]),
        SyndicTurnId::from_bytes([92; 16]),
    )
}

#[derive(Clone, Copy)]
enum Checkpoint {
    Resolving,
    Waiting,
    Starting,
    Active,
}

fn cas_identity() -> ParentCasIdentity {
    ParentCasIdentity::new(
        CasThreadId::new("parent-thread").unwrap(),
        CasTurnId::new("parent-turn").unwrap(),
    )
}

fn failure(kind: HandoffFailureKind) -> HandoffFailureEvidence {
    HandoffFailureEvidence::new(kind, None).unwrap()
}

fn transitions() -> [(Checkpoint, HandoffJobTransition); 12] {
    use Checkpoint::*;
    [
        (Resolving, HandoffJobTransition::CompleteResolving),
        (Resolving, fail_child()),
        (Resolving, fail_parent()),
        (Waiting, fail_parent()),
        (
            Waiting,
            HandoffJobTransition::StartParent(parent_identity()),
        ),
        (
            Starting,
            HandoffJobTransition::ParentAccepted(cas_identity()),
        ),
        (
            Starting,
            HandoffJobTransition::RetryableFailure(failure(
                HandoffFailureKind::CasRejectedBeforeAcceptance,
            )),
        ),
        (
            Starting,
            HandoffJobTransition::TerminalFailure(failure(
                HandoffFailureKind::UnrecoverablePostAppend,
            )),
        ),
        (
            Active,
            HandoffJobTransition::TerminalFailure(failure(HandoffFailureKind::ParentInterrupted)),
        ),
        (
            Active,
            HandoffJobTransition::TerminalFailure(failure(HandoffFailureKind::ParentIncomplete)),
        ),
        (
            Active,
            HandoffJobTransition::TerminalFailure(failure(
                HandoffFailureKind::ParentTerminalFailure,
            )),
        ),
        (Active, HandoffJobTransition::Succeed),
    ]
}

fn advance_to_checkpoint(store: &HomeStore, state: &BerylState, id: JobId, checkpoint: Checkpoint) {
    let count = match checkpoint {
        Checkpoint::Resolving => 0,
        Checkpoint::Waiting => 1,
        Checkpoint::Starting => 2,
        Checkpoint::Active => 3,
    };
    for transition in [
        HandoffJobTransition::CompleteResolving,
        HandoffJobTransition::StartParent(parent_identity()),
        HandoffJobTransition::ParentAccepted(cas_identity()),
    ]
    .into_iter()
    .take(count)
    {
        let jobs = state.durable_jobs();
        let prepared = jobs
            .prepare_handoff_job_transition(store, id, job(store, state, id).revision(), transition)
            .unwrap();
        assert!(matches!(
            execute(store, prepared.contribution()),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
    }
}

#[test]
fn handoff_outcomes_require_both_mutation_records_and_preserve_attempt_identity() {
    for (waiting, transition) in transitions() {
        let directory = tempdir().unwrap();
        let (store, state) = open(directory.path());
        let first = admission(81, 1, 82, "resolving-closure");
        let id = first.job_id();
        admit(&store, &state, first);
        advance_to_checkpoint(&store, &state, id, waiting);
        let jobs = state.durable_jobs();
        let old = job(&store, &state, id);
        let prepared = jobs
            .prepare_handoff_job_transition(&store, id, old.revision(), transition)
            .unwrap();
        let witness = prepared.witness().clone();
        assert_eq!(
            witness.old_job().resolution(),
            witness.new_job().resolution()
        );
        assert_eq!(witness.old_job().request(), witness.new_job().request());
        if witness.new_job().state().parent().is_some() {
            assert_eq!(witness.new_job().state().parent(), Some(parent_identity()));
        }
        assert_eq!(
            jobs.handoff_job_transition_status(&store, &witness)
                .unwrap(),
            HandoffJobTransitionStatus::ExactOld
        );
        assert!(matches!(
            execute(&store, prepared.contribution()),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert_eq!(
            jobs.handoff_job_transition_status(&store, &witness)
                .unwrap(),
            HandoffJobTransitionStatus::ExactNew
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
                jobs.corrupt_handoff_job_index_for_test(
                    jobs.revision(&store).unwrap(),
                    old.clone(),
                    HandoffJobIndexFault::LiveCopy(old)
                )
            ),
            CommandOutcome::Committed { .. }
        ));
        assert_eq!(
            jobs.handoff_job_transition_status(&store, &witness)
                .unwrap(),
            HandoffJobTransitionStatus::Collision
        );
        store.close().unwrap();
    }
}

#[test]
fn handoff_preparation_rejects_missing_index_closure_and_stale_source() {
    for (waiting, transition) in transitions() {
        for fault in [
            HandoffJobIndexFault::MissingJob,
            HandoffJobIndexFault::MissingLive,
            HandoffJobIndexFault::MissingRequest,
            HandoffJobIndexFault::MissingAttempt,
            HandoffJobIndexFault::MissingLatest,
        ] {
            let directory = tempdir().unwrap();
            let (store, state) = open(directory.path());
            let first = admission(83, 1, 84, "missing-resolving-closure");
            let id = first.job_id();
            admit(&store, &state, first);
            advance_to_checkpoint(&store, &state, id, waiting);
            let jobs = state.durable_jobs();
            let old = job(&store, &state, id);
            let prepared = jobs
                .prepare_handoff_job_transition(&store, id, old.revision(), transition.clone())
                .unwrap();
            assert!(matches!(
                execute(
                    &store,
                    jobs.corrupt_handoff_job_index_for_test(
                        jobs.revision(&store).unwrap(),
                        old.clone(),
                        fault
                    )
                ),
                CommandOutcome::Committed { .. }
            ));
            assert!(
                jobs.prepare_handoff_job_transition(&store, id, old.revision(), transition.clone())
                    .is_err()
            );
            assert!(matches!(
                execute(&store, prepared.contribution()),
                CommandOutcome::NotCommitted { .. }
            ));
            store.close().unwrap();
        }
    }
}

#[test]
fn handoff_candidate_execution_rejects_old_handles_and_foreign_witnesses() {
    for (waiting, transition) in transitions() {
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
        advance_to_checkpoint(&store, &state, id, waiting);
        let old = job(&store, &state, id);
        let stale = state
            .durable_jobs()
            .prepare_handoff_job_transition(&store, id, old.revision(), transition.clone())
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
                .handoff_job_transition_status_candidate(&access, &witness)
                .is_err()
        );
        assert_eq!(
            jobs.handoff_job_transition_status_candidate(&access, &witness)
                .unwrap(),
            HandoffJobTransitionStatus::ExactOld
        );
        let mut command = HomeCommand::new(access.home_revision().unwrap());
        command.add(stale.contribution()).unwrap();
        assert!(matches!(
            access.execute(command),
            CommandOutcome::NotCommitted { .. }
        ));
        let prepared = jobs
            .prepare_handoff_job_transition_candidate(&access, id, old.revision(), transition)
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
            jobs.handoff_job_transition_status_candidate(&access, &witness)
                .unwrap(),
            HandoffJobTransitionStatus::ExactNew
        );
        let store = recovery.publish().unwrap();
        store.close().unwrap();
        let (reopened, state) = open(directory.path());
        assert_eq!(
            state
                .durable_jobs()
                .handoff_job_transition_status(&reopened, &witness)
                .unwrap(),
            HandoffJobTransitionStatus::ExactNew
        );
        reopened.close().unwrap();
        let foreign = tempdir().unwrap();
        let (store, state) = open(foreign.path());
        assert!(
            state
                .durable_jobs()
                .handoff_job_transition_status(&store, &witness)
                .is_err()
        );
        store.close().unwrap();
    }
}

#[test]
fn parent_preparation_rejects_wrong_checkpoint_revision_and_failure_kind() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let first = admission(87, 1, 88, "parent-transition");
    let id = first.job_id();
    admit(&store, &state, first);
    let jobs = state.durable_jobs();
    let initial = job(&store, &state, id);
    for transition in [
        HandoffJobTransition::ParentAccepted(cas_identity()),
        HandoffJobTransition::Succeed,
        HandoffJobTransition::RetryableFailure(failure(HandoffFailureKind::ParentIncomplete)),
        HandoffJobTransition::TerminalFailure(failure(HandoffFailureKind::CasUnavailable)),
        HandoffJobTransition::TerminalFailure(failure(HandoffFailureKind::ParentInterrupted)),
    ] {
        assert!(
            jobs.prepare_handoff_job_transition(&store, id, initial.revision(), transition)
                .is_err()
        );
    }
    assert!(
        jobs.prepare_handoff_job_transition(
            &store,
            id,
            initial.revision(),
            HandoffJobTransition::StartParent(parent_identity())
        )
        .is_err()
    );
    assert!(
        jobs.prepare_handoff_job_transition(
            &store,
            id,
            initial.revision(),
            HandoffJobTransition::ParentArchived(
                HandoffFailureEvidence::new(HandoffFailureKind::ChildInputPending, None).unwrap()
            )
        )
        .is_err()
    );
    assert!(
        jobs.prepare_handoff_job_transition(
            &store,
            JobId::from_bytes([99; 16]),
            initial.revision(),
            fail_parent()
        )
        .is_err()
    );
    advance_to_checkpoint(&store, &state, id, Checkpoint::Waiting);
    let waiting = job(&store, &state, id);
    assert!(
        jobs.prepare_handoff_job_transition(&store, id, initial.revision(), fail_parent())
            .is_err()
    );
    for transition in [HandoffJobTransition::CompleteResolving, fail_child()] {
        assert!(
            jobs.prepare_handoff_job_transition(&store, id, waiting.revision(), transition)
                .is_err()
        );
    }
    let parent = jobs
        .prepare_handoff_job_transition(
            &store,
            id,
            waiting.revision(),
            HandoffJobTransition::StartParent(parent_identity()),
        )
        .unwrap();
    assert!(matches!(
        execute(&store, parent.contribution()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let started = job(&store, &state, id);
    for transition in [
        fail_parent(),
        HandoffJobTransition::StartParent(parent_identity()),
        HandoffJobTransition::Succeed,
        HandoffJobTransition::TerminalFailure(failure(HandoffFailureKind::ParentIncomplete)),
    ] {
        assert!(
            jobs.prepare_handoff_job_transition(&store, id, started.revision(), transition)
                .is_err()
        );
    }
    store.close().unwrap();
}
