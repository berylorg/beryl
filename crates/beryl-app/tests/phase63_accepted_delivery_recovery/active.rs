use std::{path::Path, thread, time::Duration};

use beryl_backend::ManagedBackendClientConnector;
use beryl_model::{CasProcessGeneration, RuntimeId};
use syndic_storage::{
    AcceptedInputLifecycle, AcceptedRouteEffectiveState, BindingState, DeliveryRecoveryCase,
    InputGateState, NextTurnReason, SyndicStorage, TurnIncompleteReason, TurnLifecycle,
    TurnTerminalOutcome,
};

use crate::{
    app_support::{
        PromotedTurn, SeededHome, close_seeded, execute, point_limit, promote_installed_next,
        restart_service, seeded_home, startup_source, time,
    },
    phase62_support::{
        AUTHORIZATION, NormalTerminalServer, TIMEOUT, UnavailableProvider, accepted_route_state,
        execution_binding, install_next_records,
    },
    records::{ActiveSeed, activate_promoted_turn_at},
    support as storage_support,
};

fn promoted_active(seed: u8, publish_cas_turn: bool) -> (SeededHome, PromotedTurn, ActiveSeed) {
    promoted_active_at(seed, publish_cas_turn, time(63_030))
}

fn promoted_active_at(
    seed: u8,
    publish_cas_turn: bool,
    started_at: syndic_storage::SyndicTimestamp,
) -> (SeededHome, PromotedTurn, ActiveSeed) {
    let home = seeded_home();
    let ids = install_next_records(
        &home.store,
        home.storage,
        seed,
        execution_binding(RuntimeId::from_bytes([seed.wrapping_add(60); 16])),
    );
    let promoted = promote_installed_next(
        &home.store,
        home.storage,
        &home.state,
        ids,
        seed.wrapping_add(30),
    );
    let active = activate_promoted_turn_at(
        &home.store,
        home.storage,
        ids.thread,
        promoted.turn,
        seed.wrapping_add(60),
        publish_cas_turn,
        started_at,
    );
    (home, promoted, active)
}

fn assert_authority_lost(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: beryl_model::SyndicThreadId,
    turn_id: beryl_model::SyndicTurnId,
) {
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert!(gate.selected_route().is_some());
    let binding = storage
        .current_binding(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
    let state = storage
        .turn_state(store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    let end = state
        .end_status()
        .expect("recovery terminal has exact status");
    assert_eq!(end.outcome(), TurnTerminalOutcome::Incomplete);
    assert_eq!(
        end.incomplete_reason(),
        Some(TurnIncompleteReason::AuthorityLost)
    );
}

#[test]
fn activated_without_cas_turn_converges_before_handoff_and_is_idempotent() {
    let future_started_at = time(4_102_444_800_000);
    let (home, promoted, active) = promoted_active_at(181, false, future_started_at);
    assert!(active.cas_turn.is_none());
    assert!(
        home.storage
            .active_cas_turn(&home.store, active.snapshot, point_limit())
            .unwrap()
            .is_none()
    );
    let directory = close_seeded(home);

    let (service, storage, _) = restart_service(directory.path(), Box::new(UnavailableProvider));
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_page_reads(), 1);
    assert_eq!(diagnostics.startup_recovery_cases(), 1);
    assert_eq!(diagnostics.startup_active_convergences(), 1);
    assert_eq!(diagnostics.startup_terminal_convergences(), 1);
    assert_eq!(diagnostics.startup_pending_turns(), 0);
    let (first_gate, first_binding, first_state, first_revision) = {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_authority_lost(home, storage, promoted.ids.thread, promoted.turn);
        assert_eq!(
            accepted_route_state(home, storage, &promoted.ids),
            AcceptedRouteEffectiveState::Promoted
        );
        let gate = storage
            .input_gate(home, promoted.ids.thread, point_limit())
            .unwrap()
            .unwrap();
        let binding = storage
            .current_binding(home, promoted.ids.thread, point_limit())
            .unwrap()
            .unwrap();
        let state = storage
            .turn_state(home, promoted.turn, point_limit())
            .unwrap()
            .unwrap();
        assert!(state.updated_at() >= future_started_at);
        (gate, binding, state, storage.revision(home).unwrap())
    };
    service.close().unwrap();

    let (service, storage, _) = restart_service(directory.path(), Box::new(UnavailableProvider));
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_page_reads(), 1);
    assert_eq!(diagnostics.startup_recovery_cases(), 0);
    assert_eq!(diagnostics.startup_active_convergences(), 0);
    assert_eq!(diagnostics.startup_terminal_convergences(), 0);
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(storage.revision(home).unwrap(), first_revision);
        assert_eq!(
            storage
                .input_gate(home, promoted.ids.thread, point_limit())
                .unwrap()
                .unwrap(),
            first_gate
        );
        assert_eq!(
            storage
                .current_binding(home, promoted.ids.thread, point_limit())
                .unwrap()
                .unwrap(),
            first_binding
        );
        assert_eq!(
            storage
                .turn_state(home, promoted.turn, point_limit())
                .unwrap()
                .unwrap(),
            first_state
        );
    }
    service.close().unwrap();
}

