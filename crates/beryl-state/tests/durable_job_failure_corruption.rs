#![cfg(feature = "test-faults")]

mod support;

use std::path::Path;

use beryl_home_store::{
    CommandOutcome, DomainCallbackSource, DomainRegistrationError, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, ResolutionIntentId, SyndicAcceptedInputId,
    SyndicDraftId, SyndicThreadId, SyndicTurnId,
};
use beryl_state::{
    AdmitBranchHandoffJob, BerylState, BerylStateRegistrationError, BranchHandoffCheckpoint,
    BranchHandoffJobAdmission, DiscussionContextDigest, DiscussionContextOwnerId,
    HandoffFailureEvidence, HandoffFailureKind, ParentCasIdentity, ParentHandoffIdentity,
    ParentQueueOrdinal, ResolutionAttemptOrdinal, ResolutionRequestIdentity, ResolutionText,
};
use tempfile::tempdir;

use support::{execute, open};

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
fn routine_reopen_defers_every_dormant_incompatible_persisted_failure_pair_to_schema_validation() {
    let directory = tempdir().unwrap();
    let mut case = 0_u8;

    for retryable in [true, false] {
        for kind in FAILURE_KINDS {
            for stage in STAGES {
                if compatible(retryable, kind, stage) {
                    continue;
                }
                case = case.checked_add(1).unwrap();
                let path = directory.path().join(format!("ordinary-{case}"));
                std::fs::create_dir(&path).unwrap();
                let store = corrupt_home(&path, case, retryable, kind, stage);
                store.close().unwrap();

                let mut reopened =
                    HomeStore::open(HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT))
                        .unwrap();
                let routine = BerylState::register(&mut reopened)
                    .expect("routine reopen must not exhaustively scan durable jobs");
                routine.durable_jobs().revision(&reopened).expect(
                    "routine durable-job handle must be usable before the dormant record is read",
                );
                reopened.close().unwrap();

                let mut schema_boundary =
                    HomeStore::open(HomeOpenOptions::new(&path, HomeSchemaVersion::CURRENT))
                        .unwrap();
                let error = match BerylState::register_with_schema_validation(&mut schema_boundary)
                {
                    Ok(_) => panic!("incompatible persisted failure unexpectedly reopened"),
                    Err(error) => error,
                };
                assert_registration_failure(error);
            }
        }
    }

    assert_eq!(case, 58);
}

fn corrupt_home(
    path: &Path,
    seed: u8,
    retryable: bool,
    kind: HandoffFailureKind,
    stage: Stage,
) -> HomeStore {
    let (store, state) = open(path);
    corrupt_job(&store, &state, seed, retryable, kind, stage);
    store
}

fn corrupt_job(
    store: &HomeStore,
    state: &BerylState,
    seed: u8,
    retryable: bool,
    kind: HandoffFailureKind,
    stage: Stage,
) {
    let admission = admission(seed);
    let job_id = admission.job_id();
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
    let job = state.durable_jobs().job(store, job_id).unwrap().unwrap();
    match execute(
        store,
        state.durable_jobs().corrupt_failure_state_for_test(
            state.durable_jobs().revision(store).unwrap(),
            job_id,
            job.revision(),
            checkpoint(stage, seed),
            HandoffFailureEvidence::new(kind, None).unwrap(),
            retryable,
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed durable-job corruption, got {outcome:?}"),
    }
}

fn assert_registration_failure(error: BerylStateRegistrationError) {
    let BerylStateRegistrationError::Domain { domain, source } = error else {
        panic!("expected durable-job registration failure, got {error}");
    };
    assert_eq!(domain, "beryl-durable-job");
    let DomainRegistrationError::ValidationAccess {
        domain,
        source: DomainCallbackSource::Read(source),
    } = source
    else {
        panic!("expected durable-job validation-access failure, got {source}");
    };
    assert_eq!(domain, "beryl-durable-job");
    assert!(source.to_string().contains("incompatible"));
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

fn checkpoint(stage: Stage, seed: u8) -> BranchHandoffCheckpoint {
    match stage {
        Stage::WaitingResolvingTurn => BranchHandoffCheckpoint::WaitingResolvingTurn,
        Stage::WaitingParent => BranchHandoffCheckpoint::WaitingParent,
        Stage::StartingParent => BranchHandoffCheckpoint::StartingParent {
            parent: parent(seed),
        },
        Stage::ParentActive => BranchHandoffCheckpoint::ParentActive {
            parent: parent(seed),
            cas: ParentCasIdentity::new(
                CasThreadId::new(format!("parent-thread-{seed}")).unwrap(),
                CasTurnId::new(format!("parent-turn-{seed}")).unwrap(),
            ),
        },
    }
}

fn parent(seed: u8) -> ParentHandoffIdentity {
    ParentHandoffIdentity::new(
        SyndicAcceptedInputId::from_bytes([seed.wrapping_add(5); 16]),
        SyndicTurnId::from_bytes([seed.wrapping_add(6); 16]),
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
