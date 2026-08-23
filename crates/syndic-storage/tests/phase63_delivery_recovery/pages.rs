use beryl_home_store::CursorReadLimits;
use beryl_model::SyndicItemId;
use syndic_storage::{
    DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES, DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
    DeliveryRecoveryCase, InputGateState, SyndicReadError, SyndicStorage,
};

use crate::{
    recovery_support::{ordered_draft, ordered_id, pending_home, point_limit, replace_gate_state},
    support::{TestHome, exact_cas, open, seed_canonical_empty_thread},
};

fn limits(items: usize) -> CursorReadLimits {
    CursorReadLimits::new(items, DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES).unwrap()
}

#[test]
fn startup_cursor_progress_survives_earlier_gate_mutation_and_filtered_pages() {
    let home = TestHome::new("phase63-startup-key-cursor");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let threads = [ordered_id(1), ordered_id(2), ordered_id(3)];
    for (index, thread) in threads.iter().enumerate() {
        seed_canonical_empty_thread(
            &store,
            storage,
            *thread,
            ordered_draft(10_000 + index as u64),
        );
    }
    for (index, thread) in [(0_u64, threads[0]), (2, threads[2])] {
        let text = format!("restart-{index}");
        exact_cas::submit_current_draft(
            &store,
            storage,
            thread,
            ordered_draft(20_000 + index),
            SyndicItemId::from_bytes(*ordered_id(30_000 + index).as_bytes()),
            &text,
            crate::support::timestamp(3 + index),
        );
    }

    let first = storage
        .delivery_recovery_startup_page(&store, None, limits(1))
        .unwrap();
    assert_eq!(first.records().len(), 1);
    assert_eq!(first.records()[0].thread_id(), threads[0]);
    let first_cursor = first.next_cursor().expect("two physical gates remain");

    replace_gate_state(&store, storage, threads[0], InputGateState::Idle);
    let second = storage
        .delivery_recovery_startup_page(&store, Some(first_cursor), limits(1))
        .unwrap();
    assert!(second.records().is_empty());
    let second_cursor = second
        .next_cursor()
        .expect("filtered idle row still advances");
    let third = storage
        .delivery_recovery_startup_page(&store, Some(second_cursor), limits(1))
        .unwrap();
    assert_eq!(third.records().len(), 1);
    assert_eq!(third.records()[0].thread_id(), threads[2]);
    assert!(third.next_cursor().is_none());
}

#[test]
fn terminal_heavy_gate_pages_clamp_and_advance_while_filtered_empty() {
    let home = TestHome::new("phase63-terminal-heavy-pages");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    for value in 1..=300 {
        seed_canonical_empty_thread(
            &store,
            storage,
            ordered_id(value),
            ordered_draft(10_000 + value),
        );
    }
    let oversized = CursorReadLimits::new(usize::MAX, usize::MAX).unwrap();

    let first = storage
        .delivery_recovery_startup_page(&store, None, oversized)
        .unwrap();
    assert!(first.records().is_empty());
    assert_eq!(
        first.next_cursor().is_some(),
        300 > DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS
    );
    assert!(first.stored_bytes() <= DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES);
    assert!(first.decoded_bytes() <= DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES);
    let second = storage
        .delivery_recovery_startup_page(&store, first.next_cursor(), oversized)
        .unwrap();
    assert!(second.records().is_empty());
    assert!(second.next_cursor().is_none());

    let revision = storage.revision(&store).unwrap();
    let first = storage
        .recovered_pending_page(&store, revision, None, oversized, point_limit())
        .unwrap();
    assert!(first.records().is_empty());
    let cursor = first
        .next_cursor()
        .expect("physical recovered-pending scan is clamped");
    let second = storage
        .recovered_pending_page(&store, revision, Some(cursor), oversized, point_limit())
        .unwrap();
    assert!(second.records().is_empty());
    assert!(second.next_cursor().is_none());
}

