mod support;

use beryl_home_store::{CommandOutcome, CursorReadLimits};
use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, JobId, ResolutionIntentId, SyndicAcceptedInputId,
    SyndicDraftId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{
    AdmitBranchHandoffJob, BranchHandoffCheckpoint, BranchHandoffJobAdmission,
    BranchHandoffJobLifecycle, BranchHandoffJobState, CompleteResolvingTurn,
    DiscussionContextDigest, DiscussionContextOwnerId, DurableJobMutationError,
    HandoffFailureEvidence, HandoffFailureKind, ParentCasIdentity, ParentHandoffIdentity,
    ParentQueueOrdinal, RecordParentCasAcceptance, RecordRetryableHandoffFailure,
    RecordTerminalHandoffFailure, ResolutionAttemptOrdinal, ResolutionRequestIdentity,
    ResolutionText, RetryBranchHandoff, StartParentHandoff, SucceedBranchHandoff,
};
use tempfile::tempdir;

use support::{contributor_source, execute, open};

fn admission(
    intent_byte: u8,
    attempt: u64,
    discussion_byte: u8,
    request_suffix: &str,
) -> BranchHandoffJobAdmission {
    BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([intent_byte; 16]),
        ResolutionAttemptOrdinal::new(attempt).unwrap(),
        SyndicThreadId::from_bytes([discussion_byte; 16]),
        SyndicThreadId::from_bytes([90; 16]),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([40; 16])),
        DiscussionContextDigest::from_bytes([50; 32]),
        SyndicTurnId::from_bytes([60; 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new(format!("child-thread-{request_suffix}")).unwrap(),
            CasTurnId::new(format!("child-turn-{request_suffix}")).unwrap(),
            DynamicToolCallId::new(format!("tool-call-{request_suffix}")).unwrap(),
        ),
        ParentQueueOrdinal::new(7),
        ResolutionText::new("Use the branch result exactly.\nPreserve both constraints.").unwrap(),
    )
}

fn admit(
    store: &beryl_home_store::HomeStore,
    state: beryl_state::BerylState,
    admission: BranchHandoffJobAdmission,
) {
    match execute(
        store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(store).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed durable-job admission, got {outcome:?}"),
    }
}

fn job(
    store: &beryl_home_store::HomeStore,
    state: beryl_state::BerylState,
    job_id: JobId,
) -> beryl_state::BranchHandoffJobRecord {
    state.durable_jobs().job(store, job_id).unwrap().unwrap()
}

#[test]
fn admission_is_idempotent_queryable_and_live_after_reopen() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let first = admission(1, 1, 10, "one");
    let request = first.request().clone();
    let job_id = first.job_id();
    admit(&store, state, first);

    let persisted = job(&store, state, job_id);
    assert_eq!(
        persisted.lifecycle(),
        BranchHandoffJobLifecycle::WaitingResolvingTurn
    );
    assert_eq!(persisted.revision().get(), 1);
    assert_eq!(persisted.request(), &request);
    assert_eq!(
        state
            .durable_jobs()
            .request_admission(&store, &request)
            .unwrap()
            .unwrap()
            .job_id(),
        job_id
    );
    let live = state
        .durable_jobs()
        .list_live(&store, None, CursorReadLimits::new(8, 1024 * 1024).unwrap())
        .unwrap();
    assert_eq!(live.records(), std::slice::from_ref(&persisted));
    assert!(live.stored_bytes() > 0);
    assert!(live.decoded_bytes() > 0);

    let mut duplicate = admission(2, 2, 10, "two");
    duplicate = BranchHandoffJobAdmission::new(
        duplicate.intent_id(),
        duplicate.attempt_ordinal(),
        duplicate.discussion_thread_id(),
        duplicate.parent_thread_id(),
        duplicate.context_owner_id(),
        duplicate.context_digest(),
        duplicate.resolving_turn_id(),
        request.clone(),
        duplicate.parent_queue_ordinal(),
        duplicate.resolution().clone(),
    );
    let outcome = execute(
        &store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(duplicate),
        ),
    );
    let CommandOutcome::NotCommitted { evidence: error } = outcome else {
        panic!("expected rejected duplicate durable-job admission, got {outcome:?}");
    };
    assert!(matches!(
        contributor_source::<DurableJobMutationError>(&error),
        Some(DurableJobMutationError::RequestAlreadyAdmitted { .. })
    ));

    store.close().unwrap();
    let (reopened, state) = open(directory.path());
    assert_eq!(job(&reopened, state, job_id), persisted);
    assert_eq!(
        state
            .durable_jobs()
            .request_admission(&reopened, &request)
            .unwrap()
            .unwrap()
            .job_id(),
        job_id
    );
}

