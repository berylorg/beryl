use super::*;

fn thread(value: u8) -> SyndicThreadId {
    SyndicThreadId::from_bytes([value; 16])
}

#[test]
fn successful_exact_selection_records_once_and_expires_oldest() {
    let mut history = ThreadNavigationHistory::default();
    for index in 1..=80 {
        history.begin(Some(thread(index - 1)), thread(index));
        history.settle(Some(thread(index)));
        history.settle(Some(thread(index)));
    }
    assert_eq!(history.entries.len(), HISTORY_CAPACITY);
    assert_eq!(history.entries.front(), Some(&thread(17)));
    assert_eq!(history.entries.back(), Some(&thread(80)));
}

#[test]
fn noop_cancel_and_proven_prior_preserve_history() {
    let mut history = ThreadNavigationHistory::default();
    history.begin(Some(thread(1)), thread(2));
    history.settle(Some(thread(2)));
    let before = history.entries.clone();
    history.begin(Some(thread(2)), thread(2));
    history.settle(Some(thread(2)));
    history.begin(Some(thread(2)), thread(3));
    history.cancel();
    history.settle(Some(thread(3)));
    history.begin(Some(thread(2)), thread(4));
    history.settle(Some(thread(2)));
    assert_eq!(history.entries, before);
}

fn select(history: &mut ThreadNavigationHistory, prior: u8, target: u8) {
    history.begin(Some(thread(prior)), thread(target));
    history.settle(Some(thread(target)));
    history.synchronize(Some(thread(target)));
}

fn navigate(history: &mut ThreadNavigationHistory, forward: bool, target: u8) {
    assert_eq!(history.begin_movement(forward), Some(thread(target)));
    assert_eq!(history.target(false), None);
    assert_eq!(history.target(true), None);
    assert_eq!(history.begin_movement(!forward), None);
    history.settle(Some(thread(target)));
    history.synchronize(Some(thread(target)));
}

#[test]
fn backward_forward_round_trip_and_repeated_visits_keep_exact_positions() {
    let mut history = ThreadNavigationHistory::default();
    select(&mut history, 1, 2);
    select(&mut history, 2, 3);
    navigate(&mut history, false, 2);
    navigate(&mut history, false, 1);
    navigate(&mut history, true, 2);
    navigate(&mut history, true, 3);
    select(&mut history, 3, 1);
    assert_eq!(
        history.test_entries(),
        vec![thread(1), thread(2), thread(3), thread(1)]
    );
    navigate(&mut history, false, 3);
    navigate(&mut history, true, 1);
}

#[test]
fn successful_selection_from_backward_position_discards_forward_branch() {
    let mut history = ThreadNavigationHistory::default();
    select(&mut history, 1, 2);
    select(&mut history, 2, 3);
    navigate(&mut history, false, 2);
    select(&mut history, 2, 4);
    assert_eq!(
        history.test_entries(),
        vec![thread(1), thread(2), thread(4)]
    );
    assert_eq!(history.target(true), None);
}

#[test]
fn acquisition_records_no_entry_but_user_departure_can_return_forward() {
    let mut history = ThreadNavigationHistory::default();
    select(&mut history, 1, 2);
    history.synchronize(Some(thread(3)));
    assert_eq!(history.test_entries(), vec![thread(1), thread(2)]);
    assert_eq!(history.target(false), Some(thread(2)));
    navigate(&mut history, false, 2);
    assert_eq!(
        history.test_entries(),
        vec![thread(1), thread(2), thread(3)]
    );
    navigate(&mut history, true, 3);
}

#[test]
fn acquisition_after_back_clears_forward_without_inventing_acquisition_entry() {
    let mut history = ThreadNavigationHistory::default();
    select(&mut history, 1, 2);
    navigate(&mut history, false, 1);
    history.settle_acquisition(Some(thread(3)));
    assert_eq!(history.test_entries(), vec![thread(1)]);
    assert_eq!(history.target(false), Some(thread(1)));
    assert_eq!(history.target(true), None);
    navigate(&mut history, false, 1);
    navigate(&mut history, true, 3);
}

#[test]
fn background_identity_change_preserves_tail_until_successful_user_departure() {
    let mut history = ThreadNavigationHistory::default();
    select(&mut history, 1, 2);
    navigate(&mut history, false, 1);
    let entries = history.test_entries();
    history.synchronize(Some(thread(3)));
    assert_eq!(history.test_entries(), entries);
    assert_eq!(history.cursor, Some(0));
    assert_eq!(history.target(false), Some(thread(1)));
    history.begin_movement(false);
    history.cancel();
    assert_eq!(history.test_entries(), entries);
    navigate(&mut history, false, 1);
    assert_eq!(history.test_entries(), vec![thread(1), thread(3)]);
    navigate(&mut history, true, 3);
}

#[test]
fn successful_same_thread_acquisition_preserves_forward_tail() {
    let mut history = ThreadNavigationHistory::default();
    select(&mut history, 1, 2);
    navigate(&mut history, false, 1);
    history.settle_acquisition(Some(thread(1)));
    assert_eq!(history.test_entries(), vec![thread(1), thread(2)]);
    assert_eq!(history.target(true), Some(thread(2)));
}

#[test]
fn same_identity_rebind_preserves_forward_and_cancel_or_failed_move_preserves_cursor() {
    let mut history = ThreadNavigationHistory::default();
    select(&mut history, 1, 2);
    select(&mut history, 2, 3);
    navigate(&mut history, false, 2);
    history.synchronize(Some(thread(2)));
    assert_eq!(history.target(true), Some(thread(3)));
    let entries = history.test_entries();
    history.begin_movement(false);
    history.cancel();
    assert_eq!(history.target(false), Some(thread(1)));
    history.begin_movement(true);
    history.settle(Some(thread(2)));
    assert_eq!(history.target(true), Some(thread(3)));
    assert_eq!(history.test_entries(), entries);
}

#[test]
fn capacity_expiration_on_unrecorded_departure_preserves_exact_target_and_return() {
    let mut history = ThreadNavigationHistory::default();
    for index in 1..=64 {
        select(&mut history, index - 1, index);
    }
    history.synchronize(Some(thread(65)));
    navigate(&mut history, false, 64);
    assert_eq!(history.entries.len(), HISTORY_CAPACITY);
    assert_eq!(history.entries.front(), Some(&thread(2)));
    navigate(&mut history, true, 65);
}

#[test]
fn histories_are_window_local_and_unsuccessful_pending_selection_does_not_branch() {
    let mut first = ThreadNavigationHistory::default();
    let mut second = ThreadNavigationHistory::default();
    select(&mut first, 1, 2);
    select(&mut second, 3, 4);
    navigate(&mut first, false, 1);
    first.begin(Some(thread(1)), thread(5));
    first.settle(Some(thread(1)));
    assert_eq!(first.target(true), Some(thread(2)));
    assert_eq!(second.test_entries(), vec![thread(3), thread(4)]);
    assert_eq!(second.target(false), Some(thread(3)));
}
