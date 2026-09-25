use super::*;
use beryl_app::cas_projection::test_faults::{
    AcquisitionBarrierStage, install_acquisition_barrier,
};

#[test]
fn native_preflight_conflicts_retain_one_worker_and_loaded_projection() {
    let (mut fixture, faults, slot, cas_thread_id) = scheduler_fixture(227, 4);
    let server = NativeLineageServer::spawn_retry_success(cas_thread_id);
    attach_session(&fixture, &slot, &server, 227_001);
    let ids = admit_pending_turn(&mut fixture, &faults, 227);
    server.wait_for_first_resume();
    let pending_turn = promoted_pending_turn(&fixture.store, &fixture.storage, &ids);
    server.release_first_resume();
    server.wait_for_initial_retries();
    let route = wait_for_parked_route(&fixture);
    wait_until("parked native worker release", || {
        (fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_active()
            == 0)
            .then_some(())
    });

    let scanned = fixture
        .store
        .accepted_input_scheduler_diagnostics()
        .recovered_pending_source_page_reads();
    fixture.store.notify_scheduled_ordinary_execution_ready();
    wait_until("pending scan exhausted behind parked native route", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (diagnostics.recovered_pending_source_page_reads() > scanned
            && !diagnostics.recovered_pending_retained_source_cursor()
            && diagnostics.workers_active() == 0)
            .then_some(())
    });
    let before = fixture.store.accepted_input_scheduler_diagnostics();
    let generation = fixture.store.service_generation();
    let first = install_acquisition_barrier(
        generation,
        AcquisitionBarrierStage::OrdinaryPreflightConfirmation,
    );
    let control = fixture.store.native_lineage_recovery_control();
    control
        .submit(route.key(), NativeLineageRecoveryCommand::Retry)
        .unwrap();
    first.wait();
    assert_pending_unchanged(&fixture, &ids, pending_turn);
    assert_single_retry_worker(&fixture, before.workers_started());
    fixture.advance_unrelated_syndic_revision(229);
    let second = install_acquisition_barrier(
        generation,
        AcquisitionBarrierStage::OrdinaryPreflightConfirmation,
    );
    first.release();
    second.wait();
    drop(first);
    assert_pending_unchanged(&fixture, &ids, pending_turn);
    assert_single_retry_worker(&fixture, before.workers_started());
    assert!(matches!(
        control
            .snapshot_for_thread(fixture.thread)
            .unwrap()
            .status(),
        NativeLineageRecoveryStatus::Leaving {
            pending_turn_continues: true
        }
    ));
    fixture.advance_unrelated_syndic_revision(231);
    second.release();
    drop(second);

    server.wait_for_turn_start();
    wait_for_turn_complete(&fixture, pending_turn);
    wait_until("retained native retry worker joined", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_active() == 0
            && diagnostics.workers_started() == before.workers_started() + 1)
            .then_some(())
    });
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, &ids),
        pending_turn
    );
    let after = fixture.store.accepted_input_scheduler_diagnostics();
    assert_eq!(after.workers_started(), before.workers_started() + 1);
    assert!(!after.fatal());
    assert_eq!(server.resume_request_count(), 4);
    control.acknowledge_leaving(route.key()).unwrap();
    close_fixture(fixture, &slot, server);
}

fn assert_single_retry_worker(fixture: &syndic::Fixture, previous_workers: u64) {
    let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
    assert_eq!(diagnostics.workers_active(), 1);
    assert_eq!(diagnostics.workers_started(), previous_workers + 1);
}

fn assert_pending_unchanged(fixture: &syndic::Fixture, ids: &NextRecordIds, turn: SyndicTurnId) {
    assert_eq!(
        promoted_pending_turn(&fixture.store, &fixture.storage, ids),
        turn
    );
    let command = fixture.store.live_home_command().unwrap();
    let state = fixture
        .storage
        .turn_state(command.home(), turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    assert_eq!(state.source_event_count(), 0);
}
