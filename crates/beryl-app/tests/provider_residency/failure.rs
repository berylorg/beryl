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

pub(super) fn prove_passive_store_failure_capture() {
    use beryl_app::cas_projection::test_faults::OutageCaptureState;
    let faults = FaultController::new();
    let harness = LiveHarness::with_faults(113, faults.clone());
    let observer = harness.outage_observer();
    let reader = capture_provider_broker_snapshot_reader(harness.session());
    let settlement = harness.next_provider_seal_ack();
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness
        .server()
        .begin_backpressure(ObservationSpec::new(1, PAUSED_PATTERNS));
    barrier.wait_for_stage();
    faults.fail_next_in_scope(
        FaultPoint::AfterPersist,
        syndic_storage::test_faults::provider_observation_stage_fault_scope(),
    );
    barrier.release();
    let deadline = std::time::Instant::now() + super::server::TIMEOUT;
    loop {
        let snapshot = observer.snapshot().unwrap();
        if snapshot.state == OutageCaptureState::Ready {
            assert_eq!(
                snapshot.targets, 1,
                "active-only targets also participate in capture"
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "inventory did not publish: {snapshot:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    harness.server().finish_pending(1);
    harness.wait_for_provider_seal_ack(settlement);
    let snapshot = observer.snapshot().unwrap();
    assert_eq!(snapshot.gapped_targets, 1);
    assert_eq!(
        snapshot.facts, 0,
        "spanning observation cannot publish its suffix"
    );
    let batches = reader.snapshot().provider_staging_batches();
    for sequence in 2..=3 {
        let settlement = harness.next_provider_seal_ack();
        harness
            .server()
            .send_observation(ObservationSpec::new(sequence, 64));
        harness.wait_for_provider_seal_ack(settlement);
    }
    let retained = observer.snapshot().unwrap();
    assert!(
        retained.facts > 0,
        "ordinary failed-gate polls must continue receiving"
    );
    assert!(retained.encoded_bytes <= 4 * 1024 * 1024);
    let settlement = harness.next_provider_seal_ack();
    harness
        .server()
        .send_observation(ObservationSpec::new(4, PAUSED_PATTERNS));
    harness.wait_for_provider_seal_ack(settlement);
    assert_eq!(
        observer.snapshot().unwrap().facts,
        retained.facts,
        "oversized observation is wholly lost"
    );
    assert_eq!(
        reader.snapshot().provider_staging_batches(),
        batches,
        "passive receipt must not retry storage"
    );
    harness.wait_for_page_leases(0);
    drop(barrier);
    harness.close_failed();
    assert!(observer.snapshot().is_none_or(|snapshot| snapshot.state
        == OutageCaptureState::Unavailable
        && snapshot.facts == 0));
}

pub(super) fn prove_dispatched_request_failure_capture() {
    use beryl_app::cas_projection::test_faults::{OutageCaptureState, start_outage_probe_request};
    let faults = FaultController::new();
    let harness = LiveHarness::with_faults(114, faults.clone());
    let observer = harness.outage_observer();
    let request = start_outage_probe_request(harness.session());
    harness.server().await_request();
    let settlement = harness.next_provider_seal_ack();
    let barrier = install_provider_fragment_stage_barrier(harness.session());
    harness
        .server()
        .begin_backpressure(ObservationSpec::new(1, PAUSED_PATTERNS));
    barrier.wait_for_stage();
    faults.fail_next_in_scope(
        FaultPoint::AfterPersist,
        syndic_storage::test_faults::provider_observation_stage_fault_scope(),
    );
    barrier.release();
    harness.server().finish_pending(1);
    harness.wait_for_provider_seal_ack(settlement);
    let settlement = harness.next_provider_seal_ack();
    harness
        .server()
        .send_observation(ObservationSpec::new(2, 64));
    harness.wait_for_provider_seal_ack(settlement);
    assert_eq!(
        observer.snapshot().unwrap().state,
        OutageCaptureState::Pending,
        "the dispatched request still owns its command permit"
    );
    assert!(!request.is_finished());
    harness.server().reject_request();
    let result = request.join().unwrap().unwrap_err();
    assert!(
        format!("{result:?}").contains("outage probe rejection"),
        "original backend outcome was replaced: {result:?}"
    );
    let deadline = std::time::Instant::now() + super::server::TIMEOUT;
    loop {
        let snapshot = observer.snapshot().unwrap();
        if snapshot.state == OutageCaptureState::Ready && snapshot.facts > 0 {
            assert_eq!(snapshot.targets, 1);
            assert_eq!(snapshot.gapped_targets, 1);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "pending observation did not flush after request settlement: {snapshot:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    drop(barrier);
    harness.close_failed();
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
