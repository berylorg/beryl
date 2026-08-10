use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use beryl_app::cas_projection::{
    ScheduledOrdinaryAdmission, ScheduledOrdinaryAdmissionError, ScheduledOrdinaryAdmissionResult,
    ScheduledOrdinaryExecutionProvider, ScheduledOrdinaryExecutionUnavailable,
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_home_store::CursorReadLimits;
use beryl_model::{
    CasProcessGeneration, RuntimeId, SyndicAcceptedInputId, SyndicDraftId, SyndicThreadId,
};
use serde_json::json;
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, AcceptedInputAdmission, AcceptedInputLifecycle,
    AcceptedNextCandidate, AcceptedRouteEffectiveState, AcceptedRouteEntry,
    BeginAcceptedInputDelivery, BindingState, ClaimStopDispatch, CompleteAcceptedInputDelivery,
    InputGateRecord, InputGateState, NextTurnReason, PendingSteeringTargetProof,
    RecoveryProjectionRequest, SelectedPathProof, SteeringTargetProof, StopAdmissionRead,
    StopAttemptNonce, StopCause, StopCauseSet, StopOperationNonce, StopOperationState,
    SyndicStorage, TurnIncompleteReason, TurnLifecycle,
};

use crate::{
    app_support::{
        close_seeded, execute, point_limit, promote_installed_next, restart_service,
        restart_service_with, seeded_home, time,
    },
    phase62_support::{
        AUTHORIZATION, NormalTerminalServer, SessionSlot, TIMEOUT, UnavailableProvider,
        accepted_route_state, admit_runtime_awaiting_terminal_input, execution_binding,
        install_next_records, ready_provider, try_accepted_route_state,
    },
    records::activate_promoted_turn,
    support as storage_support,
};

struct CountingUnavailableProvider {
    attempts: Arc<AtomicUsize>,
}

impl ScheduledOrdinaryExecutionProvider for CountingUnavailableProvider {
    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {}
}

fn route_entry(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    input_id: SyndicAcceptedInputId,
) -> AcceptedRouteEntry {
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let route = gate
        .selected_route()
        .expect("accepted input retains selected route authority");
    storage
        .accepted_route_page(store, thread_id, route.generation(), route.revision(), None)
        .unwrap()
        .records()
        .iter()
        .find(|entry| entry.input().id() == input_id)
        .unwrap()
        .clone()
}

fn queued_candidate(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
) -> Result<Option<AcceptedNextCandidate>, syndic_storage::SyndicReadError> {
    let revision = storage.revision(store)?;
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let mut source_cursor = None;
    loop {
        let page = storage.accepted_next_source_page(store, revision, source_cursor, limits)?;
        for source in page.records() {
            if source.thread_id() != thread_id {
                continue;
            }
            let mut candidate_cursor = None;
            loop {
                let page = storage.accepted_next_candidate_page(
                    store,
                    *source,
                    candidate_cursor,
                    limits,
                )?;
                let next_cursor = page.next_cursor();
                if let Some(candidate) = page.into_candidate() {
                    return Ok(Some(candidate));
                }
                candidate_cursor = next_cursor;
                if candidate_cursor.is_none() {
                    return Ok(None);
                }
            }
        }
        source_cursor = page.next_cursor();
        if source_cursor.is_none() {
            return Ok(None);
        }
    }
}

