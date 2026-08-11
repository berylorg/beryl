#![cfg(feature = "test-faults")]

mod support;

#[path = "phase63_delivery_recovery/finalizing_history_support.rs"]
mod finalizing_history_support;
#[path = "phase63_delivery_recovery/support.rs"]
mod recovery_support;
#[path = "phase65_stop_storage/support.rs"]
mod stop_support;

use std::{thread, time::Duration};

use beryl_home_store::CommandOutcome;
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId};
use syndic_storage::{
    BeginAcceptedInputDelivery, ClaimStopDispatch, CompactionOperationNonce, CreateThread,
    DeliveryRecoveryCase, InputGateState, JoinStopCause, LiveSourceEvent, SourceEventPayload,
    SourceEventSequence, StopAdmissionIneligibility, StopAdmissionRead, StopAttemptNonce,
    StopCause, StopCauseSet, StopOperationNonce, StopOperationState, SyndicPointReadLimit,
    SyndicReadError, TurnEndStatus, TurnIncompleteReason, TurnTerminalOutcome,
};

use stop_support::{
    active_stop_fixture, active_stop_fixture_with_faults, admit_current_draft_as_accepted, execute,
    point_limit,
};
use support::{TestHome, exact_cas, open, timestamp};

fn delete_fixture(
    store: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    deletion: syndic_storage::test_faults::FixtureDelete,
) {
    let mut mutation = syndic_storage::test_faults::FixtureBatch::new();
    mutation.delete(deletion).unwrap();
    support::commit(store, storage, mutation);
}

#[test]
fn empty_active_route_returns_an_exact_safe_admission_source() {
    let fixture = active_stop_fixture("phase68-stop-admission-empty");
    let gate = fixture.gate();
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();

    let StopAdmissionRead::Admissible(candidate) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("active empty route must admit an exact stop")
    };
    assert_eq!(candidate.target(), &fixture.target);
    assert_eq!(candidate.current_gate_revision(), gate.revision());
    assert_eq!(candidate.selected_route(), gate.selected_route().unwrap());
    assert_eq!(candidate.current_turn_state_revision(), state.revision());

    let request = candidate.admission(
        StopOperationNonce::from_bytes([0x68; 16]),
        StopCauseSet::from(StopCause::SelectedOperationControl),
    );
    assert_eq!(request.operation_id().thread_id(), fixture.thread);
    assert_eq!(request.target(), &fixture.target);
    match fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_admit_stop_operation(request.clone()),
        ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean stop admission, got {outcome:?}"),
    }

    let StopAdmissionRead::Stopping(live) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("admission must publish live stop authority")
    };
    assert_eq!(live.operation_id(), request.operation_id());
    assert_eq!(live.target(), request.target());
    assert_eq!(live.state(), StopOperationState::Admitted);
}

#[test]
fn path_neutral_accepted_input_advance_returns_the_current_gate_and_route() {
    let fixture = active_stop_fixture("phase68-stop-admission-path-neutral");
    let before = fixture.gate();
    admit_current_draft_as_accepted(&fixture, "queued before stop", 0x69, 5);
    let current = fixture.gate();
    assert!(current.revision() > before.revision());
    assert!(
        current.selected_route().unwrap().revision() > before.selected_route().unwrap().revision()
    );

    let StopAdmissionRead::Admissible(candidate) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("path-neutral accepted input must preserve stop admission")
    };
    assert_eq!(candidate.target(), &fixture.target);
    assert_eq!(candidate.current_gate_revision(), current.revision());
    assert_eq!(
        candidate.selected_route(),
        current.selected_route().unwrap()
    );

    match fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_admit_stop_operation(candidate.admission(
                    StopOperationNonce::from_bytes([0x6a; 16]),
                    StopCauseSet::from(StopCause::DiagnosticControl),
                )),
        ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean path-neutral stop admission, got {outcome:?}"),
    }
}

