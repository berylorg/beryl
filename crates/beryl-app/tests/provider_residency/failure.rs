use beryl_app::cas_projection::test_faults::{
    install_provider_fragment_stage_barrier, install_provider_submit_receiver_loss,
    provider_broker_snapshot,
};
use beryl_home_store::test_faults::{FaultController, FaultPoint};

use super::{
    fixture::{LiveHarness, ingress_snapshot},
    server::ObservationSpec,
};

const SMALL_PATTERNS: u64 = 2_000;
const PAUSED_PATTERNS: u64 = 40_000;

pub fn prove_failure_release_and_atomic_visibility() {
    prove_submit_receiver_loss();
    prove_target_abandonment();
    prove_schema_failure();
    prove_fragment_store_failure();
    prove_unknown_outcome_reconciliation();
}

fn prove_submit_receiver_loss() {
    let harness = LiveHarness::new(104);
    let spec = ObservationSpec::new(1, 1);
    let receiver_loss = install_provider_submit_receiver_loss(harness.session());
    let _ = harness.server().send_observation(spec);
    harness.wait_for_target_closed();
    harness.wait_for_page_leases(0);
    let released = provider_broker_snapshot(harness.session());
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.staged_fragments().current(), 0);
    harness.assert_unpublished(spec.sequence);
    drop(receiver_loss);
    harness.close();
}

fn prove_target_abandonment() {
    let mut harness = LiveHarness::new(105);
    let spec = ObservationSpec::new(1, PAUSED_PATTERNS);
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness.server().begin_backpressure(spec);
    barrier.wait_for_stage();
    harness.abandon_target();
    harness.assert_unpublished(spec.sequence);

    harness.server().probe_backpressure();
    harness.server().wait_for_no_pong();
    barrier.release();
    let report = harness.server().finish_backpressure(spec.sequence);
    let _ = ingress_snapshot(&harness, report);
    harness.wait_for_page_leases(0);
    let released = provider_broker_snapshot(harness.session());
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.staged_fragments().current(), 0);
    harness.assert_unpublished(spec.sequence);
    drop(barrier);
    harness.close();
}

fn prove_schema_failure() {
    let harness = LiveHarness::new(106);
    harness.server().send_missing_text(1);
    harness.wait_for_target_closed();
    harness.wait_for_page_leases(0);
    harness.assert_unpublished(1);
    harness.close();
}

fn prove_fragment_store_failure() {
    let harness = LiveHarness::new(107);
    let spec = ObservationSpec::new(1, PAUSED_PATTERNS);
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness.server().begin_backpressure(spec);
    barrier.wait_for_stage();
    let build = harness
        .storage()
        .provider_observation_build(
            &*harness.store(),
            barrier.observation_id(),
            super::syndic::point_limit(),
        )
        .unwrap()
        .unwrap();
    let corruption = harness
        .storage()
        .current_corrupt_provider_observation(
            &build,
            syndic_storage::test_faults::ProviderObservationCorruption::BuildDigest,
        )
        .unwrap();
    match harness.store().execute_current(corruption) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome @ beryl_home_store::CommandOutcome::NotCommitted { .. } => {
            panic!("expected committed corruption injection, got {outcome:?}")
        }
        outcome @ beryl_home_store::CommandOutcome::Committed {
            later_failure: Some(_),
            ..
        } => panic!("unexpected later failure: {outcome:?}"),
        outcome @ beryl_home_store::CommandOutcome::Indeterminate { .. } => {
            panic!("indeterminate corruption injection: {outcome:?}")
        }
    }
    barrier.release();

    harness.wait_for_target_closed();
    assert_eq!(
        harness.store().health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    harness.wait_for_page_leases(0);
    let released = provider_broker_snapshot(harness.session());
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.staged_fragments().current(), 0);
    harness.assert_unpublished(spec.sequence);
    drop(barrier);
    harness.close();
}

fn prove_unknown_outcome_reconciliation() {
    let faults = FaultController::new();
    let harness = LiveHarness::with_faults(109, faults.clone());
    let staged = ObservationSpec::new(1, PAUSED_PATTERNS);
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness.server().begin_backpressure(staged);
    barrier.wait_for_stage();
    faults.fail_next_in_scope(
        FaultPoint::AfterPersist,
        syndic_storage::test_faults::provider_observation_stage_fault_scope(),
    );
    harness.server().probe_backpressure();
    harness.server().wait_for_no_pong();
    let settlement = harness.next_provider_seal_ack();
    barrier.release();
    let report = harness.server().finish_backpressure(staged.sequence);
    harness.wait_for_provider_seal_ack(settlement);
    harness.wait_for_frontier(1);
    let _ = ingress_snapshot(&harness, report);
    harness.assert_frontier(1);
    harness.assert_digest(staged);
    drop(barrier);

    let published = ObservationSpec::new(2, SMALL_PATTERNS);
    faults.fail_next_in_scope(
        FaultPoint::AfterPersist,
        syndic_storage::test_faults::live_source_event_fault_scope(),
    );
    let _ = harness.send(published, 2);
    harness.wait_for_broker_idle();
    harness.assert_frontier(2);
    harness.assert_digest(published);
    harness.wait_for_page_leases(0);
    let released = provider_broker_snapshot(harness.session());
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.in_flight().high_water(), 1);
    assert_eq!(released.staged_fragments().current(), 0);
    assert_eq!(released.staged_fragments().high_water(), 1);
    harness.close();
}
