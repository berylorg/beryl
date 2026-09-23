use super::*;
use beryl_state::{
    AdmitBranchHandoffJob, BerylState, BranchHandoffJobAdmission, CompleteResolvingTurn,
    ParentQueueOrdinal, ResolutionAttemptOrdinal, ResolutionRequestIdentity, ResolutionText,
};

fn settled(
    storage: &SyndicStorage,
    store: &HomeStore,
    pending: DiscussionHandoffGateRecord,
) -> PreparedDiscussionChildSettlement {
    let DiscussionChildSettlement::Settled(prepared) = storage
        .prepare_discussion_child_settlement(store, pending)
        .unwrap()
    else {
        panic!("expected settled child")
    };
    prepared
}

fn admit(
    storage: &SyndicStorage,
    store: &HomeStore,
    request: AdmitDiscussionHandoff,
) -> DiscussionHandoffGateRecord {
    let prepared = storage
        .prepare_discussion_handoff(store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let pending = prepared.intent().new_gate();
    execute(store, prepared);
    pending
}

fn ready_command(
    access: &beryl_home_store::HomeCandidateRecoveryAccess<'_>,
    state: &BerylState,
    proof: PreparedDiscussionChildSettlement,
    job: JobId,
) -> HomeCommand {
    let jobs = state.durable_jobs();
    let record = jobs.job_candidate(access, job).unwrap().unwrap();
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command
        .add_validation(proof.into_ready_validation().unwrap())
        .unwrap();
    command
        .add(jobs.complete_resolving_turn(
            jobs.revision_candidate(access).unwrap(),
            CompleteResolvingTurn::new(job, record.revision()),
        ))
        .unwrap();
    command
}

#[test]
fn ready_validation_waits_for_terminal_history_and_fences_recovered_handles() {
    let home = TestHome::new("child-settlement-ready");
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    let mut request = support::discussion_handoff::active_request(
        &store,
        &storage,
        ResolutionIntentId::from_bytes([210; 16]),
        JobId::from_bytes([211; 16]),
    );
    let admission = BranchHandoffJobAdmission::new(
        request.intent_id,
        ResolutionAttemptOrdinal::new(1).unwrap(),
        request.thread_id,
        request.parent.thread_id,
        request.context_owner,
        request.context_digest,
        request.resolving_target.pending().active_turn_id(),
        ResolutionRequestIdentity::new(
            request.resolving_target.pending().cas_thread_id().clone(),
            request.resolving_target.cas_turn_id().clone(),
            beryl_model::DynamicToolCallId::new("resolve").unwrap(),
        ),
        ParentQueueOrdinal::new(request.parent.accepted_high_water),
        ResolutionText::new("Resolution result").unwrap(),
    );
    let job = admission.job_id();
    request.job_id = job;
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let pending = prepared.intent().new_gate();
    let mut command = command(&store, prepared);
    command
        .add(state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(matches!(
        storage
            .prepare_discussion_child_settlement(&store, pending)
            .unwrap(),
        DiscussionChildSettlement::Waiting
    ));
    support::discussion_handoff::complete_resolving_turn(&store, &storage);
    let old = settled(&storage, &store, pending);
    assert_eq!(
        old.disposition(),
        DiscussionChildSettlementDisposition::Ready
    );
    assert!(old.clone().into_queued_release().is_err());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let mut recovery = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let fresh_state = BerylState::reacquire_candidate(&recovery).unwrap();
    let access = recovery.recovery_access().unwrap();
    assert!(
        storage
            .prepare_discussion_child_settlement_candidate(&access, pending)
            .is_err()
    );
    let DiscussionChildSettlement::Settled(proof) = fresh
        .prepare_discussion_child_settlement_candidate(&access, pending)
        .unwrap()
    else {
        panic!("candidate should prove settled child")
    };
    assert_eq!(
        proof.disposition(),
        DiscussionChildSettlementDisposition::Ready
    );
    assert!(matches!(
        access.execute(ready_command(&access, &fresh_state, old, job)),
        CommandOutcome::NotCommitted { .. }
    ));
    let candidate_command = ready_command(&access, &fresh_state, proof, job);
    let before = fresh.revision_candidate(&access).unwrap();
    assert!(matches!(
        access.execute(candidate_command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(fresh.revision_candidate(&access).unwrap(), before);
    let store = recovery.publish().unwrap();
    assert_eq!(fresh.revision(&store).unwrap(), before);
    assert_eq!(
        fresh_state
            .durable_jobs()
            .job(&store, job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        beryl_state::BranchHandoffJobLifecycle::WaitingParent
    );
    store.close().unwrap();
}

#[test]
fn queued_settlement_releases_exact_gate_and_preserves_input_after_stale_proof_rejection() {
    let home = TestHome::new("child-settlement-queued");
    let (store, storage, mut request) = seeded(&home, FaultController::new());
    queue::accept_next(&store, &storage);
    request.input_gate_revision = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .revision();
    request.thread_revision = storage
        .thread(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .revision();
    let pending = admit(&storage, &store, request);
    assert!(matches!(
        storage
            .prepare_discussion_child_settlement(&store, pending)
            .unwrap(),
        DiscussionChildSettlement::Waiting
    ));
    support::discussion_handoff::complete_resolving_turn(&store, &storage);
    let proof = settled(&storage, &store, pending);
    assert_eq!(
        proof.disposition(),
        DiscussionChildSettlementDisposition::QueuedInput
    );
    assert!(proof.clone().into_ready_validation().is_err());
    let input_before = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(input_before.live_next_turn_count(), 1);
    let stale = proof.into_queued_release().unwrap();
    support::commit(
        &store,
        storage.clone(),
        support::batch([
            syndic_storage::test_faults::FixtureRecord::DiscussionHandoffGate(pending),
        ]),
    );
    assert!(matches!(
        store.execute(command(&store, stale)),
        CommandOutcome::NotCommitted { .. }
    ));
    let prepared = settled(&storage, &store, pending)
        .into_queued_release()
        .unwrap();
    let intent = prepared.intent().clone();
    execute(&store, prepared);
    assert_eq!(
        storage.discussion_handoff_status(&store, &intent).unwrap(),
        DiscussionHandoffStatus::ExactNew
    );
    assert_eq!(
        storage
            .input_gate(&store, id(36), limit())
            .unwrap()
            .unwrap(),
        input_before
    );
    assert!(
        storage
            .prepare_discussion_child_settlement(&store, pending)
            .is_err()
    );
    store.close().unwrap();
}

#[test]
fn idle_gate_with_active_binding_is_rejected() {
    let home = TestHome::new("child-settlement-idle-active");
    let (store, storage, request) = seeded(&home, FaultController::new());
    let pending = admit(&storage, &store, request);
    let gate = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let idle = InputGateRecord::new(
        id(36),
        gate.revision().checked_next().unwrap(),
        InputGateState::Idle,
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        0,
        0,
        0,
    )
    .unwrap();
    support::commit(
        &store,
        storage.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(idle)]),
    );
    assert!(matches!(
        storage.prepare_discussion_child_settlement(&store, pending),
        Err(SyndicReadError::Invariant(_))
    ));
    store.close().unwrap();
}

#[test]
fn explicitly_incomplete_terminal_history_is_settled_but_missing_identity_is_not() {
    let home = TestHome::new("child-settlement-incomplete");
    let (store, storage, request) = seeded(&home, FaultController::new());
    let pending = admit(&storage, &store, request);
    support::discussion_handoff::finish_resolving_turn(
        &store,
        &storage,
        TurnEndStatus::new(
            TurnTerminalOutcome::Incomplete,
            Some(TurnIncompleteReason::StreamLost),
        )
        .unwrap(),
    );
    assert_eq!(
        settled(&storage, &store, pending).disposition(),
        DiscussionChildSettlementDisposition::Ready
    );
    let mut batch = syndic_storage::test_faults::FixtureBatch::new();
    batch
        .delete(syndic_storage::test_faults::FixtureDelete::DiscussionHandoffGate(id(36)))
        .unwrap();
    support::commit(&store, storage.clone(), batch);
    assert!(
        storage
            .prepare_discussion_child_settlement(&store, pending)
            .is_err()
    );
    store.close().unwrap();
}