#[test]
fn a_delivering_steering_leaf_is_typed_ineligible() {
    let fixture = active_stop_fixture("phase68-stop-admission-delivering");
    let admitted = admit_current_draft_as_accepted(&fixture, "delivering", 0x6b, 5);
    let ready = fixture
        .storage
        .ready_steering_input(&fixture.store, admitted.accepted_input_id(), point_limit())
        .unwrap()
        .unwrap();
    match fixture
        .store
        .execute_current(fixture.storage.current_begin_accepted_input_delivery(
            BeginAcceptedInputDelivery::new(
                fixture.thread,
                admitted.accepted_input_id(),
                ready.accepted_input_revision(),
                ready.target().clone(),
            ),
        )) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean steering-delivery start, got {outcome:?}"),
    }
    let gate = fixture.gate();

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::DeliveringSteering {
            turn_id,
            current_gate_revision,
            selected_route,
        }) if turn_id == fixture.turn
            && current_gate_revision == gate.revision()
            && selected_route == gate.selected_route().unwrap()
    ));
}

#[test]
fn admitted_joined_and_claimed_stops_return_one_shared_live_authority() {
    let fixture = active_stop_fixture("phase68-stop-admission-live-descendants");
    fixture.admit_stop();

    let StopAdmissionRead::Stopping(admitted) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("admitted stop must be live")
    };
    let join = JoinStopCause::new(
        admitted.operation_id(),
        admitted.target().clone(),
        admitted.current_gate_revision(),
        admitted.stop_revision(),
        StopCause::HealthyHomeWindowClose,
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_join_stop_cause(join))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean stop-cause join, got {outcome:?}"),
    }

    let StopAdmissionRead::Stopping(joined) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("cause join must preserve the same live stop")
    };
    assert!(
        joined
            .record()
            .causes()
            .contains(StopCause::HealthyHomeWindowClose)
    );
    let attempt = StopAttemptNonce::from_bytes([0x6c; 16]);
    let claim = ClaimStopDispatch::new(
        joined.operation_id(),
        joined.target().clone(),
        joined.current_gate_revision(),
        joined.stop_revision(),
        attempt,
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_claim_stop_dispatch(claim))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean stop-dispatch claim, got {outcome:?}"),
    }

    let StopAdmissionRead::Stopping(claimed) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("claimed stop must remain live")
    };
    assert_eq!(claimed.attempt(), Some(attempt));
    assert!(matches!(
        claimed.state(),
        StopOperationState::DispatchClaimed
    ));

    admit_current_draft_as_accepted(&fixture, "queued during stop", 0x6d, 6);
    let descendant_gate = fixture.gate();
    let StopAdmissionRead::Stopping(descendant) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("queued stop-period input must preserve live stop authority")
    };
    assert_eq!(descendant.target(), &fixture.target);
    assert_eq!(
        descendant.current_gate_revision(),
        descendant_gate.revision()
    );
    assert_eq!(descendant.attempt(), Some(attempt));
}

#[test]
fn idle_and_pending_gates_are_typed_ineligible() {
    let home = TestHome::new("phase68-stop-admission-idle-pending");
    let mut store = open(home.path());
    let storage = syndic_storage::SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([0x6e; 16]);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([0x6f; 16]),
                support::exact_cas::execution_binding(),
                timestamp(1),
            ),
        ),
    );
    let idle_gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        storage
            .stop_admission_read(&store, thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::Idle {
            current_gate_revision,
        }) if current_gate_revision == idle_gate.revision()
    ));

    let turn = exact_cas::submit_current_draft(
        &store,
        storage,
        thread,
        SyndicDraftId::from_bytes([0x70; 16]),
        SyndicItemId::from_bytes([0x71; 16]),
        "pending",
        timestamp(2),
    );
    let pending_gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(pending_gate.state(), InputGateState::PendingTurn(id) if *id == turn));
    assert!(matches!(
        storage
            .stop_admission_read(&store, thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::PendingTurn {
            turn_id,
            current_gate_revision,
        }) if turn_id == turn && current_gate_revision == pending_gate.revision()
    ));
}

