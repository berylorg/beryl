use super as support;
use beryl_home_store::HomeStore;
use beryl_model::*;
use syndic_storage::*;

pub fn seed_pending(store: &HomeStore, storage: &SyndicStorage) -> DiscussionParentRequest {
    seed_parent_history(store, storage);
    support::discussion_creation::create_child(store, storage, support::id(30), 36, 37);
    let request = support::discussion_handoff::active_request(
        store,
        storage,
        ResolutionIntentId::from_bytes([231; 16]),
        JobId::from_bytes([231; 16]),
    );
    let admission = storage
        .prepare_discussion_handoff(store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let child_gate = admission.intent().new_gate();
    support::discussion_input::committed(store, admission.contribution());
    DiscussionParentRequest {
        child_gate,
        parent_thread_id: request.parent.thread_id,
        context_owner: request.context_owner,
        context_digest: request.context_digest,
    }
}

pub fn seed(
    store: &HomeStore,
    storage: SyndicStorage,
    text: &str,
) -> (
    AcceptedInputRecord,
    DiscussionHandoffReceipt,
    DiscussionHandoffGateRecord,
) {
    let source = seed_pending(store, &storage);
    let DiscussionParentEligibility::Proven(parent) =
        storage.prepare_discussion_parent(store, source).unwrap()
    else {
        panic!("ready parent")
    };
    let prepared = storage
        .prepare_generated_discussion_input(
            store,
            parent,
            GeneratedDiscussionInput {
                parent_turn_id: SyndicTurnId::from_bytes([234; 16]),
                canonical_item_id: SyndicItemId::from_bytes([232; 16]),
                resolution: text.to_owned(),
                admitted_at: support::timestamp(100),
            },
        )
        .unwrap();
    let intent = prepared.intent();
    assert_eq!(
        storage
            .generated_discussion_input_status(store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::ExactOld
    );
    support::discussion_input::committed(store, prepared.into_contribution());
    assert_eq!(
        storage
            .generated_discussion_input_status(store, &intent)
            .unwrap(),
        GeneratedDiscussionInputStatus::ExactNew
    );
    let input = intent.input().clone();
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        panic!("generated receipt")
    };
    (input, receipt, source.child_gate)
}
fn seed_parent_history(store: &HomeStore, storage: &SyndicStorage) {
    use support::{draft_id, exact_cas, id, timestamp};
    let thread = id(30);
    support::seed_canonical_empty_thread(store, storage.clone(), thread, draft_id(32));
    let user = SyndicItemId::from_bytes([28; 16]);
    let turn = exact_cas::submit_current_draft(
        store,
        storage.clone(),
        thread,
        draft_id(31),
        user,
        "Discuss this",
        timestamp(2),
    );
    let source = exact_cas::establish_turn(store, storage.clone(), thread, turn, timestamp(2));
    exact_cas::admit_event(
        store,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(2),
    );
    exact_cas::correlate_user_item(
        store,
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
    exact_cas::admit_started_then_completed_item(
        store,
        storage.clone(),
        thread,
        turn,
        SyndicItemId::from_bytes([33; 16]),
        &source,
        CasItemId::new("discussion-source").unwrap(),
        assistant.clone(),
        assistant,
        timestamp(3),
        timestamp(3),
    );
    exact_cas::admit_event(
        store,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(TurnEndStatus::complete()),
        timestamp(4),
    );
    support::converge_and_release_terminal_history(store, storage.clone(), thread, turn);
}
