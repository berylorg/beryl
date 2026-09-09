use std::path::Path;

use beryl_backend::ManagedBackendClientConnector;
use beryl_home_store::test_faults::FaultController;
use beryl_model::CasProcessGeneration;
use syndic_storage::TurnLifecycle;

use crate::{support::*, syndic};

#[test]
fn fresh_execution_wake_revisits_retained_pending_work_and_dispatches_the_existing_turn() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_and_faults(
        181,
        FaultController::new(),
        move |assets| Box::new(ready_provider(provider_slot, assets)),
    );
    let parent = fixture.submit_text(" completed parent");
    fixture.complete_with_assistant(parent, " completed answer");
    let storage = fixture.storage.clone();
    let thread = fixture.thread;
    let cas_thread = {
        let command = fixture.store.live_home_command().unwrap();
        current_cas_thread_id(command.home(), &storage, thread)
    };
    let unavailable_before = fixture
        .store
        .accepted_input_scheduler_diagnostics()
        .recovered_pending_execution_unavailable();
    let pending = fixture.submit_text(SUBMITTED_TEXT);
    fixture.store.notify_scheduled_ordinary_execution_ready();
    wait_until(
        "pending source retained without execution authority",
        || {
            let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
            (diagnostics.recovered_pending_execution_unavailable() > unavailable_before
                && !diagnostics.recovered_pending_retained_source_cursor()
                && diagnostics.workers_active() == 0)
                .then_some(())
        },
    );
    {
        let command = fixture.store.live_home_command().unwrap();
        let state = storage
            .turn_state(command.home(), pending.turn, point_limit())
            .unwrap()
            .unwrap();
        assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
        assert_eq!(state.source_event_count(), 0);
        assert!(
            storage
                .non_idle_gate_source(command.home(), thread, point_limit())
                .unwrap()
                .is_some()
        );
    }

    let server = NormalTerminalServer::spawn_resume_terminal(cas_thread);
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            syndic::execution_binding().runtime_id(),
            CasProcessGeneration::new(62_181).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    fixture.store.notify_scheduled_ordinary_execution_ready();
    server.wait_for_projection();
    wait_until(
        "existing pending turn completes through scheduled dispatch",
        || {
            let command = fixture.store.live_home_command().ok()?;
            let state = storage
                .turn_state(command.home(), pending.turn, point_limit())
                .ok()??;
            (state.lifecycle() == TurnLifecycle::Complete).then_some(())
        },
    );
    wait_until("pending worker and cursor released", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_active() == 0
            && !diagnostics.recovered_pending_retained_source_cursor()
            && slot.is_ready())
        .then_some(())
    });
    {
        let command = fixture.store.live_home_command().unwrap();
        assert_eq!(
            storage
                .thread(command.home(), thread, point_limit())
                .unwrap()
                .unwrap()
                .committed_tail(),
            Some(pending.turn)
        );
        assert!(
            storage
                .non_idle_gate_source(command.home(), thread, point_limit())
                .unwrap()
                .is_none()
        );
    }
    let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovered_pending_source_page_reads() >= 2);
    assert!(!diagnostics.fatal());
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}
