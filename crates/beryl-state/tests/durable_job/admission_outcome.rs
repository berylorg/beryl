use super::*;
use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::{
    BerylState, HandoffJobAdmissionStatus, HandoffJobIndexFault, HandoffJobTransition,
};

fn terminal(store: &beryl_home_store::HomeStore, state: &BerylState, id: JobId) {
    let jobs = state.durable_jobs();
    let old = job(store, state, id);
    let prepared = jobs
        .prepare_handoff_job_transition(
            store,
            id,
            old.revision(),
            HandoffJobTransition::TerminalFailure(
                HandoffFailureEvidence::new(HandoffFailureKind::InvariantViolation, None).unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        execute(store, prepared.contribution()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn exact_admission_scope_and_historical_request_lookup_have_distinct_lifetimes() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let jobs = state.durable_jobs();
    let base = admission(101, 1, 102, "first-admission");
    let first = BranchHandoffJobAdmission::new(
        base.intent_id(),
        base.attempt_ordinal(),
        base.discussion_thread_id(),
        base.parent_thread_id(),
        base.context_owner_id(),
        base.context_digest(),
        base.resolving_turn_id(),
        base.request().clone(),
        base.parent_queue_ordinal(),
        ResolutionText::new("🦀".repeat(beryl_state::RESOLUTION_TEXT_MAX_SCALARS)).unwrap(),
    );
    let request = first.request().clone();
    let id = first.job_id();
    assert!(
        jobs.admitted_handoff_request(&store, &request)
            .unwrap()
            .is_none()
    );
    let prepared = jobs
        .prepare_handoff_job_admission(&store, first.clone())
        .unwrap();
    let witness = prepared.witness().clone();
    assert_eq!(
        jobs.handoff_job_admission_status(&store, &witness).unwrap(),
        HandoffJobAdmissionStatus::ExactOld
    );
    assert!(matches!(
        execute(&store, prepared.contribution()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        jobs.handoff_job_admission_status(&store, &witness).unwrap(),
        HandoffJobAdmissionStatus::ExactNew
    );
    assert_eq!(
        jobs.admitted_handoff_request(&store, &request)
            .unwrap()
            .as_ref(),
        Some(witness.new_job())
    );
    assert!(matches!(
        jobs.prepare_handoff_job_admission(&store, first),
        Err(DurableJobMutationError::RequestAlreadyAdmitted { .. })
    ));
    let later = admission(103, 2, 102, "later-admission");
    assert!(matches!(
        jobs.prepare_handoff_job_admission(&store, later.clone()),
        Err(DurableJobMutationError::LiveAttemptExists { .. })
    ));
    terminal(&store, &state, id);
    let original_terminal = jobs
        .admitted_handoff_request(&store, &request)
        .unwrap()
        .unwrap();
    assert_eq!(
        original_terminal.lifecycle(),
        BranchHandoffJobLifecycle::TerminalFailed
    );
    assert_eq!(
        jobs.handoff_job_admission_status(&store, &witness).unwrap(),
        HandoffJobAdmissionStatus::Collision
    );
    let prepared = jobs.prepare_handoff_job_admission(&store, later).unwrap();
    let later_witness = prepared.witness().clone();
    assert_eq!(
        jobs.handoff_job_admission_status(&store, &later_witness)
            .unwrap(),
        HandoffJobAdmissionStatus::ExactOld
    );
    assert!(matches!(
        execute(&store, prepared.contribution()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        jobs.handoff_job_admission_status(&store, &later_witness)
            .unwrap(),
        HandoffJobAdmissionStatus::ExactNew
    );
    assert_eq!(
        jobs.admitted_handoff_request(&store, &request).unwrap(),
        Some(original_terminal.clone())
    );
    store.close().unwrap();
    let (store, fresh) = open(directory.path());
    assert_eq!(
        fresh
            .durable_jobs()
            .admitted_handoff_request(&store, &request)
            .unwrap(),
        Some(original_terminal)
    );
    store.close().unwrap();
}

#[test]
fn missing_admission_records_never_classify_as_exact_new_or_admitted_request() {
    for fault in [
        HandoffJobIndexFault::MissingJob,
        HandoffJobIndexFault::MissingLive,
        HandoffJobIndexFault::MissingRequest,
        HandoffJobIndexFault::MissingAttempt,
        HandoffJobIndexFault::MissingLatest,
    ] {
        let directory = tempdir().unwrap();
        let (store, state) = open(directory.path());
        let jobs = state.durable_jobs();
        let request = admission(104, 1, 105, "closure");
        let prepared = jobs
            .prepare_handoff_job_admission(&store, request.clone())
            .unwrap();
        let witness = prepared.witness().clone();
        assert!(matches!(
            execute(&store, prepared.contribution()),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert!(matches!(
            execute(
                &store,
                jobs.corrupt_handoff_job_index_for_test(
                    jobs.revision(&store).unwrap(),
                    witness.new_job().clone(),
                    fault.clone()
                )
            ),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert_eq!(
            jobs.handoff_job_admission_status(&store, &witness).unwrap(),
            HandoffJobAdmissionStatus::Collision
        );
        match fault {
            HandoffJobIndexFault::MissingRequest => assert!(
                jobs.admitted_handoff_request(&store, request.request())
                    .unwrap()
                    .is_none()
            ),
            HandoffJobIndexFault::MissingLatest => assert!(
                jobs.admitted_handoff_request(&store, request.request())
                    .unwrap()
                    .is_some()
            ),
            _ => assert!(
                jobs.admitted_handoff_request(&store, request.request())
                    .is_err()
            ),
        }
        store.close().unwrap();
    }
}

#[test]
fn corrupt_predecessor_closure_and_stale_writer_cannot_admit_later_attempt() {
    for fault in [
        HandoffJobIndexFault::MissingRequest,
        HandoffJobIndexFault::MissingAttempt,
    ] {
        let directory = tempdir().unwrap();
        let (store, state) = open(directory.path());
        let jobs = state.durable_jobs();
        let first = admission(106, 1, 107, "prior");
        let id = first.job_id();
        admit(&store, &state, first);
        terminal(&store, &state, id);
        let later = admission(108, 2, 107, "next");
        let prepared = jobs
            .prepare_handoff_job_admission(&store, later.clone())
            .unwrap();
        let witness = prepared.witness().clone();
        assert!(matches!(
            execute(
                &store,
                jobs.corrupt_handoff_job_index_for_test(
                    jobs.revision(&store).unwrap(),
                    job(&store, &state, id),
                    fault
                )
            ),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert!(
            jobs.prepare_handoff_job_admission(&store, later.clone())
                .is_err()
        );
        assert!(matches!(
            execute(
                &store,
                jobs.admit_branch_handoff(
                    jobs.revision(&store).unwrap(),
                    AdmitBranchHandoffJob::new(later)
                )
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        assert!(matches!(
            execute(&store, prepared.contribution()),
            CommandOutcome::NotCommitted { .. }
        ));
        assert_eq!(
            jobs.handoff_job_admission_status(&store, &witness).unwrap(),
            HandoffJobAdmissionStatus::Collision
        );
        store.close().unwrap();
    }
}

#[test]
fn recovered_candidate_inspects_admission_without_reviving_old_writer_or_foreign_witness() {
    for point in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
    ] {
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
        let jobs = state.durable_jobs();
        let admission = admission(109, 1, 110, "candidate-admission");
        let prepared = jobs
            .prepare_handoff_job_admission(&store, admission.clone())
            .unwrap();
        let stale = jobs
            .prepare_handoff_job_admission(&store, admission.clone())
            .unwrap();
        let witness = prepared.witness().clone();
        faults.fail_next(point);
        match execute(&store, prepared.contribution()) {
            CommandOutcome::NotCommitted { .. } if point == FaultPoint::BeforeCommit => {}
            CommandOutcome::Indeterminate { reconciliation, .. }
                if point == FaultPoint::AfterCommitBeforePersist =>
            {
                reconciliation.install();
            }
            other => panic!("unexpected fault outcome {other:?}"),
        }
        if store.health().state() == beryl_home_store::HomeHealthState::Healthy {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
            assert!(store.home_revision().is_err());
        }
        let mut recovery = store.recover_same_home().unwrap();
        let fresh = BerylState::reacquire_candidate(&recovery).unwrap();
        let access = recovery.recovery_access().unwrap();
        assert!(
            jobs.handoff_job_admission_status_candidate(&access, &witness)
                .is_err()
        );
        assert_eq!(
            fresh
                .durable_jobs()
                .handoff_job_admission_status_candidate(&access, &witness)
                .unwrap(),
            if point == FaultPoint::BeforeCommit {
                HandoffJobAdmissionStatus::ExactOld
            } else {
                HandoffJobAdmissionStatus::ExactNew
            }
        );
        assert_eq!(
            fresh
                .durable_jobs()
                .admitted_handoff_request_candidate(&access, admission.request())
                .unwrap()
                .is_some(),
            point == FaultPoint::AfterCommitBeforePersist
        );
        let mut command = beryl_home_store::HomeCommand::new(access.home_revision().unwrap());
        command.add(stale.contribution()).unwrap();
        assert!(matches!(
            access.execute(command),
            CommandOutcome::NotCommitted { .. }
        ));
        for pending in access.pending_reconciliations() {
            access.reconcile(&pending).unwrap();
        }
        recovery.publish().unwrap().close().unwrap();
        let foreign_dir = tempdir().unwrap();
        let (foreign, foreign_state) = open(foreign_dir.path());
        assert!(
            foreign_state
                .durable_jobs()
                .handoff_job_admission_status(&foreign, &witness)
                .is_err()
        );
        foreign.close().unwrap();
    }
}