#[test]
fn failure_kinds_cannot_claim_an_impossible_job_checkpoint() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let first = admission(9, 1, 19, "failure-stage");
    let job_id = first.job_id();
    admit(&store, state, first);

    for evidence in [
        HandoffFailureEvidence::new(HandoffFailureKind::CasRejectedBeforeAcceptance, None).unwrap(),
        HandoffFailureEvidence::new(HandoffFailureKind::ParentInterrupted, None).unwrap(),
    ] {
        let contribution = if evidence.kind().is_retryable() {
            state.durable_jobs().record_retryable_failure(
                state.durable_jobs().revision(&store).unwrap(),
                RecordRetryableHandoffFailure::new(
                    job_id,
                    job(&store, state, job_id).revision(),
                    evidence,
                ),
            )
        } else {
            state.durable_jobs().record_terminal_failure(
                state.durable_jobs().revision(&store).unwrap(),
                RecordTerminalHandoffFailure::new(
                    job_id,
                    job(&store, state, job_id).revision(),
                    evidence,
                ),
            )
        };
        let outcome = execute(&store, contribution);
        let CommandOutcome::NotCommitted { evidence: error } = outcome else {
            panic!("expected rejected durable-job command, got {outcome:?}");
        };
        assert!(matches!(
            contributor_source::<DurableJobMutationError>(&error),
            Some(DurableJobMutationError::FailureKindMismatch { .. })
        ));
        assert_eq!(
            job(&store, state, job_id).lifecycle(),
            BranchHandoffJobLifecycle::WaitingResolvingTurn
        );
    }
}

