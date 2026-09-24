use super as support;
use beryl_home_store::HomeStore;
use beryl_model::*;
use sha2::{Digest, Sha256};
use syndic_storage::{test_faults::*, *};

pub fn seed(
    store: &HomeStore,
    storage: SyndicStorage,
    text: &str,
) -> (
    AcceptedInputRecord,
    DiscussionHandoffReceipt,
    DiscussionHandoffGateRecord,
) {
    seed_parent_history(store, &storage);
    support::discussion_creation::create_child(store, &storage, support::id(30), 36, 37);
    let request = support::discussion_handoff::active_request(
        &store,
        &storage,
        ResolutionIntentId::from_bytes([231; 16]),
        JobId::from_bytes([231; 16]),
    );
    let parent = request.parent.thread_id;
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let admission = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let pending = admission.intent().new_gate();
    support::discussion_input::committed(&store, admission.contribution());
    let item_id = SyndicItemId::from_bytes([232; 16]);
    let turn_id = support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        parent,
        SyndicDraftId::from_bytes([233; 16]),
        item_id,
        &format!("Discussion resolution:\n\n{text}"),
        support::timestamp(100),
    );
    let turn = storage.turn(&store, turn_id, limit).unwrap().unwrap();
    let item = storage
        .canonical_item(&store, item_id, limit)
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(&store, parent, limit).unwrap().unwrap();
    let proof = DiscussionHandoffReceipt {
        parent_thread_revision: request.parent.thread_revision,
        parent_gate_revision: request.parent.input_gate_revision,
        child_thread_id: request.thread_id,
        intent_id: request.intent_id,
        job_id: request.job_id,
        context_owner: request.context_owner,
        context_digest: request.context_digest,
        resolving_turn_id: request.resolving_target.pending().active_turn_id(),
        resolution_digest: Sha256::digest(text.as_bytes()).into(),
        parent_turn_id: turn_id,
        canonical_item_id: item_id,
    };
    let input = AcceptedInputRecord::new(
        SyndicAcceptedInputId::from_bytes([231; 16]),
        parent,
        AcceptedInputOrdinal::FIRST,
        AcceptedInputSource::DiscussionHandoff(proof),
        item.presentation().content().unwrap(),
        None,
        turn.submitted_at(),
    )
    .unwrap();
    let generated = CanonicalItemRecord::local_discussion_handoff(
        item_id,
        turn_id,
        item.ordinal(),
        item.revision(),
        input.content(),
        input.id(),
    );
    let batch = support::batch([
        FixtureRecord::Turn(TurnRecord::new(
            turn_id,
            parent,
            TurnKind::BerylDiscussionHandoff,
            turn.parent(),
            turn.ancestor_skip(),
            turn.depth(),
            turn.chain_digest(),
            turn.submitted_at(),
        )),
        FixtureRecord::CanonicalItem(generated.clone()),
        FixtureRecord::AcceptedInput(input.clone()),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::from_source(
            parent,
            input.ordinal(),
            input.id(),
            AcceptedOrderSource::DiscussionHandoff,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                parent,
                gate.revision(),
                gate.state().clone(),
                1,
                gate.route_generation_high_water(),
                gate.selected_route(),
                0,
                0,
                0,
            )
            .unwrap(),
        ),
    ]);
    support::commit(&store, storage.clone(), batch);
    (input, proof, pending)
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
