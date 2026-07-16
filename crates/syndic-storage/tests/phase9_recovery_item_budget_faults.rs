#![cfg(feature = "test-faults")]

#[path = "phase9_recovery_projection/support.rs"]
mod support;

use beryl_home_store::{HomeCommand, HomeHealthState, HomeStore};
use syndic_storage::{
    RecoveryBudgetKind, RecoveryItemCount, RecoveryProjectionError, RecoveryProjectionRequest,
    SyndicStorage, TurnItemOrdinal, TurnStateRecord, TurnTerminalOutcome,
    test_faults::{
        FixtureBatch, FixtureDelete, FixtureRecord, recovery_frontier_metrics,
        reset_recovery_frontier_metrics,
    },
};

use support::{Builder, TestHome, open, point_limit};

struct BudgetFixture {
    _home: TestHome,
    store: HomeStore,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    selected: syndic_storage::SelectedPathProof,
}

fn build_budget_fixture(name: &str, root_item_count: u64) -> BudgetFixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 31);
    let root = builder.submit_text("root item");
    builder.complete_without_assistant(root, TurnTerminalOutcome::Complete);
    let child = builder.submit_text("child item");
    builder.complete_without_assistant(child, TurnTerminalOutcome::Complete);
    builder.submit_text("pending tail");
    let thread = builder.thread();
    let selected = builder.selected_path();

    let root_state = storage
        .turn_state(&store, root.turn, point_limit())
        .unwrap()
        .unwrap();
    let root_state = root_state.record();
    let mut fault = FixtureBatch::new();
    fault
        .put(FixtureRecord::TurnState(
            TurnStateRecord::with_finalization_frontier(
                root_state.turn_id(),
                root_state.revision(),
                root_state.lifecycle(),
                root_state.source_event_count(),
                root_item_count,
                root_item_count,
                root_state.end_status(),
                root_state.updated_at(),
            )
            .unwrap(),
        ))
        .unwrap();
    fault
        .delete(FixtureDelete::TurnItem {
            turn: root.turn,
            ordinal: TurnItemOrdinal::FIRST,
        })
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(&store).unwrap(), fault))
        .unwrap();
    store.execute(command).unwrap();

    BudgetFixture {
        _home: home,
        store,
        storage,
        thread,
        selected,
    }
}

#[test]
fn exact_item_budget_reaches_index_scan_but_plus_one_stops_before_allocation() {
    assert_eq!(
        u64::from(
            RecoveryItemCount::new(RecoveryItemCount::MAX)
                .unwrap()
                .get()
        ),
        RecoveryItemCount::MAX
    );

    let exact = build_budget_fixture(
        "phase9-recovery-exact-item-budget",
        RecoveryItemCount::MAX - 1,
    );
    reset_recovery_frontier_metrics();
    let error = exact
        .storage
        .prepare_recovery_projection(
            &exact.store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                exact.thread,
                exact.selected,
                Some(u64::MAX),
            ),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        RecoveryProjectionError::MissingHistory {
            record: "turn-item index"
        }
    ));
    let metrics = recovery_frontier_metrics();
    assert_eq!(metrics.allocation_attempts(), 1);
    assert_eq!(metrics.allocation_completions(), 1);
    assert_eq!(
        metrics.requested_items(),
        usize::try_from(RecoveryItemCount::MAX).unwrap()
    );
    assert!(metrics.observed_capacity() >= metrics.requested_items());
    assert_eq!(metrics.turn_item_read_attempts(), 1);
    assert_eq!(exact.store.health().state(), HomeHealthState::Healthy);
    exact.store.close().unwrap();

    let overflow = build_budget_fixture(
        "phase9-recovery-item-budget-overflow",
        RecoveryItemCount::MAX,
    );
    reset_recovery_frontier_metrics();
    let error = overflow
        .storage
        .prepare_recovery_projection(
            &overflow.store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                overflow.thread,
                overflow.selected,
                Some(u64::MAX),
            ),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        RecoveryProjectionError::BudgetOverflow {
            kind: RecoveryBudgetKind::ItemCount,
            maximum,
            actual,
        } if maximum == RecoveryItemCount::MAX && actual == RecoveryItemCount::MAX + 1
    ));
    let metrics = recovery_frontier_metrics();
    assert_eq!(metrics.allocation_attempts(), 0);
    assert_eq!(metrics.allocation_completions(), 0);
    assert_eq!(metrics.requested_items(), 0);
    assert_eq!(metrics.observed_capacity(), 0);
    assert_eq!(metrics.turn_item_read_attempts(), 0);
    assert_eq!(overflow.store.health().state(), HomeHealthState::Healthy);
    overflow.store.close().unwrap();
}