#[test]
fn awaiting_terminal_is_typed_ineligible_without_stop_authority() {
    let fixture = active_stop_fixture("phase68-stop-admission-awaiting-terminal");
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture.gate();
    let event = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(fixture.source.clone()),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::UnknownTerminal,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(5),
    )
    .unwrap();
    match fixture
        .store
        .execute_current(fixture.storage.current_admit_live_source_event(event))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean unknown-terminal admission, got {outcome:?}"),
    }
    let awaiting = fixture.gate();

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::AwaitingTerminal {
            turn_id,
            current_gate_revision,
        }) if turn_id == fixture.turn && current_gate_revision == awaiting.revision()
    ));
}

#[test]
fn compacting_and_finalizing_gates_are_typed_ineligible() {
    let compacting = recovery_support::pending_home("phase68-stop-admission-compacting", 680);
    recovery_support::replace_gate_state(
        &compacting.store,
        compacting.storage,
        compacting.thread,
        InputGateState::Compacting {
            turn_id: compacting.turn,
            operation_nonce: CompactionOperationNonce::from_bytes([68; 16]),
        },
    );
    let compacting_gate = compacting
        .storage
        .input_gate(&compacting.store, compacting.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        compacting.storage.stop_admission_read(
            &compacting.store,
            compacting.thread,
            point_limit(),
        ).unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::Compacting {
            turn_id,
            current_gate_revision,
        }) if turn_id == compacting.turn
            && current_gate_revision == compacting_gate.revision()
    ));

    let finalizing =
        finalizing_history_support::terminal_home("phase68-stop-admission-finalizing", 681, false);
    let finalizing_gate = finalizing
        .storage
        .input_gate(&finalizing.store, finalizing.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        finalizing.storage.stop_admission_read(
            &finalizing.store,
            finalizing.thread,
            point_limit(),
        ).unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::FinalizingHistory {
            turn_id,
            current_gate_revision,
        }) if turn_id == finalizing.turn
            && current_gate_revision == finalizing_gate.revision()
    ));
}

#[test]
fn a_stale_binding_successor_is_typed_ineligible() {
    let fixture = active_stop_fixture("phase68-stop-admission-stale-binding");
    let source = recovery_support::startup_source(&fixture.store, fixture.storage);
    let DeliveryRecoveryCase::Active(active) = fixture
        .storage
        .classify_delivery_recovery(&fixture.store, &source, point_limit())
        .unwrap()
    else {
        panic!("active stop fixture must classify as active delivery authority")
    };
    let abandonment = active
        .generic_abandonment("phase68 deliberate target loss", active.minimum_timestamp())
        .unwrap();
    match fixture
        .store
        .execute_current(fixture.storage.current_abandon_active_binding(abandonment))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean active-binding abandonment, got {outcome:?}"),
    }
    let gate = fixture.gate();
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit())
            .unwrap(),
        StopAdmissionRead::Ineligible(StopAdmissionIneligibility::PendingTurn {
            turn_id,
            current_gate_revision,
        }) if turn_id == fixture.turn && current_gate_revision == gate.revision()
    ));
}

#[test]
fn a_constituent_point_read_budget_exhaustion_is_returned() {
    let fixture = active_stop_fixture("phase68-stop-admission-point-limit");
    let tiny = SyndicPointReadLimit::new(1).unwrap();
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, tiny),
        Err(SyndicReadError::Read(_))
    ));
}

