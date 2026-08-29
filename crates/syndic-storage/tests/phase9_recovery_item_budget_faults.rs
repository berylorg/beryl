#![cfg(feature = "test-faults")]

#[path = "support/mod.rs"]
mod support;

use beryl_home_store::{HomeHealthState, HomeStore};
use beryl_model::{SyndicThreadId, SyndicTurnId};
use syndic_storage::{
    RecoveryBudgetKind, RecoveryItemCount, RecoveryProjectionError, RecoveryProjectionRequest,
    SelectedPathProof, SyndicPointReadLimit, SyndicStorage, TurnStateRecord,
    test_faults::{FixtureRecord, recovery_residency_metrics, reset_recovery_residency_metrics},
};

use support::{TestHome, batch, commit, id, open, seed_populated};

struct BudgetFixture {
    _home: TestHome,
    store: HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    selected: SelectedPathProof,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn selected_path(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
) -> SelectedPathProof {
    let thread = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    )
}

fn with_item_count(state: &TurnStateRecord, item_count: u64) -> TurnStateRecord {
    TurnStateRecord::with_finalization_frontier(
        state.turn_id(),
        state.revision(),
        state.lifecycle(),
        state.source_event_count(),
        item_count,
        item_count,
        state.end_status(),
        state.updated_at(),
    )
    .unwrap()
}

fn build_budget_fixture(name: &str, root_item_count: u64) -> BudgetFixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage.clone());
    let thread = id(30);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let root_state = storage
        .turn_state(&store, root, point_limit())
        .unwrap()
        .unwrap();
    let fault = batch([FixtureRecord::TurnState(with_item_count(
        &root_state,
        root_item_count,
    ))]);
    commit(&store, storage.clone(), fault);
    let selected = selected_path(&store, &storage, thread);
    BudgetFixture {
        _home: home,
        store,
        storage,
        thread,
        selected,
    }
}

#[test]
fn exact_item_budget_reaches_index_scan_without_a_proportional_frontier() {
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
    reset_recovery_residency_metrics();
    let error = exact
        .storage
        .prepare_recovery_projection(
            &exact.store,
            RecoveryProjectionRequest::for_current_selected_path(
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
    let metrics = recovery_residency_metrics();
    assert_eq!(metrics.max_resident_turns(), 1);
    assert_eq!(metrics.max_resident_items(), 0);
    assert_eq!(metrics.turn_item_read_attempts(), 1);
    assert_eq!(metrics.cursor_page_count(), 0);
    assert_eq!(metrics.max_cursor_page_bytes(), 0);
    assert_eq!(exact.store.health().state(), HomeHealthState::Healthy);
    exact.store.close().unwrap();

    let overflow = build_budget_fixture(
        "phase9-recovery-item-budget-overflow",
        RecoveryItemCount::MAX,
    );
    reset_recovery_residency_metrics();
    let error = overflow
        .storage
        .prepare_recovery_projection(
            &overflow.store,
            RecoveryProjectionRequest::for_current_selected_path(
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
    let metrics = recovery_residency_metrics();
    assert_eq!(metrics.max_resident_turns(), 1);
    assert_eq!(metrics.max_resident_items(), 0);
    assert_eq!(metrics.turn_item_read_attempts(), 0);
    assert_eq!(metrics.cursor_page_count(), 0);
    assert_eq!(metrics.max_cursor_page_bytes(), 0);
    assert_eq!(overflow.store.health().state(), HomeHealthState::Healthy);
    overflow.store.close().unwrap();
}