#[test]
fn delivering_steering_becomes_unknown_without_duplicate_delivery() {
    let home = seeded_home();
    storage_support::commit(
        &home.store,
        home.storage,
        storage_support::batch(storage_support::phase11::mixed_abandonment_records()),
    );
    home.store.validate_registered_domains().unwrap();
    let thread_id = storage_support::id(40);
    let turn_id = storage_support::populated::active_turn();
    let snapshot_id = storage_support::populated::active_snapshot();
    let source = startup_source(&home.store, home.storage);
    let DeliveryRecoveryCase::Active(active) = home
        .storage
        .classify_delivery_recovery(&home.store, &source, point_limit())
        .unwrap()
    else {
        panic!("mixed delivering fixture must classify as active");
    };
    assert_eq!(active.thread_id(), thread_id);
    assert_eq!(active.turn_id(), turn_id);
    assert_eq!(active.snapshot_id(), snapshot_id);
    assert!(
        home.storage
            .active_cas_turn(&home.store, snapshot_id, point_limit())
            .unwrap()
            .is_some()
    );
    let directory = close_seeded(home);

    let (service, storage, _) = restart_service(directory.path(), Box::new(UnavailableProvider));
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 1);
    assert_eq!(diagnostics.startup_active_convergences(), 1);
    assert_eq!(diagnostics.startup_terminal_convergences(), 1);
    let (route, page) = {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_authority_lost(home, storage, thread_id, turn_id);
        assert!(
            storage
                .active_cas_turn(home, snapshot_id, point_limit())
                .unwrap()
                .is_some(),
            "recovery preserves durable possible-dispatch evidence"
        );

        let gate = storage
            .input_gate(home, thread_id, point_limit())
            .unwrap()
            .unwrap();
        let route = gate
            .selected_route()
            .expect("generic abandonment retains projection-loss route authority");
        let page = storage
            .accepted_route_page(home, thread_id, route.generation(), route.revision(), None)
            .unwrap();
        (route, page)
    };
    let delivering = page
        .records()
        .iter()
        .find(|entry| entry.input().id() == storage_support::phase11::delivering_input())
        .unwrap();
    assert_eq!(
        delivering.effective_state(),
        AcceptedRouteEffectiveState::DeliveryUnknown
    );
    assert_eq!(
        delivering.leaf().lifecycle(),
        AcceptedInputLifecycle::Delivering
    );
    let retryable = page
        .records()
        .iter()
        .find(|entry| entry.input().id() == storage_support::phase11::retryable_input())
        .unwrap();
    assert_eq!(
        retryable.effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );

    let server = NormalTerminalServer::spawn_admission_only();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            RuntimeId::from_bytes([182; 16]),
            CasProcessGeneration::new(63_182).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    server.wait_for_admission();
    thread::sleep(Duration::from_millis(100));
    {
        let command_home = service.live_home_command().unwrap();
        assert_eq!(
            storage
                .accepted_route_page(
                    command_home.home(),
                    thread_id,
                    route.generation(),
                    route.revision(),
                    None,
                )
                .unwrap()
                .records()
                .iter()
                .find(|entry| {
                    entry.input().id() == storage_support::phase11::delivering_input()
                })
                .unwrap()
                .effective_state(),
            AcceptedRouteEffectiveState::DeliveryUnknown
        );
    }
    session.invalidate_connection();
    drop(session);
    service.close().unwrap();
    server.join();
}

