use crate::support::exact_cas;
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use syndic_storage::{SourceEventPayload, SyndicTimestamp, TurnEndStatus};

pub(crate) fn seed(
    home: &beryl_home_store::HomeStore,
    storage: &syndic_storage::SyndicStorage,
    target: SyndicThreadId,
) {
    let item = SyndicItemId::from_bytes([237; 16]);
    let timestamp = SyndicTimestamp::from_unix_millis;
    let text = format!("```rust\n{}```", "let payload = 1;\n".repeat(2000));
    let turn = exact_cas::submit_current_draft(
        home,
        storage.clone(),
        target,
        SyndicDraftId::from_bytes([238; 16]),
        item,
        &text,
        timestamp(201),
    );
    finish(home, storage, target, turn, item);
}

pub(crate) fn finish(
    home: &beryl_home_store::HomeStore,
    storage: &syndic_storage::SyndicStorage,
    target: SyndicThreadId,
    turn: SyndicTurnId,
    item: SyndicItemId,
) {
    let timestamp = SyndicTimestamp::from_unix_millis;
    let base = storage
        .turn(
            home,
            turn,
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .submitted_at()
        .unix_millis();
    let source =
        exact_cas::establish_turn(home, storage.clone(), target, turn, timestamp(base + 1));
    exact_cas::admit_event(
        home,
        storage.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(base + 2),
    );
    exact_cas::correlate_user_item(
        home,
        storage.clone(),
        target,
        turn,
        item,
        &source,
        timestamp(base + 3),
    );
    exact_cas::admit_event(
        home,
        storage.clone(),
        target,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(base + 4),
    );
    exact_cas::converge_and_release_terminal_history(home, storage.clone(), target, turn);
}