#[test]
fn retry_resumes_the_same_parent_turn_and_terminal_failure_releases_live_index() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let first = admission(3, 1, 11, "retry");
    let job_id = first.job_id();
    admit(&store, state, first);

    match execute(
        &store,
        state.durable_jobs().complete_resolving_turn(
            state.durable_jobs().revision(&store).unwrap(),
            CompleteResolvingTurn::new(job_id, job(&store, state, job_id).revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed resolving-turn completion, got {outcome:?}"),
    }
    let parent = ParentHandoffIdentity::new(
        SyndicAcceptedInputId::from_bytes([70; 16]),
        SyndicTurnId::from_bytes([71; 16]),
    );
    match execute(
        &store,
        state.durable_jobs().start_parent_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            StartParentHandoff::new(job_id, job(&store, state, job_id).revision(), parent),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed parent-handoff start, got {outcome:?}"),
    }
    let retryable = HandoffFailureEvidence::new(
        HandoffFailureKind::CasRejectedBeforeAcceptance,
        Some("CAS rejected before accepting the correlated parent turn"),
    )
    .unwrap();
    match execute(
        &store,
        state.durable_jobs().record_retryable_failure(
            state.durable_jobs().revision(&store).unwrap(),
            RecordRetryableHandoffFailure::new(
                job_id,
                job(&store, state, job_id).revision(),
                retryable.clone(),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed retryable failure recording, got {outcome:?}"),
    }
    let failed = job(&store, state, job_id);
    assert!(matches!(
        failed.state(),
        BranchHandoffJobState::RetryableFailed {
            resume: BranchHandoffCheckpoint::StartingParent { parent: retained },
            evidence,
        } if *retained == parent && evidence == &retryable
    ));

    match execute(
        &store,
        state.durable_jobs().retry_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            RetryBranchHandoff::new(job_id, failed.revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed branch-handoff retry, got {outcome:?}"),
    }
    assert!(matches!(
        job(&store, state, job_id).state(),
        BranchHandoffJobState::StartingParent { parent: retained } if *retained == parent
    ));

    let cas = ParentCasIdentity::new(
        CasThreadId::new("parent-cas-thread").unwrap(),
        CasTurnId::new("parent-cas-turn").unwrap(),
    );
    match execute(
        &store,
        state.durable_jobs().record_parent_cas_acceptance(
            state.durable_jobs().revision(&store).unwrap(),
            RecordParentCasAcceptance::new(
                job_id,
                job(&store, state, job_id).revision(),
                cas.clone(),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed parent CAS acceptance, got {outcome:?}"),
    }
    let terminal = HandoffFailureEvidence::new(
        HandoffFailureKind::ParentInterrupted,
        Some("the exact accepted parent turn ended interrupted"),
    )
    .unwrap();
    match execute(
        &store,
        state.durable_jobs().record_terminal_failure(
            state.durable_jobs().revision(&store).unwrap(),
            RecordTerminalHandoffFailure::new(
                job_id,
                job(&store, state, job_id).revision(),
                terminal.clone(),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed terminal failure recording, got {outcome:?}"),
    }
    let failed = job(&store, state, job_id);
    assert!(matches!(
        failed.state(),
        BranchHandoffJobState::TerminalFailed {
            stopped_at: BranchHandoffCheckpoint::ParentActive {
                parent: retained,
                cas: retained_cas,
            },
            evidence,
        } if *retained == parent && retained_cas == &cas && evidence == &terminal
    ));
    assert!(state
        .durable_jobs()
        .list_live(&store, None, CursorReadLimits::new(8, 1024 * 1024).unwrap(),)
        .unwrap()
        .records()
        .is_empty());

    let second = admission(4, 2, 11, "fresh-after-terminal");
    let second_job = second.job_id();
    admit(&store, state, second);
    assert_eq!(
        state
            .durable_jobs()
            .latest_attempt(&store, SyndicThreadId::from_bytes([11; 16]))
            .unwrap()
            .unwrap()
            .job_id(),
        second_job
    );
}

#[test]
fn success_retains_relationships_and_cannot_regress() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let first = admission(5, 1, 12, "success");
    let discussion_id = first.discussion_thread_id();
    let parent_thread_id = first.parent_thread_id();
    let job_id = first.job_id();
    admit(&store, state, first);
    match execute(
        &store,
        state.durable_jobs().complete_resolving_turn(
            state.durable_jobs().revision(&store).unwrap(),
            CompleteResolvingTurn::new(job_id, job(&store, state, job_id).revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed resolving-turn completion, got {outcome:?}"),
    }
    match execute(
        &store,
        state.durable_jobs().start_parent_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            StartParentHandoff::new(
                job_id,
                job(&store, state, job_id).revision(),
                ParentHandoffIdentity::new(
                    SyndicAcceptedInputId::from_bytes([72; 16]),
                    SyndicTurnId::from_bytes([73; 16]),
                ),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed parent-handoff start, got {outcome:?}"),
    }
    match execute(
        &store,
        state.durable_jobs().record_parent_cas_acceptance(
            state.durable_jobs().revision(&store).unwrap(),
            RecordParentCasAcceptance::new(
                job_id,
                job(&store, state, job_id).revision(),
                ParentCasIdentity::new(
                    CasThreadId::new("successful-parent-thread").unwrap(),
                    CasTurnId::new("successful-parent-turn").unwrap(),
                ),
            ),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed parent CAS acceptance, got {outcome:?}"),
    }

    match execute(
        &store,
        state.durable_jobs().succeed_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            SucceedBranchHandoff::new(job_id, job(&store, state, job_id).revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed branch-handoff success, got {outcome:?}"),
    }

    let succeeded = job(&store, state, job_id);
    assert_eq!(succeeded.lifecycle(), BranchHandoffJobLifecycle::Succeeded);
    assert_eq!(succeeded.discussion_thread_id(), discussion_id);
    assert_eq!(succeeded.parent_thread_id(), parent_thread_id);
    assert!(state
        .durable_jobs()
        .list_live(&store, None, CursorReadLimits::new(8, 1024 * 1024).unwrap(),)
        .unwrap()
        .records()
        .is_empty());

    let outcome = execute(
        &store,
        state.durable_jobs().record_terminal_failure(
            state.durable_jobs().revision(&store).unwrap(),
            RecordTerminalHandoffFailure::new(
                job_id,
                succeeded.revision(),
                HandoffFailureEvidence::new(HandoffFailureKind::InvariantViolation, None).unwrap(),
            ),
        ),
    );
    let CommandOutcome::NotCommitted {
        evidence: regression,
    } = outcome
    else {
        panic!("expected rejected durable-job regression, got {outcome:?}");
    };
    assert!(matches!(
        contributor_source::<DurableJobMutationError>(&regression),
        Some(DurableJobMutationError::InvalidTransition { .. })
    ));

    let later = admission(6, 2, 12, "after-success");
    let outcome = execute(
        &store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            AdmitBranchHandoffJob::new(later),
        ),
    );
    let CommandOutcome::NotCommitted { evidence: error } = outcome else {
        panic!("expected rejected post-success durable-job admission, got {outcome:?}");
    };
    assert!(matches!(
        contributor_source::<DurableJobMutationError>(&error),
        Some(DurableJobMutationError::SuccessfulAttemptExists { .. })
    ));
}

#[test]
fn resolution_and_failure_evidence_are_strictly_bounded() {
    assert!(ResolutionText::new("").is_err());
    assert!(ResolutionText::new("x".repeat(64 * 1024 + 1)).is_err());
    assert!(HandoffFailureEvidence::new(
        HandoffFailureKind::InvariantViolation,
        Some(&"x".repeat(2 * 1024 + 1)),
    )
    .is_err());
    assert!(ResolutionAttemptOrdinal::new(0).is_err());
}