#[test]
fn post_abandonment_restart_publishes_terminal_and_converges_captured_history() {
    let (home, promoted, active_seed) = promoted_active(183, true);
    assert!(active_seed.cas_turn.is_some());
    let source = startup_source(&home.store, home.storage);
    let DeliveryRecoveryCase::Active(active) = home
        .storage
        .classify_delivery_recovery(&home.store, &source, point_limit())
        .unwrap()
    else {
        panic!("pre-cut fixture must classify as active");
    };
    let future_abandoned_at = time(4_102_444_900_000);
    let request = active
        .generic_abandonment(
            "phase63 app cut after generic abandonment",
            future_abandoned_at,
        )
        .unwrap();
    execute(
        &home.store,
        home.storage
            .abandon_active_binding(home.storage.revision(&home.store).unwrap(), request),
    );
    let post_source = startup_source(&home.store, home.storage);
    assert!(matches!(
        home.storage.classify_delivery_recovery(
            &home.store,
            &post_source,
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::PostAbandonment {
            thread_id,
            turn_id,
            ..
        }) if thread_id == promoted.ids.thread && turn_id == promoted.turn
    ));
    let revision_before = home.storage.revision(&home.store).unwrap();
    let gate_before = home
        .storage
        .input_gate(&home.store, promoted.ids.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate_before.state(),
        &InputGateState::PendingTurn(promoted.turn)
    );
    let route_before = gate_before
        .selected_route()
        .expect("post-abandonment cut retains loss route");
    let binding_before = home
        .storage
        .current_binding(&home.store, promoted.ids.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        binding_before.binding().state(),
        BindingState::Stale(_)
    ));
    let state_before = home
        .storage
        .turn_state(&home.store, promoted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state_before.lifecycle(), TurnLifecycle::Pending);
    let directory = close_seeded(home);

    let (service, storage, _) = restart_service(directory.path(), Box::new(UnavailableProvider));
    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert_eq!(diagnostics.startup_recovery_cases(), 1);
    assert_eq!(diagnostics.startup_active_convergences(), 0);
    assert_eq!(diagnostics.startup_terminal_convergences(), 1);
    assert!(service.initial_storage_revision() > revision_before);
    let command_home = service.live_home_command().unwrap();
    let home = command_home.home();
    assert_eq!(
        service.initial_storage_revision(),
        storage.revision(home).unwrap()
    );
    assert_authority_lost(home, storage, promoted.ids.thread, promoted.turn);
    let gate_after = storage
        .input_gate(home, promoted.ids.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate_after.selected_route(), Some(route_before));
    assert_eq!(
        gate_after.revision(),
        gate_before
            .revision()
            .checked_next()
            .unwrap()
            .checked_next()
            .unwrap()
    );
    assert_eq!(
        storage
            .current_binding(home, promoted.ids.thread, point_limit())
            .unwrap()
            .unwrap(),
        binding_before
    );
    let state_after = storage
        .turn_state(home, promoted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert!(state_after.revision() > state_before.revision());
    assert_eq!(
        state_after.source_event_count(),
        state_before.source_event_count() + 1
    );
    assert_eq!(state_after.item_count(), state_before.item_count());
    assert_eq!(
        state_after.finalized_item_count(),
        state_before.finalized_item_count()
    );
    assert_eq!(
        state_after.open_item_count(),
        state_before.open_item_count()
    );
    assert_eq!(
        state_after.history_blocking_item_count(),
        state_before.history_blocking_item_count()
    );
    assert_eq!(
        state_after.provider_observation_issue(),
        state_before.provider_observation_issue()
    );
    assert!(state_after.updated_at() >= future_abandoned_at);
    assert!(
        storage
            .active_cas_turn(home, active_seed.snapshot, point_limit(),)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        accepted_route_state(home, storage, &promoted.ids),
        AcceptedRouteEffectiveState::Promoted
    );
    drop(command_home);
    service.close().unwrap();
}