#[test]
fn a_stop_revision_change_between_complete_passes_is_concurrent_change() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};

    let faults = FaultController::new();
    let fixture =
        active_stop_fixture_with_faults("phase68-stop-admission-concurrent-stop", faults.clone());
    fixture.admit_stop();
    let stop = fixture.stop();
    let gate = fixture.gate();
    let join = JoinStopCause::new(
        stop.id(),
        stop.target().clone(),
        gate.revision(),
        stop.revision(),
        StopCause::DiagnosticControl,
    );

    let blocks = (0..33)
        .map(|_| faults.block_next(FaultPoint::BeforeReadConfirmation))
        .collect::<Vec<_>>();
    for block in &blocks[..32] {
        block.release();
    }
    let outcome = thread::scope(|scope| {
        let reader = scope.spawn(|| {
            fixture
                .storage
                .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        });
        assert!(blocks[32].wait_until_reached(Duration::from_secs(10)));
        match fixture
            .store
            .execute_current(fixture.storage.current_join_stop_cause(join))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean concurrent stop-cause join, got {outcome:?}"),
        }
        blocks[32].release();
        reader.join().unwrap()
    });

    assert!(matches!(
        outcome,
        Err(SyndicReadError::ConcurrentChange {
            operation: "stop-admission read"
        })
    ));
}

#[test]
fn matching_terminal_consumption_during_the_second_pass_is_concurrent_change() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};

    let faults = FaultController::new();
    let fixture =
        active_stop_fixture_with_faults("phase68-stop-admission-terminal-race", faults.clone());
    fixture.admit_stop();
    exact_cas::correlate_user_item(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        fixture.turn,
        SyndicItemId::from_bytes([104; 16]),
        &fixture.source,
        timestamp(5),
    );
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.revision(),
        fixture.gate().revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(fixture.source.clone()),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
        ),
        timestamp(6),
    )
    .unwrap();

    let blocks = (0..33)
        .map(|_| faults.block_next(FaultPoint::BeforeReadConfirmation))
        .collect::<Vec<_>>();
    for block in &blocks[..32] {
        block.release();
    }
    let outcome = thread::scope(|scope| {
        let reader = scope.spawn(|| {
            fixture
                .storage
                .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        });
        assert!(blocks[32].wait_until_reached(Duration::from_secs(10)));
        match fixture
            .store
            .execute_current(fixture.storage.current_admit_live_source_event(event))
        {
            CommandOutcome::Committed {
                later_failure: None,
                ..
            } => {}
            outcome => panic!("expected clean concurrent terminal admission, got {outcome:?}"),
        }
        blocks[32].release();
        reader.join().unwrap()
    });

    assert!(matches!(
        outcome,
        Err(SyndicReadError::ConcurrentChange {
            operation: "stop-admission read"
        })
    ));
}

#[test]
fn a_gate_route_change_inside_one_raw_pass_is_concurrent_change() {
    use beryl_home_store::test_faults::{FaultController, FaultPoint};

    let faults = FaultController::new();
    let fixture =
        active_stop_fixture_with_faults("phase68-stop-admission-concurrent-route", faults.clone());
    let blocks = (0..8)
        .map(|_| faults.block_next(FaultPoint::BeforeReadConfirmation))
        .collect::<Vec<_>>();
    for block in &blocks[..7] {
        block.release();
    }
    let outcome = thread::scope(|scope| {
        let reader = scope.spawn(|| {
            fixture
                .storage
                .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        });
        assert!(blocks[7].wait_until_reached(Duration::from_secs(10)));
        admit_current_draft_as_accepted(&fixture, "concurrent route", 0x75, 8);
        blocks[7].release();
        reader.join().unwrap()
    });

    assert!(matches!(
        outcome,
        Err(SyndicReadError::ConcurrentChange {
            operation: "stop-admission read"
        })
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn a_stably_missing_active_cas_turn_is_an_invariant_failure() {
    use syndic_storage::test_faults::{FixtureBatch, FixtureDelete};

    let fixture = active_stop_fixture("phase68-stop-admission-missing-cas-turn");
    let mut mutation = FixtureBatch::new();
    mutation
        .delete(FixtureDelete::ActiveCasTurn(fixture.target.snapshot_id()))
        .unwrap();
    support::commit(&fixture.store, fixture.storage, mutation);

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(
            "steering delivery-recovery authority has no active CAS turn"
        ))
    ));
}

