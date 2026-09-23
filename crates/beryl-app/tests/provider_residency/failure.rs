use beryl_app::cas_projection::test_faults::{
    ProviderBrokerSnapshotReader, capture_provider_broker_snapshot_reader,
    install_provider_fragment_stage_barrier, install_provider_submit_receiver_loss,
    provider_broker_snapshot,
};
use beryl_home_store::test_faults::{FaultController, FaultPoint};

use super::{fixture::LiveHarness, server::ObservationSpec};

const SMALL_PATTERNS: u64 = 2_000;
const PAUSED_PATTERNS: u64 = 40_000;

pub(super) fn prove_submit_receiver_loss() {
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

pub(super) fn prove_target_abandonment() {
    let mut harness = LiveHarness::new(105);
    let spec = ObservationSpec::new(1, PAUSED_PATTERNS);
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness.server().begin_backpressure(spec);
    barrier.wait_for_stage();
    harness.assert_unpublished(spec.sequence);

    harness.server().probe_backpressure_prefix();
    harness.server().wait_for_no_pong();
    barrier.release();
    harness.server().wait_for_resumed_prefix();
    // The server withholds the suffix, so disposal cannot race publication at seal.
    harness.abandon_target();
    harness.wait_for_page_leases(0);
    let released = provider_broker_snapshot(harness.session());
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.staged_fragments().current(), 0);
    harness.assert_unpublished(spec.sequence);
    drop(barrier);
    harness.close();
}

pub(super) fn prove_schema_failure() {
    let harness = LiveHarness::new(106);
    harness.server().send_missing_text(1);
    harness.wait_for_target_closed();
    harness.wait_for_page_leases(0);
    harness.assert_unpublished(1);
    harness.close();
}

pub(super) fn prove_fragment_store_failure() {
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

pub(super) fn prove_indeterminate_staging() {
    let faults = FaultController::new();
    let harness = LiveHarness::with_faults(109, faults.clone());
    let reader = capture_provider_broker_snapshot_reader(harness.session());
    let spec = ObservationSpec::new(1, PAUSED_PATTERNS);
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness.server().begin_backpressure(spec);
    barrier.wait_for_stage();
    harness.assert_unpublished(spec.sequence);
    faults.fail_next_in_scope(
        FaultPoint::AfterCommitBeforePersist,
        syndic_storage::test_faults::provider_observation_stage_fault_scope(),
    );
    barrier.release();
    let custody = assert_terminal_custody(&harness, &reader);
    harness.assert_unpublished(spec.sequence);
    drop(barrier);
    harness.close_with_reconciliation(custody);
}

pub(super) fn prove_indeterminate_publication() {
    let faults = FaultController::new();
    let harness = LiveHarness::with_faults(110, faults.clone());
    let reader = capture_provider_broker_snapshot_reader(harness.session());
    let spec = ObservationSpec::new(1, SMALL_PATTERNS);
    faults.fail_next_in_scope(
        FaultPoint::AfterCommitBeforePersist,
        syndic_storage::test_faults::live_source_event_fault_scope(),
    );
    let _ = harness.server().send_observation(spec);
    let custody = assert_terminal_custody(&harness, &reader);
    harness.assert_frontier(1);
    harness.assert_digest(spec);
    harness.close_with_reconciliation(custody);
}

fn assert_terminal_custody(
    harness: &LiveHarness,
    reader: &ProviderBrokerSnapshotReader,
) -> beryl_home_store::ReconciliationHandle {
    harness.wait_for_target_closed();
    let mut scopes = harness.store().pending_reconciliations();
    assert_eq!(scopes.len(), 1);
    let custody = scopes.pop().unwrap();
    harness.wait_for_page_leases(0);
    harness.session().invalidate_connection();
    assert_eq!(
        harness.store().health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    assert_eq!(harness.store().pending_reconciliations().len(), 1);
    let released = reader.snapshot();
    assert_eq!(released.in_flight().current(), 0);
    assert_eq!(released.staged_fragments().current(), 0);
    custody
}