fn replace_gate_state(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    state: InputGateState,
) {
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let replacement = InputGateRecord::new(
        thread_id,
        gate.revision().checked_next().unwrap(),
        state,
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    storage_support::commit(
        store,
        storage,
        storage_support::batch([FixtureRecord::InputGate(replacement)]),
    );
}

fn admit_current_input(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    seed: u8,
) -> AcceptedInputAdmission {
    let current = storage
        .current_draft(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        thread_id,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([seed; 16]),
        None,
        time(63_410),
    );
    execute(
        store,
        storage.admit_accepted_input(storage.revision(store).unwrap(), admission.clone()),
    );
    admission
}

fn assert_authority_lost_predecessor(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    active_turn: beryl_model::SyndicTurnId,
) {
    let state = storage
        .turn_state(store, active_turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(
        state.end_status().unwrap().incomplete_reason(),
        Some(TurnIncompleteReason::AuthorityLost)
    );
}

#[test]
fn admitted_and_claimed_stops_abandon_on_restart_without_losing_next_turn_input() {
    for (seed, claim_dispatch) in [(201_u8, false), (202_u8, true)] {
        let home = seeded_home();
        let ids = install_next_records(
            &home.store,
            home.storage,
            seed,
            execution_binding(RuntimeId::from_bytes([seed.wrapping_add(40); 16])),
        );
        let promoted = promote_installed_next(
            &home.store,
            home.storage,
            &home.state,
            ids,
            seed.wrapping_add(20),
        );
        activate_promoted_turn(
            &home.store,
            home.storage,
            ids.thread,
            promoted.turn,
            seed.wrapping_add(40),
            true,
        );
        let candidate = match home
            .storage
            .stop_admission_read(&home.store, ids.thread, point_limit())
            .unwrap()
        {
            StopAdmissionRead::Admissible(candidate) => candidate,
            other => panic!("active restart fixture must admit stop, observed {other:?}"),
        };
        let admission = candidate.admission(
            StopOperationNonce::from_bytes([seed.wrapping_add(60); 16]),
            StopCauseSet::from(StopCause::SelectedOperationControl),
        );
        let operation_id = admission.operation_id();
        home.store
            .execute_current(home.storage.current_admit_stop_operation(admission.clone()))
            .unwrap();
        if claim_dispatch {
            let live = match home
                .storage
                .stop_admission_read(&home.store, ids.thread, point_limit())
                .unwrap()
            {
                StopAdmissionRead::Stopping(live) => *live,
                other => panic!("admitted restart fixture must be stopping, observed {other:?}"),
            };
            let claim = ClaimStopDispatch::new(
                operation_id,
                live.target().clone(),
                live.current_gate_revision(),
                live.stop_revision(),
                StopAttemptNonce::from_bytes([seed.wrapping_add(80); 16]),
            );
            home.store
                .execute_current(home.storage.current_claim_stop_dispatch(claim))
                .unwrap();
            let claimed = match home
                .storage
                .stop_admission_read(&home.store, ids.thread, point_limit())
                .unwrap()
            {
                StopAdmissionRead::Stopping(live) => live,
                other => panic!("claimed restart fixture must be stopping, observed {other:?}"),
            };
            assert_eq!(claimed.state(), StopOperationState::DispatchClaimed);
        }

        let queued = admit_current_input(
            &home.store,
            home.storage,
            ids.thread,
            seed.wrapping_add(100),
        )
        .accepted_input_id();
        let queued_ids = crate::phase62_support::NextRecordIds {
            thread: ids.thread,
            accepted_input: queued,
            parent: ids.parent,
        };
        assert_eq!(
            accepted_route_state(&home.store, home.storage, &queued_ids),
            AcceptedRouteEffectiveState::NextTurn(NextTurnReason::Stop)
        );
        home.store.validate_registered_domains().unwrap();
        let directory = close_seeded(home);

        let (service, storage, _) =
            restart_service(directory.path(), Box::new(UnavailableProvider));
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        assert!(diagnostics.recovery_handed_off());
        assert_eq!(diagnostics.startup_recovery_cases(), 1);
        assert_eq!(diagnostics.startup_active_convergences(), 1);
        assert_eq!(diagnostics.startup_terminal_convergences(), 1);
        assert!(!diagnostics.fatal());
        {
            let command_home = service.live_home_command().unwrap();
            let home = command_home.home();
            assert_authority_lost_predecessor(home, storage, promoted.turn);
            let binding = storage
                .current_binding(home, ids.thread, point_limit())
                .unwrap()
                .unwrap();
            assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
            let queued_after_restart = queued_candidate(home, storage, ids.thread)
                .unwrap()
                .expect("stop-period input remains scheduled next-turn work");
            assert_eq!(queued_after_restart.input_id(), queued);
            assert_eq!(
                queued_after_restart.next_turn_reason(),
                NextTurnReason::Stop
            );
            assert_eq!(
                try_accepted_route_state(home, storage, &queued_ids)
                    .unwrap()
                    .unwrap(),
                AcceptedRouteEffectiveState::NextTurn(NextTurnReason::Stop)
            );
        }
        service.close().unwrap();
    }
}

#[test]
fn awaiting_terminal_authority_closes_before_queued_next_turn_starts() {
    const COMPLETED_PARENT_INPUT: &str = "phase63 completed parent request";
    const COMPLETED_PARENT_OUTPUT: &str = "phase63 completed parent answer";
    const INTERRUPTED_PREDECESSOR_INPUT: &str = "phase63 interrupted predecessor request";

    let mut fixture = crate::syndic::Fixture::new(190);
    let completed = fixture.submit_text(COMPLETED_PARENT_INPUT);
    fixture.complete_with_assistant(completed, COMPLETED_PARENT_OUTPUT);
    let active = fixture.submit_text(INTERRUPTED_PREDECESSOR_INPUT);
    let source = fixture.activate_without_terminal(active);
    fixture.mark_active_unknown_terminal(active, &source);
    let execution = crate::syndic::execution_binding();
    let runtime_id = execution.runtime_id();
    let ids = admit_runtime_awaiting_terminal_input(&mut fixture, 190);
    assert_eq!(ids.parent, active.turn);
    {
        let command_home = fixture.store.live_home_command().unwrap();
        let home = command_home.home();
        assert!(
            queued_candidate(home, fixture.storage, ids.thread)
                .unwrap()
                .is_none(),
            "AwaitingTerminal keeps accepted-next work fenced until active authority converges"
        );
        assert_eq!(
            fixture
                .storage
                .input_gate(home, ids.thread, point_limit())
                .unwrap()
                .unwrap()
                .state(),
            &InputGateState::AwaitingTerminal(active.turn)
        );
        home.validate_registered_domains().unwrap();
    }
    let (directory, initial_service) = fixture.into_service();
    initial_service.close().unwrap();

    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let (service, storage, _) = restart_service_with(directory.path(), |state| {
        Box::new(ready_provider(provider_slot, state.assets()))
    });
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 1);
    assert_eq!(diagnostics.startup_active_convergences(), 1);
    assert_eq!(diagnostics.startup_terminal_convergences(), 1);
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        let recovered_binding = storage
            .current_binding(home, ids.thread, point_limit())
            .unwrap()
            .unwrap();
        assert!(matches!(
            recovered_binding.binding().state(),
            BindingState::Stale(_)
        ));
        assert_authority_lost_predecessor(home, storage, active.turn);
        let queued = queued_candidate(home, storage, ids.thread)
            .unwrap()
            .expect("queued stop input remains pending until execution authority arrives");
        assert_eq!(queued.input_id(), ids.accepted_input);
        assert_eq!(queued.next_turn_reason(), NextTurnReason::UnknownTerminal);
    }

    let server = NormalTerminalServer::spawn_recovery_terminal(vec![
        json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": COMPLETED_PARENT_INPUT}],
        }),
        json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": COMPLETED_PARENT_OUTPUT}],
        }),
        json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": INTERRUPTED_PREDECESSOR_INPUT}],
        }),
    ]);
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            runtime_id,
            CasProcessGeneration::new(63_411).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    let before_execution = service.accepted_input_scheduler_diagnostics();
    service.notify_scheduled_ordinary_execution_ready();
    let activation = crate::phase62_support::wait_until("queued stop durable promotion", || {
        let command_home = service.live_home_command().ok()?;
        let gate = storage
            .input_gate(command_home.home(), ids.thread, point_limit())
            .ok()
            .flatten()?;
        if gate.state() != &InputGateState::Idle {
            return Some(Ok(gate.state().clone()));
        }
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.fatal()
            || diagnostics.next_execution_unavailable()
                > before_execution.next_execution_unavailable()
            || diagnostics.workers_joined() > before_execution.workers_joined())
        .then_some(Err(diagnostics))
    });
    assert!(
        activation.is_ok(),
        "queued stop promotion parked or failed: {activation:?}"
    );
    let recovery_preflight = {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        let thread = storage
            .thread(home, ids.thread, point_limit())
            .unwrap()
            .unwrap();
        let selected_path = SelectedPathProof::new(
            thread.committed_tail(),
            thread.revision(),
            thread.selected_path_digest(),
        );
        storage.prepare_recovery_projection(
            home,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                ids.thread,
                selected_path,
                Some(2_000_000),
            ),
        )
    };
    assert!(
        recovery_preflight.is_ok(),
        "queued stop recovery preflight failed: {recovery_preflight:?}"
    );
    let promoted_diagnostics = service.accepted_input_scheduler_diagnostics();
    let dispatch = crate::phase62_support::wait_until("queued stop durable activation", || {
        let command_home = service.live_home_command().ok()?;
        let binding = storage
            .current_binding(command_home.home(), ids.thread, point_limit())
            .ok()
            .flatten()?;
        if matches!(binding.binding().state(), BindingState::Active(_)) {
            return Some(Ok(()));
        }
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.fatal()
            || diagnostics.workers_joined() > promoted_diagnostics.workers_joined()
            || diagnostics.next_execution_unavailable()
                > promoted_diagnostics.next_execution_unavailable())
        .then_some(Err(diagnostics))
    });
    assert!(
        dispatch.is_ok(),
        "queued stop execution did not activate: {dispatch:?}"
    );
    server.wait_for_projection();
    server.wait_for_turn_start();
    {
        let command_home = service.live_home_command().unwrap();
        assert_authority_lost_predecessor(command_home.home(), storage, active.turn);
    }
    crate::phase62_support::wait_until("queued stop input becomes promoted", || {
        let command_home = service.live_home_command().ok()?;
        queued_candidate(command_home.home(), storage, ids.thread)
            .ok()
            .flatten()
            .is_none()
            .then_some(())
    });
    crate::phase62_support::wait_until("queued stop execution returns its session", || {
        slot.is_ready().then_some(())
    });
    service.close().unwrap();
    server.join();
}

