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
