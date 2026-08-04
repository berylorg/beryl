use beryl_app::cas_projection::{
    AdmittedProjectionSession, OrdinaryInputReplayDiagnosticsSnapshot,
    test_faults::provider_broker_snapshot,
};
use syndic_storage::{BindingState, InputGateState, TurnEndStatus, TurnLifecycle};

use crate::syndic::{Fixture, point_limit};

pub fn assert_three_pass_work(
    snapshot: OrdinaryInputReplayDiagnosticsSnapshot,
    descriptors_per_pass: usize,
    minimum_logical_text_bytes_per_pass: u64,
) {
    assert_eq!(snapshot.passes_started(), 3);
    assert_eq!(
        snapshot.descriptors_emitted(),
        descriptors_per_pass.checked_mul(3).unwrap()
    );
    assert!(
        snapshot.logical_text_bytes()
            >= minimum_logical_text_bytes_per_pass.checked_mul(3).unwrap()
    );
}

pub fn assert_connection_released(session: &AdmittedProjectionSession) {
    let broker = provider_broker_snapshot(session);
    assert_eq!(broker.in_flight().current(), 0);
    assert!(broker.in_flight().high_water() <= 1);
    assert_eq!(broker.submitted(), broker.acked());
    assert_eq!(broker.staged_fragments().current(), 0);
    assert_eq!(broker.checked_user_publications().activity().current(), 0);
    assert!(broker.checked_user_publications().activity().high_water() <= 1);
    let pages = session.provider_page_diagnostics();
    assert_eq!(pages.leased, 0);
    assert_eq!(pages.available, pages.page_count);
}

pub fn assert_durable_success(
    fixture: &Fixture,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
    expected_status: TurnEndStatus,
) {
    let state = fixture
        .storage
        .turn_state(&fixture.store, turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Complete);
    assert_eq!(state.source_event_count(), 4);
    assert_eq!(state.item_count(), 1);
    assert_eq!(state.finalized_item_count(), 1);
    assert_eq!(state.open_item_count(), 0);
    assert_eq!(state.end_status(), Some(expected_status));
    assert_eq!(state.incomplete_reason(), None);

    let items = fixture
        .storage
        .turn_items(
            &fixture.store,
            turn,
            None,
            beryl_home_store::CursorReadLimits::new(2, 64 * 1024).unwrap(),
        )
        .unwrap();
    assert!(!items.has_more());
    assert_eq!(items.records().len(), 1);

    let gate = fixture
        .storage
        .input_gate(&fixture.store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert_eq!(gate.live_count(), 0);

    let binding = fixture
        .storage
        .current_binding(&fixture.store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("successful submitted-input execution did not restore a valid binding")
    };
    let selected = fixture.selected_path(thread);
    assert_eq!(usable.represented_prefix().tail(), Some(turn));
    assert_eq!(
        usable.represented_prefix().source_thread_revision(),
        selected.thread_revision()
    );
    assert_eq!(usable.represented_prefix().digest(), selected.digest());
}