#[test]
fn delivered_steering_leaf_survives_reopen_without_provider_or_backend_replay() {
    let home = seeded_home();
    let ids = install_next_records(
        &home.store,
        home.storage,
        191,
        execution_binding(RuntimeId::from_bytes([251; 16])),
    );
    let promoted = promote_installed_next(&home.store, home.storage, &home.state, ids, 221);
    let active = activate_promoted_turn(
        &home.store,
        home.storage,
        ids.thread,
        promoted.turn,
        251,
        true,
    );
    let delivered_input =
        admit_current_input(&home.store, home.storage, ids.thread, 43).accepted_input_id();
    let admitted = route_entry(&home.store, home.storage, ids.thread, delivered_input);
    assert_eq!(
        admitted.effective_state(),
        AcceptedRouteEffectiveState::Ready
    );
    assert_eq!(
        admitted.leaf().lifecycle(),
        AcceptedInputLifecycle::Admitted
    );

    let binding = home
        .storage
        .current_binding(&home.store, ids.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(_) = binding.binding().state() else {
        panic!("delivered restart fixture must retain active authority");
    };
    let target = SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            binding.binding().revision(),
            active.snapshot,
            promoted.turn,
            active.cas_thread.clone(),
        ),
        active
            .cas_turn
            .clone()
            .expect("delivered steering fixture publishes a CAS turn"),
    );
    home.store
        .execute_current(home.storage.current_begin_accepted_input_delivery(
            BeginAcceptedInputDelivery::new(
                ids.thread,
                delivered_input,
                admitted.leaf().revision(),
                target.clone(),
            ),
        ))
        .unwrap();
    let delivering = route_entry(&home.store, home.storage, ids.thread, delivered_input);
    assert_eq!(
        delivering.effective_state(),
        AcceptedRouteEffectiveState::Delivering
    );
    home.store
        .execute_current(home.storage.current_complete_accepted_input_delivery(
            CompleteAcceptedInputDelivery::new(
                ids.thread,
                delivered_input,
                delivering.leaf().revision(),
                target,
            ),
        ))
        .unwrap();
    let delivered = route_entry(&home.store, home.storage, ids.thread, delivered_input);
    assert_eq!(
        delivered.effective_state(),
        AcceptedRouteEffectiveState::Delivered
    );
    assert_eq!(
        delivered.leaf().lifecycle(),
        AcceptedInputLifecycle::Delivered
    );
    home.store.validate_registered_domains().unwrap();
    let directory = close_seeded(home);

    let first_attempts = Arc::new(AtomicUsize::new(0));
    let (service, storage, _) = restart_service(
        directory.path(),
        Box::new(CountingUnavailableProvider {
            attempts: Arc::clone(&first_attempts),
        }),
    );
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 1);
    assert_eq!(diagnostics.startup_active_convergences(), 1);
    assert_eq!(diagnostics.startup_terminal_convergences(), 1);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(first_attempts.load(Ordering::SeqCst), 0);
    {
        let command_home = service.live_home_command().unwrap();
        let reopened = route_entry(command_home.home(), storage, ids.thread, delivered_input);
        assert_eq!(reopened.leaf(), delivered.leaf());
        assert_eq!(
            reopened.effective_state(),
            AcceptedRouteEffectiveState::Delivered
        );
        assert_eq!(
            accepted_route_state(command_home.home(), storage, &ids),
            AcceptedRouteEffectiveState::Promoted
        );
    }
    service.close().unwrap();

    let second_attempts = Arc::new(AtomicUsize::new(0));
    let (service, storage, _) = restart_service(
        directory.path(),
        Box::new(CountingUnavailableProvider {
            attempts: Arc::clone(&second_attempts),
        }),
    );
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 0);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(second_attempts.load(Ordering::SeqCst), 0);
    {
        let command_home = service.live_home_command().unwrap();
        let reopened_again = route_entry(command_home.home(), storage, ids.thread, delivered_input);
        assert_eq!(reopened_again.leaf(), delivered.leaf());
        assert_eq!(
            reopened_again.effective_state(),
            AcceptedRouteEffectiveState::Delivered
        );
    }
    service.close().unwrap();
}