#[test]
fn recovered_pending_page_proves_safe_work_and_fences_cursor_revision() {
    let home = TestHome::new("phase63-pending-revision-fence");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let threads = [ordered_id(11), ordered_id(12)];
    for (index, thread) in threads.iter().enumerate() {
        seed_canonical_empty_thread(
            &store,
            storage,
            *thread,
            ordered_draft(40_000 + index as u64),
        );
    }
    let mut turns = Vec::new();
    for (index, thread) in threads.into_iter().enumerate() {
        let text = format!("safe pending {index}");
        turns.push(exact_cas::submit_current_draft(
            &store,
            storage,
            thread,
            ordered_draft(50_000 + index as u64),
            SyndicItemId::from_bytes(*ordered_id(60_000 + index as u64).as_bytes()),
            &text,
            crate::support::timestamp(10 + index as u64),
        ));
    }
    let revision = storage.revision(&store).unwrap();
    let first = storage
        .recovered_pending_page(&store, revision, None, limits(1), point_limit())
        .unwrap();
    assert_eq!(first.records().len(), 1);
    let source = first.records()[0];
    assert_eq!(source.thread_id(), ordered_id(11));
    assert_eq!(source.turn_id(), turns[0]);
    assert_eq!(source.source_revision(), revision);
    assert_eq!(source.minimum_timestamp(), crate::support::timestamp(10));
    let cursor = first.next_cursor().expect("second pending row remains");

    replace_gate_state(
        &store,
        storage,
        ordered_id(11),
        InputGateState::PendingTurn(turns[0]),
    );
    assert!(matches!(
        storage.recovered_pending_page(&store, revision, Some(cursor), limits(1), point_limit(),),
        Err(SyndicReadError::StaleRecoveredPendingScan)
    ));
    assert!(matches!(
        storage.recovered_pending_page(
            &store,
            storage.revision(&store).unwrap(),
            Some(cursor),
            limits(1),
            point_limit(),
        ),
        Err(SyndicReadError::InvalidRecoveredPendingCursor)
    ));
    let rebased = storage
        .rebase_recovered_pending_cursor(&store, cursor)
        .unwrap();
    let second = storage
        .recovered_pending_page(
            &store,
            rebased.source_revision(),
            Some(rebased),
            limits(1),
            point_limit(),
        )
        .unwrap();
    assert_eq!(second.records().len(), 1);
    assert_eq!(second.records()[0].thread_id(), ordered_id(12));
    assert_eq!(second.records()[0].turn_id(), turns[1]);
    assert!(second.next_cursor().is_none());
}

#[test]
fn startup_cursor_from_another_home_is_rejected() {
    fn two_gate_home(
        name: &str,
        base: u64,
    ) -> (TestHome, beryl_home_store::HomeStore, SyndicStorage) {
        let home = TestHome::new(name);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        for value in [base, base + 1] {
            seed_canonical_empty_thread(
                &store,
                storage,
                ordered_id(value),
                ordered_draft(10_000 + value),
            );
        }
        (home, store, storage)
    }
    let (_first_home, first_store, first_storage) =
        two_gate_home("phase63-startup-cursor-home-a", 100);
    let first = first_storage
        .delivery_recovery_startup_page(&first_store, None, limits(1))
        .unwrap();
    let cursor = first.next_cursor().expect("second physical gate remains");

    let (_second_home, second_store, second_storage) =
        two_gate_home("phase63-startup-cursor-home-b", 200);
    assert!(matches!(
        second_storage.delivery_recovery_startup_page(&second_store, Some(cursor), limits(1),),
        Err(SyndicReadError::InvalidDeliveryRecoveryStartupCursor)
    ));
}

#[test]
fn recovered_pending_and_classification_survive_reopen() {
    let recovery = pending_home("phase63-reopen-pending", 300);
    let path = recovery.home.path().to_path_buf();
    let expected_thread = recovery.thread;
    let expected_turn = recovery.turn;
    recovery.store.close().unwrap();

    let mut reopened = open(&path);
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let source_page = storage
        .delivery_recovery_startup_page(&reopened, None, limits(16))
        .unwrap();
    assert_eq!(source_page.records().len(), 1);
    let case = storage
        .classify_delivery_recovery(&reopened, &source_page.records()[0], point_limit())
        .unwrap();
    assert!(matches!(
        case,
        DeliveryRecoveryCase::Pending {
            thread_id,
            turn_id,
            ..
        } if thread_id == expected_thread && turn_id == expected_turn
    ));
    let revision = storage.revision(&reopened).unwrap();
    let pending = storage
        .recovered_pending_page(&reopened, revision, None, limits(16), point_limit())
        .unwrap();
    assert_eq!(pending.records().len(), 1);
    assert_eq!(pending.records()[0].turn_id(), expected_turn);
}
