use super::*;
use beryl_app::discussion_handoff_limits::{HandoffScanConfiguration, HandoffScanLimits};
use beryl_state::{HandoffFailureEvidence, HandoffFailureKind, HandoffJobTransition};

#[path = "candidate_scan/custody.rs"]
mod custody;
#[path = "candidate_scan/nested.rs"]
mod nested;

fn limits() -> HandoffScanLimits {
    HandoffScanLimits::try_from(HandoffScanConfiguration {
        handoff_recovery_page_items: 2,
        handoff_recovery_page_encoded_bytes: beryl_state::HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES,
        handoff_job_record_encoded_bytes: beryl_state::HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
        handoff_reconcile_slots: 1,
        handoff_ready_job_items: 1,
    })
    .unwrap()
}
fn fail_read(fixture: &Fixture) {
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(fixture.store.home_revision().is_err());
}

#[test]
fn candidate_scans_multiple_waiting_pages_and_continues_after_removing_a_live_key() {
    let fixture = Fixture::new(true);
    fixture.finish_child();
    let mut waiting = Vec::new();
    for seed in [220, 230, 240] {
        support::discussion_creation::create_child(
            &fixture.store,
            &fixture.syndic,
            id(30),
            seed,
            seed + 1,
        );
        let request = support::discussion_handoff::active_request_for(
            &fixture.store,
            &fixture.syndic,
            id(seed),
            support::draft_id(seed + 2),
            beryl_model::SyndicItemId::from_bytes([seed + 3; 16]),
            ResolutionIntentId::from_bytes([seed + 4; 16]),
            JobId::from_bytes([seed + 5; 16]),
        );
        waiting.push(admit_request(&fixture.store, &fixture.state, &fixture.syndic, request).0);
    }
    let retryable = fixture
        .state
        .durable_jobs()
        .job(&fixture.store, waiting[0])
        .unwrap()
        .unwrap();
    let transition = fixture
        .state
        .durable_jobs()
        .prepare_handoff_job_transition(
            &fixture.store,
            retryable.job_id(),
            retryable.revision(),
            HandoffJobTransition::RetryableFailure(
                HandoffFailureEvidence::new(HandoffFailureKind::RootUnavailable, None).unwrap(),
            ),
        )
        .unwrap();
    support::discussion_input::committed(&fixture.store, transition.contribution());
    let fence = fixture.process.test_fence().unwrap();
    fail_read(&fixture);
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let summary = fixture
        .operations
        .converge_candidate(
            &access,
            &state,
            &syndic,
            limits(),
            support::timestamp(600),
            CommandCancellation::new(),
        )
        .unwrap();
    assert_eq!(
        summary,
        HandoffCandidateConvergenceSummary {
            job_visits: 7,
            transitions: 1
        }
    );
    assert_eq!(
        state
            .durable_jobs()
            .job_candidate(&access, fixture.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::TerminalFailed
    );
    for (index, job) in waiting.into_iter().enumerate() {
        assert_eq!(
            state
                .durable_jobs()
                .job_candidate(&access, job)
                .unwrap()
                .unwrap()
                .lifecycle(),
            if index == 0 {
                BranchHandoffJobLifecycle::RetryableFailed
            } else {
                BranchHandoffJobLifecycle::WaitingResolvingTurn
            }
        );
    }
    assert_eq!(
        fixture
            .operations
            .converge_candidate(
                &access,
                &state,
                &syndic,
                limits(),
                support::timestamp(600),
                CommandCancellation::new()
            )
            .unwrap(),
        HandoffCandidateConvergenceSummary {
            job_visits: 3,
            transitions: 0
        }
    );
    let store = candidate.publish().unwrap();
    fence.try_reopen(true).unwrap();
    store.close().unwrap();
}

#[test]
fn candidate_finishes_two_step_parent_result_and_rejects_stale_or_cancelled_access() {
    let fixture = parent_execution::start();
    let source = parent_execution::accepted(&fixture);
    parent_execution::finish(&fixture, &source, TurnEndStatus::complete());
    fail_read(&fixture);
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let before = access.home_revision().unwrap();
    let mismatched =
        DiscussionSettlementOperations::new(fixture.process.clone(), NonZeroUsize::new(2).unwrap());
    assert!(matches!(
        mismatched.converge_candidate(
            &access,
            &state,
            &syndic,
            limits(),
            support::timestamp(600),
            CommandCancellation::new()
        ),
        Err(HandoffCandidateConvergenceError::SlotConfigurationMismatch)
    ));
    assert!(matches!(
        fixture.operations.converge_candidate(
            &access,
            &state,
            &syndic,
            limits(),
            support::timestamp(600),
            cancellation
        ),
        Err(HandoffCandidateConvergenceError::Cancelled)
    ));
    assert_eq!(access.home_revision().unwrap(), before);
    assert!(
        fixture
            .operations
            .converge_candidate(
                &access,
                &fixture.state,
                &fixture.syndic,
                limits(),
                support::timestamp(600),
                CommandCancellation::new()
            )
            .is_err()
    );
    assert_eq!(
        fixture
            .operations
            .converge_candidate(
                &access,
                &state,
                &syndic,
                limits(),
                support::timestamp(600),
                CommandCancellation::new()
            )
            .unwrap(),
        HandoffCandidateConvergenceSummary {
            job_visits: 1,
            transitions: 2
        }
    );
    assert_eq!(
        state
            .durable_jobs()
            .job_candidate(&access, fixture.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::Succeeded
    );
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn candidate_leaves_generated_pending_input_for_ordinary_runtime_without_dispatch() {
    let fixture = parent_execution::start();
    let identity = parent_input::identity(&fixture);
    fail_read(&fixture);
    let mut candidate = fixture.store.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let syndic = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    let before = access.home_revision().unwrap();
    assert_eq!(
        fixture
            .operations
            .converge_candidate(
                &access,
                &state,
                &syndic,
                limits(),
                support::timestamp(600),
                CommandCancellation::new()
            )
            .unwrap(),
        HandoffCandidateConvergenceSummary {
            job_visits: 1,
            transitions: 0
        }
    );
    assert_eq!(access.home_revision().unwrap(), before);
    let store = candidate.publish().unwrap();
    assert_eq!(
        syndic
            .turn_state(&store, identity.turn_id(), limit())
            .unwrap()
            .unwrap()
            .dispatch_provenance(),
        TurnDispatchProvenance::Unattempted
    );
    assert_eq!(
        state
            .durable_jobs()
            .job(&store, fixture.job)
            .unwrap()
            .unwrap()
            .lifecycle(),
        BranchHandoffJobLifecycle::StartingParent
    );
    store.close().unwrap();
}
