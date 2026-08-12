use beryl_home_store::CursorReadLimits;
use beryl_model::{SyndicItemId, SyndicTurnId};
use syndic_storage::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, AcceptedRouteEffectiveState, AcceptedRouteRevision,
    CompleteTerminalHistory, DeliveryRecoveryCase, InputGateRecord, InputGateState, NextTurnReason,
    PromoteAcceptedInput, SyndicMutationError,
};

use crate::{
    finalizing_history_support::{
        completion_request, converge_first_item, execute_result, finish_transcript, queue_input,
        terminal_home, typed_error,
    },
    recovery_support::{execute, ordered_id, point_limit, startup_source},
    support::{open, timestamp},
};

#[test]
fn finalizing_history_reopens_classifies_and_routes_new_input_as_terminal_history() {
    let recovery = terminal_home("phase63-finalizing-reopen", 501, false);
    let path = recovery.home.path().to_path_buf();
    let thread = recovery.thread;
    let turn = recovery.turn;
    assert!(matches!(
        recovery.storage.classify_delivery_recovery(
            &recovery.store,
            &startup_source(&recovery.store, recovery.storage),
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::FinalizingHistory {
            thread_id,
            turn_id,
            ..
        }) if thread_id == thread && turn_id == turn
    ));
    recovery
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    recovery.store.close().unwrap();

    let mut store = open(&path);
    let storage = syndic_storage::SyndicStorage::register(&mut store).unwrap();
    assert!(matches!(
        storage.classify_delivery_recovery(
            &store,
            &startup_source(&store, storage),
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::FinalizingHistory {
            thread_id,
            turn_id,
            ..
        }) if thread_id == thread && turn_id == turn
    ));

    let admission = queue_input(&store, storage, thread, 501, timestamp(11), timestamp(12));
    let input = storage
        .accepted_input(&store, admission.accepted_input_id(), point_limit())
        .unwrap()
        .unwrap();
    let route = storage
        .accepted_route_page(
            &store,
            thread,
            input.route_generation(),
            AcceptedRouteRevision::FIRST,
            None,
        )
        .unwrap();
    assert_eq!(route.records().len(), 1);
    assert_eq!(
        route.records()[0].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::TerminalHistory)
    );

    let before = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let stale = completion_request(&store, storage, thread, turn);
    let outcome = execute_result(
        &store,
        storage.complete_terminal_history(storage.revision(&store).unwrap(), stale),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::TerminalHistoryCompletionConflict
    ));
    assert_eq!(
        storage.input_gate(&store, thread, point_limit()).unwrap(),
        Some(before.clone())
    );

    finish_transcript(&store, storage, thread);
    let request = completion_request(&store, storage, thread, turn);
    execute(
        &store,
        storage.complete_terminal_history(storage.revision(&store).unwrap(), request),
    );
    let after = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(after.state(), &InputGateState::Idle);
    assert_eq!(after.revision(), before.revision().checked_next().unwrap());
    assert_eq!(after.accepted_high_water(), before.accepted_high_water());
    assert_eq!(
        after.route_generation_high_water(),
        before.route_generation_high_water()
    );
    assert_eq!(after.selected_route(), before.selected_route());
    assert_eq!(after.live_steering_count(), before.live_steering_count());
    assert_eq!(after.live_next_turn_count(), before.live_next_turn_count());
    assert_eq!(
        after.live_logical_utf8_bytes(),
        before.live_logical_utf8_bytes()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut store = open(&path);
    let storage = syndic_storage::SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = storage
        .accepted_next_source_page(&store, storage.revision(&store).unwrap(), None, limits)
        .unwrap();
    assert_eq!(sources.records().len(), 1);
    let candidate = storage
        .accepted_next_candidate_page(&store, sources.records()[0], None, limits)
        .unwrap()
        .into_candidate()
        .expect("terminal-history input becomes promotable after exact release");
    let successor = SyndicTurnId::from_bytes(*ordered_id(50_501).as_bytes());
    execute(
        &store,
        storage.promote_accepted_input(PromoteAcceptedInput::new(
            candidate,
            successor,
            SyndicItemId::from_bytes(*ordered_id(50_502).as_bytes()),
            timestamp(13),
        )),
    );
    assert_eq!(
        storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::PendingTurn(successor)
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut store = open(&path);
    syndic_storage::SyndicStorage::register(&mut store).unwrap();
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn release_refuses_a_finalizable_item_until_item_and_transcript_converge() {
    let recovery = terminal_home("phase63-finalizing-item-refusal", 502, true);
    let item_id = SyndicItemId::from_bytes(*ordered_id(30_502).as_bytes());
    finish_transcript(&recovery.store, recovery.storage, recovery.thread);
    let before = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let request = completion_request(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        recovery.turn,
    );
    let outcome = execute_result(
        &recovery.store,
        recovery.storage.complete_terminal_history(
            recovery.storage.revision(&recovery.store).unwrap(),
            request,
        ),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::TerminalHistoryCompletionConflict
    ));
    assert_eq!(
        recovery
            .storage
            .input_gate(&recovery.store, recovery.thread, point_limit())
            .unwrap(),
        Some(before)
    );

    converge_first_item(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        recovery.turn,
        item_id,
    );
    finish_transcript(&recovery.store, recovery.storage, recovery.thread);
    let request = completion_request(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        recovery.turn,
    );
    execute(
        &recovery.store,
        recovery.storage.complete_terminal_history(
            recovery.storage.revision(&recovery.store).unwrap(),
            request,
        ),
    );
    assert_eq!(
        recovery
            .storage
            .turn_state(&recovery.store, recovery.turn, point_limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        1
    );
    assert_eq!(
        recovery
            .storage
            .input_gate(&recovery.store, recovery.thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
    recovery
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn stale_completion_proof_consumes_multiple_queued_admission_descendants() {
    let recovery = terminal_home("phase63-finalizing-compatible-descendants", 503, false);
    finish_transcript(&recovery.store, recovery.storage, recovery.thread);
    let observed_gate = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let observed_head = recovery
        .storage
        .transcript_view_head(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let observed_build = recovery
        .storage
        .transcript_build(
            &recovery.store,
            recovery.thread,
            observed_head.generation(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    let request = completion_request(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        recovery.turn,
    );

    queue_input(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        503,
        timestamp(11),
        timestamp(12),
    );
    queue_input(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        504,
        timestamp(13),
        timestamp(14),
    );

    let descendant = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(
        descendant.is_compatible_finalizing_history_descendant_of(&observed_gate, recovery.turn,)
    );
    assert_eq!(
        recovery
            .storage
            .transcript_view_head(&recovery.store, recovery.thread, point_limit())
            .unwrap()
            .unwrap(),
        observed_head
    );
    assert_eq!(
        recovery
            .storage
            .transcript_build(
                &recovery.store,
                recovery.thread,
                observed_head.generation(),
                point_limit(),
            )
            .unwrap()
            .unwrap(),
        observed_build
    );
    let thread = recovery
        .storage
        .thread(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let summary = recovery
        .storage
        .history_summary(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(summary.thread_revision(), thread.revision());
    assert!(!summary.complete());

    execute(
        &recovery.store,
        recovery.storage.complete_terminal_history(
            recovery.storage.revision(&recovery.store).unwrap(),
            request,
        ),
    );
    let released = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(released.is_compatible_terminal_history_release_of(&observed_gate, recovery.turn,));
    assert_eq!(
        released.accepted_high_water(),
        descendant.accepted_high_water()
    );
    assert_eq!(
        released.route_generation_high_water(),
        descendant.route_generation_high_water()
    );
    assert_eq!(
        released.live_next_turn_count(),
        descendant.live_next_turn_count()
    );
    assert_eq!(
        released.live_logical_utf8_bytes(),
        descendant.live_logical_utf8_bytes()
    );
    let path = recovery.home.path().to_path_buf();
    recovery
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    recovery.store.close().unwrap();

    let mut reopened = open(&path);
    syndic_storage::SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn successful_completed_history_stays_complete_across_queued_admission_and_release() {
    let recovery = terminal_home("phase63-successful-history-preserved", 505, true);
    converge_first_item(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        recovery.turn,
        SyndicItemId::from_bytes(*ordered_id(30_505).as_bytes()),
    );
    finish_transcript(&recovery.store, recovery.storage, recovery.thread);
    let observed_gate = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let observed_head = recovery
        .storage
        .transcript_view_head(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let observed_build = recovery
        .storage
        .transcript_build(
            &recovery.store,
            recovery.thread,
            observed_head.generation(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert!(
        recovery
            .storage
            .history_summary(&recovery.store, recovery.thread, point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    let request = completion_request(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        recovery.turn,
    );

    queue_input(
        &recovery.store,
        recovery.storage,
        recovery.thread,
        505,
        timestamp(11),
        timestamp(12),
    );

    let thread = recovery
        .storage
        .thread(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let summary = recovery
        .storage
        .history_summary(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(summary.thread_revision(), thread.revision());
    assert!(summary.complete());
    assert_eq!(
        recovery
            .storage
            .transcript_view_head(&recovery.store, recovery.thread, point_limit())
            .unwrap()
            .unwrap(),
        observed_head
    );
    assert_eq!(
        recovery
            .storage
            .transcript_build(
                &recovery.store,
                recovery.thread,
                observed_head.generation(),
                point_limit(),
            )
            .unwrap()
            .unwrap(),
        observed_build
    );

    execute(
        &recovery.store,
        recovery.storage.complete_terminal_history(
            recovery.storage.revision(&recovery.store).unwrap(),
            request,
        ),
    );
    let released = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(released.is_compatible_terminal_history_release_of(&observed_gate, recovery.turn,));
    let path = recovery.home.path().to_path_buf();
    recovery
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    recovery.store.close().unwrap();

    let mut reopened = open(&path);
    let storage = syndic_storage::SyndicStorage::register(&mut reopened).unwrap();
    assert!(
        storage
            .history_summary(&reopened, recovery.thread, point_limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn completion_rejects_an_impossible_finalizing_gate_descendant() {
    let recovery = terminal_home("phase63-impossible-finalizing-descendant", 506, false);
    finish_transcript(&recovery.store, recovery.storage, recovery.thread);
    let gate = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let impossible = InputGateRecord::new(
        gate.thread_id(),
        gate.revision().checked_next().unwrap(),
        gate.state().clone(),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    assert!(!gate.is_compatible_finalizing_history_descendant_of(&impossible, recovery.turn,));
    let state = recovery
        .storage
        .turn_state(&recovery.store, recovery.turn, point_limit())
        .unwrap()
        .unwrap();
    let head = recovery
        .storage
        .transcript_view_head(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let outcome = execute_result(
        &recovery.store,
        recovery.storage.complete_terminal_history(
            recovery.storage.revision(&recovery.store).unwrap(),
            CompleteTerminalHistory::new(
                recovery.thread,
                recovery.turn,
                impossible,
                state.revision(),
                head.generation(),
                head.revision(),
            ),
        ),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::TerminalHistoryCompletionConflict
    ));
    assert_eq!(
        recovery
            .storage
            .input_gate(&recovery.store, recovery.thread, point_limit())
            .unwrap(),
        Some(gate)
    );
    recovery
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn gate_compatibility_rejects_a_skipped_route_generation() {
    let recovery = terminal_home("phase63-skipped-route-generation", 507, false);
    let observed = recovery
        .storage
        .input_gate(&recovery.store, recovery.thread, point_limit())
        .unwrap()
        .unwrap();
    let next_generation = observed
        .route_generation_high_water()
        .map_or(
            Ok(syndic_storage::AcceptedRouteGeneration::FIRST),
            syndic_storage::AcceptedRouteGeneration::checked_next,
        )
        .unwrap();
    let skipped_generation = next_generation.checked_next().unwrap();
    let impossible = InputGateRecord::new(
        observed.thread_id(),
        observed.revision().checked_next().unwrap(),
        observed.state().clone(),
        observed.accepted_high_water().checked_add(1).unwrap(),
        Some(skipped_generation),
        observed.selected_route(),
        0,
        observed.live_next_turn_count().checked_add(1).unwrap(),
        observed.live_logical_utf8_bytes(),
    )
    .unwrap();
    assert!(!impossible.is_compatible_finalizing_history_descendant_of(&observed, recovery.turn,));
    recovery
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