#[test]
fn a_missing_reverse_cas_turn_authority_is_an_invariant_failure() {
    let fixture = active_stop_fixture("phase68-stop-admission-missing-cas-turn-index");
    delete_fixture(
        &fixture.store,
        fixture.storage,
        syndic_storage::test_faults::FixtureDelete::CasTurn {
            thread: fixture.target.cas_thread_id().clone(),
            turn: fixture.target.cas_turn_id().clone(),
        },
    );

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(
            "steerable stop-admission authority disagrees"
        ))
    ));
}

#[test]
fn a_present_but_contradictory_cas_turn_authority_is_an_invariant_failure() {
    use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

    let fixture = active_stop_fixture("phase68-stop-admission-wrong-cas-turn-index");
    let reverse = fixture
        .storage
        .cas_turn_owner(
            &fixture.store,
            fixture.target.cas_thread_id().clone(),
            fixture.target.cas_turn_id().clone(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    let contradictory = syndic_storage::CasTurnIndexRecord::new(
        reverse.cas_thread_id().clone(),
        reverse.cas_turn_id().clone(),
        reverse.thread_id(),
        reverse.turn_id(),
        reverse.binding_revision().checked_next().unwrap(),
        reverse.snapshot_id(),
        reverse.post_turn_native_count(),
    );
    let mut mutation = FixtureBatch::new();
    mutation.put(FixtureRecord::CasTurn(contradictory)).unwrap();
    support::commit(&fixture.store, fixture.storage, mutation);

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(
            "steerable stop-admission authority disagrees"
        ))
    ));
}

#[test]
fn a_missing_ready_route_source_is_an_invariant_failure() {
    let fixture = active_stop_fixture("phase68-stop-admission-missing-ready-source");
    admit_current_draft_as_accepted(&fixture, "ready source", 0x72, 7);
    let selected = fixture.gate().selected_route().unwrap();
    delete_fixture(
        &fixture.store,
        fixture.storage,
        syndic_storage::test_faults::FixtureDelete::AcceptedReadySource {
            thread: fixture.thread,
            generation: selected.generation(),
        },
    );

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(
            "steerable stop-admission authority disagrees"
        ))
    ));
}

#[test]
fn a_missing_stopped_next_route_source_is_an_invariant_failure() {
    let fixture = active_stop_fixture("phase68-stop-admission-missing-next-source");
    admit_current_draft_as_accepted(&fixture, "next source", 0x73, 7);
    let StopAdmissionRead::Admissible(candidate) = fixture
        .storage
        .stop_admission_read(&fixture.store, fixture.thread, point_limit())
        .unwrap()
    else {
        panic!("ready route must remain admissible")
    };
    match fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_admit_stop_operation(candidate.admission(
                    StopOperationNonce::from_bytes([0x74; 16]),
                    StopCauseSet::from(StopCause::SelectedOperationControl),
                )),
        ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean missing-next-source stop admission, got {outcome:?}"),
    }
    let selected = fixture.gate().selected_route().unwrap();
    delete_fixture(
        &fixture.store,
        fixture.storage,
        syndic_storage::test_faults::FixtureDelete::AcceptedNextSource {
            thread: fixture.thread,
            generation: selected.generation(),
        },
    );

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(
            "live stop-admission authority disagrees"
        ))
    ));
}

#[test]
fn a_stopping_gate_without_its_selected_record_is_an_invariant_failure() {
    let fixture = active_stop_fixture("phase68-stop-admission-missing-stop");
    fixture.admit_stop();
    delete_fixture(
        &fixture.store,
        fixture.storage,
        syndic_storage::test_faults::FixtureDelete::StopOperation(fixture.operation_id),
    );

    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(
            "stopping gate has no matching stop-operation record"
        ))
    ));
}
