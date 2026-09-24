use super::*;
use beryl_app::discussion_handoff_limits::{HandoffScanConfiguration, HandoffScanLimits};
use std::time::{Duration, Instant};

fn limits() -> HandoffScanLimits {
    HandoffScanConfiguration {
        handoff_recovery_page_items: 2,
        handoff_recovery_page_encoded_bytes: beryl_state::HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES,
        handoff_job_record_encoded_bytes: beryl_state::HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
        handoff_reconcile_slots: 1,
        handoff_ready_job_items: 1,
    }
    .try_into()
    .unwrap()
}

fn wait(mut predicate: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < until,
            "handoff did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn job(f: &Fixture) -> beryl_state::BranchHandoffJobRecord {
    f.state
        .durable_jobs()
        .job(&f.store, f.job)
        .unwrap()
        .unwrap()
}

fn add_waiting(f: &Fixture, seed: u8) {
    support::discussion_creation::create_child(&f.store, &f.syndic, id(30), seed, seed + 1);
    let request = support::discussion_handoff::active_request_for(
        &f.store,
        &f.syndic,
        id(seed),
        support::draft_id(seed + 2),
        beryl_model::SyndicItemId::from_bytes([seed + 3; 16]),
        ResolutionIntentId::from_bytes([seed + 4; 16]),
        JobId::from_bytes([seed + 5; 16]),
    );
    admit_request(&f.store, &f.state, &f.syndic, request);
}

#[test]
fn publication_fence_and_repeated_wakes_preserve_one_generated_input() {
    let f = Fixture::new(false);
    parent_input::ready(&f);
    let mut coordinator = HandoffCoordinatorTestHarness::prepare(f.service(), limits()).unwrap();
    for _ in 0..3 {
        coordinator.wake();
    }
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        job(&f).lifecycle(),
        BranchHandoffJobLifecycle::WaitingParent
    );
    assert_eq!(coordinator.progress().0, 0);
    coordinator.release();
    wait(|| job(&f).lifecycle() == BranchHandoffJobLifecycle::StartingParent);
    let original = job(&f);
    let passes = coordinator.progress().0;
    for _ in 0..10 {
        coordinator.wake();
    }
    wait(|| coordinator.progress().0 > passes);
    assert_eq!(job(&f), original);
    assert_eq!(coordinator.progress().2, 1);
    coordinator.shutdown().unwrap();
    f.store.close().unwrap();
}

#[test]
fn backlog_exceeds_ready_capacity_and_behind_cursor_change_survives_capacity_wait() {
    let f = Fixture::new(false);
    for seed in [220, 230, 240] {
        add_waiting(&f, seed);
    }
    let first = f
        .state
        .durable_jobs()
        .list_live(&f.store, None, limits().page())
        .unwrap()
        .records()[0]
        .job_id();
    assert_eq!(first, f.job);
    let mut coordinator = HandoffCoordinatorTestHarness::prepare(f.service(), limits()).unwrap();
    coordinator.pause_at(f.job, false);
    coordinator.release();
    wait(|| coordinator.paused());
    let occupied = coordinator.occupy_slot(JobId::from_bytes([250; 16]));
    f.finish_child();
    coordinator.resume();
    wait(|| coordinator.progress().1 >= 2);
    drop(occupied);
    wait(|| job(&f).lifecycle() == BranchHandoffJobLifecycle::WaitingParent);
    wait(|| coordinator.progress().0 >= 2);
    assert_eq!(coordinator.progress().2, 1);
    coordinator.shutdown().unwrap();
    f.store.close().unwrap();
}

#[test]
fn concurrent_nondispatch_pauses_without_killing_scanner_or_automatic_retry() {
    let f = parent_execution::start();
    let mut coordinator = HandoffCoordinatorTestHarness::prepare(f.service(), limits()).unwrap();
    coordinator.pause_at(f.job, true);
    coordinator.release();
    wait(|| coordinator.paused());
    let mut proof = f
        .service()
        .reserve_parent_dispatch(f.job, id(30), parent_input::identity(&f).turn_id())
        .unwrap()
        .preparation_failed_for_test(beryl_state::HandoffFailureKind::RuntimeUnavailable)
        .unwrap();
    assert!(matches!(
        proof.prepare(CommandCancellation::new()).unwrap().execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    drop(proof);
    let paused = job(&f);
    coordinator.resume();
    wait(|| coordinator.progress().0 >= 2);
    let passes = coordinator.progress().0;
    coordinator.wake();
    wait(|| coordinator.progress().0 > passes);
    assert_eq!(job(&f), paused);
    assert!(matches!(
        f.service()
            .prepare_retry(f.job, paused.revision(), CommandCancellation::new())
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed { .. }
    ));
    wait(|| job(&f).lifecycle() == BranchHandoffJobLifecycle::StartingParent);
    coordinator.shutdown().unwrap();
    f.store.close().unwrap();
}

#[test]
fn disposal_joins_unpublished_and_capacity_blocked_workers() {
    let f = Fixture::new(false);
    let mut unpublished = HandoffCoordinatorTestHarness::prepare(f.service(), limits()).unwrap();
    unpublished.shutdown().unwrap();
    drop(unpublished);
    let mut coordinator = HandoffCoordinatorTestHarness::prepare(f.service(), limits()).unwrap();
    let occupied = coordinator.occupy_slot(JobId::from_bytes([250; 16]));
    coordinator.release();
    wait(|| coordinator.progress().1 >= 1);
    coordinator.shutdown().unwrap();
    drop(occupied);
    assert_eq!(
        job(&f).lifecycle(),
        BranchHandoffJobLifecycle::WaitingResolvingTurn
    );
    f.store.close().unwrap();
}

#[test]
fn uncertain_command_remains_owned_after_coordinator_disposal() {
    let f = Fixture::new(true);
    f.finish_child();
    let mut coordinator = HandoffCoordinatorTestHarness::prepare(f.service(), limits()).unwrap();
    f.faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    coordinator.release();
    wait(|| f.operations.retained_audit(f.job).is_some());
    assert!(coordinator.shutdown().is_err());
    drop(coordinator);
    assert!(f.operations.retained_audit(f.job).is_some());
}
