mod support;

use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, JobId, ResolutionIntentId, SyndicAcceptedInputId,
    SyndicDraftId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{
    AdmitBranchHandoffJob, BranchHandoffJobAdmission, CompleteResolvingTurn,
    DiscussionContextDigest, DiscussionContextOwnerId, DurableJobMutationError,
    HandoffFailureEvidence, HandoffFailureKind, ParentCasIdentity, ParentHandoffIdentity,
    ParentQueueOrdinal, RecordParentCasAcceptance, RecordRetryableHandoffFailure,
    RecordTerminalHandoffFailure, ResolutionAttemptOrdinal, ResolutionRequestIdentity,
    ResolutionText, StartParentHandoff,
};
use tempfile::tempdir;

use support::{contributor_source, execute, open};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stage {
    WaitingResolvingTurn,
    WaitingParent,
    StartingParent,
    ParentActive,
}

const STAGES: [Stage; 4] = [
    Stage::WaitingResolvingTurn,
    Stage::WaitingParent,
    Stage::StartingParent,
    Stage::ParentActive,
];

const FAILURE_KINDS: [HandoffFailureKind; 11] = [
    HandoffFailureKind::RuntimeUnavailable,
    HandoffFailureKind::RootUnavailable,
    HandoffFailureKind::CasUnavailable,
    HandoffFailureKind::TransientDeliveryFailure,
    HandoffFailureKind::CasRejectedBeforeAcceptance,
    HandoffFailureKind::InvariantViolation,
    HandoffFailureKind::ParentMissing,
    HandoffFailureKind::UnrecoverablePostAppend,
    HandoffFailureKind::ParentInterrupted,
    HandoffFailureKind::ParentIncomplete,
    HandoffFailureKind::ParentTerminalFailure,
];

#[test]
fn mutation_admission_enforces_the_complete_failure_checkpoint_matrix() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let mut seed = 1_u8;
    let mut accepted = 0;
    let mut rejected = 0;

    for retryable in [true, false] {
        for kind in FAILURE_KINDS {
            for stage in STAGES {
                let job_id = place_job(&store, state, seed, stage);
                let job = state.durable_jobs().job(&store, job_id).unwrap().unwrap();
                let evidence = HandoffFailureEvidence::new(kind, None).unwrap();
                let contribution = if retryable {
                    state.durable_jobs().record_retryable_failure(
                        state.durable_jobs().revision(&store).unwrap(),
                        RecordRetryableHandoffFailure::new(job_id, job.revision(), evidence),
                    )
                } else {
                    state.durable_jobs().record_terminal_failure(
                        state.durable_jobs().revision(&store).unwrap(),
                        RecordTerminalHandoffFailure::new(job_id, job.revision(), evidence),
                    )
                };
                let result = execute(&store, contribution);
                if compatible(retryable, kind, stage) {
                    result.unwrap();
                    accepted += 1;
                } else {
                    let error = result.unwrap_err();
                    assert!(matches!(
                        contributor_source::<DurableJobMutationError>(&error),
                        Some(DurableJobMutationError::FailureKindMismatch { .. })
                    ));
                    rejected += 1;
                }
                seed = seed.checked_add(1).unwrap();
            }
        }
    }

    assert_eq!(accepted, 30);
    assert_eq!(rejected, 58);
    store.close().unwrap();
    let (reopened, _) = open(directory.path());
    reopened.validate_registered_domains().unwrap();
}

fn place_job(
    store: &beryl_home_store::HomeStore,
    state: beryl_state::BerylState,
    seed: u8,
    stage: Stage,
) -> JobId {
    let admission = admission(seed);
    let job_id = admission.job_id();
    execute(
        store,
        state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(store).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ),
    )
    .unwrap();
    if stage == Stage::WaitingResolvingTurn {
        return job_id;
    }

    execute(
        store,
        state.durable_jobs().complete_resolving_turn(
            state.durable_jobs().revision(store).unwrap(),
            CompleteResolvingTurn::new(job_id, job_revision(store, state, job_id)),
        ),
    )
    .unwrap();
    if stage == Stage::WaitingParent {
        return job_id;
    }

    let parent = parent(seed);
    execute(
        store,
        state.durable_jobs().start_parent_handoff(
            state.durable_jobs().revision(store).unwrap(),
            StartParentHandoff::new(job_id, job_revision(store, state, job_id), parent),
        ),
    )
    .unwrap();
    if stage == Stage::StartingParent {
        return job_id;
    }

    execute(
        store,
        state.durable_jobs().record_parent_cas_acceptance(
            state.durable_jobs().revision(store).unwrap(),
            RecordParentCasAcceptance::new(
                job_id,
                job_revision(store, state, job_id),
                parent_cas(seed),
            ),
        ),
    )
    .unwrap();
    job_id
}

fn job_revision(
    store: &beryl_home_store::HomeStore,
    state: beryl_state::BerylState,
    job_id: JobId,
) -> beryl_model::JobRevision {
    state
        .durable_jobs()
        .job(store, job_id)
        .unwrap()
        .unwrap()
        .revision()
}

fn admission(seed: u8) -> BranchHandoffJobAdmission {
    BranchHandoffJobAdmission::new(
        ResolutionIntentId::from_bytes([seed; 16]),
        ResolutionAttemptOrdinal::FIRST,
        SyndicThreadId::from_bytes([seed; 16]),
        SyndicThreadId::from_bytes([seed.wrapping_add(1); 16]),
        DiscussionContextOwnerId::Draft(SyndicDraftId::from_bytes([seed.wrapping_add(2); 16])),
        DiscussionContextDigest::from_bytes([seed.wrapping_add(3); 32]),
        SyndicTurnId::from_bytes([seed.wrapping_add(4); 16]),
        ResolutionRequestIdentity::new(
            CasThreadId::new(format!("child-thread-{seed}")).unwrap(),
            CasTurnId::new(format!("child-turn-{seed}")).unwrap(),
            DynamicToolCallId::new(format!("tool-call-{seed}")).unwrap(),
        ),
        ParentQueueOrdinal::new(u64::from(seed)),
        ResolutionText::new(format!("resolution-{seed}")).unwrap(),
    )
}

fn parent(seed: u8) -> ParentHandoffIdentity {
    ParentHandoffIdentity::new(
        SyndicAcceptedInputId::from_bytes([seed.wrapping_add(5); 16]),
        SyndicTurnId::from_bytes([seed.wrapping_add(6); 16]),
    )
}

fn parent_cas(seed: u8) -> ParentCasIdentity {
    ParentCasIdentity::new(
        CasThreadId::new(format!("parent-thread-{seed}")).unwrap(),
        CasTurnId::new(format!("parent-turn-{seed}")).unwrap(),
    )
}

fn compatible(retryable: bool, kind: HandoffFailureKind, stage: Stage) -> bool {
    if retryable != kind.is_retryable() {
        return false;
    }
    match kind {
        HandoffFailureKind::CasRejectedBeforeAcceptance => stage == Stage::StartingParent,
        HandoffFailureKind::UnrecoverablePostAppend => {
            matches!(stage, Stage::StartingParent | Stage::ParentActive)
        }
        HandoffFailureKind::ParentInterrupted
        | HandoffFailureKind::ParentIncomplete
        | HandoffFailureKind::ParentTerminalFailure => stage == Stage::ParentActive,
        _ => true,
    }
}
