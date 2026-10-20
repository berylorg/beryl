use crate::support::{self, draft_id, exact_cas, id, timestamp};
use beryl_home_store::HomeStore;
use beryl_model::{CasItemId, SyndicItemId};
use syndic_storage::{
    ProviderAgentMessageV1, ProviderItemV1, ProviderMessagePhaseV1, ProviderTextV1,
    SourceEventPayload, SyndicPointReadLimit, SyndicStorage, TurnEndStatus,
};

pub(super) fn parent_and_child(home: &HomeStore, storage: &SyndicStorage) {
    let thread = id(30);
    support::seed_canonical_empty_thread(home, storage.clone(), thread, draft_id(32));
    let user = SyndicItemId::from_bytes([28; 16]);
    let turn = exact_cas::submit_current_draft(
        home,
        storage.clone(),
        thread,
        draft_id(31),
        user,
        "Discuss this",
        timestamp(2),
    );
    let source = exact_cas::establish_turn(home, storage.clone(), thread, turn, timestamp(2));
    exact_cas::admit_event(
        home,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(2),
    );
    exact_cas::correlate_user_item(
        home,
        storage.clone(),
        thread,
        turn,
        user,
        &source,
        timestamp(3),
    );
    let assistant = ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline("assistant"),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    });
    let assistant_item = SyndicItemId::from_bytes([33; 16]);
    exact_cas::admit_started_then_completed_item(
        home,
        storage.clone(),
        thread,
        turn,
        assistant_item,
        &source,
        CasItemId::new("discussion-source").unwrap(),
        assistant.clone(),
        assistant,
        timestamp(3),
        timestamp(3),
    );
    exact_cas::admit_event(
        home,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(4),
    );
    support::converge_and_release_terminal_history(home, storage.clone(), thread, turn);
    support::discussion_creation::create_child(home, storage, thread, 36, 37);
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let child = storage.thread(home, id(36), limit).unwrap().unwrap();
    assert_eq!(child.parent_thread_id(), Some(thread));
    let context = storage
        .context_envelope(home, child.context_owner_id().unwrap(), limit)
        .unwrap()
        .unwrap();
    let descriptor = context.envelope().descriptor();
    assert_eq!(descriptor.source_thread_id(), thread);
    assert_eq!(descriptor.source_turn_id(), turn);
    assert_eq!(descriptor.source_item_id(), assistant_item);
    assert!(storage.thread(home, id(40), limit).unwrap().is_none());
}
