use super::{draft_id, exact_cas, id, timestamp};
use beryl_home_store::HomeStore;
use beryl_model::{JobId, ResolutionIntentId, SyndicExecutionSnapshotId, SyndicItemId};
use syndic_storage::*;

pub fn active_request(
    store: &HomeStore,
    storage: &SyndicStorage,
    intent_id: ResolutionIntentId,
    job_id: JobId,
) -> AdmitDiscussionHandoff {
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let turn = exact_cas::submit_current_draft(
        store,
        storage.clone(),
        id(36),
        draft_id(201),
        SyndicItemId::from_bytes([202; 16]),
        "Resolve this discussion",
        timestamp(10),
    );
    let source = exact_cas::establish_turn(store, storage.clone(), id(36), turn, timestamp(11));
    let thread = storage.thread(store, id(36), limit).unwrap().unwrap();
    let gate = storage.input_gate(store, id(36), limit).unwrap().unwrap();
    let binding = storage
        .current_binding(store, id(36), limit)
        .unwrap()
        .unwrap();
    let owner = thread.context_owner_id().unwrap();
    let context = storage
        .context_envelope(store, owner, limit)
        .unwrap()
        .unwrap();
    let parent = storage.thread(store, id(30), limit).unwrap().unwrap();
    let parent_gate = storage.input_gate(store, id(30), limit).unwrap().unwrap();
    AdmitDiscussionHandoff {
        thread_id: id(36),
        thread_revision: thread.revision(),
        attributes_revision: storage
            .thread_attributes(store, id(36), limit)
            .unwrap()
            .unwrap()
            .revision(),
        input_gate_revision: gate.revision(),
        handoff_gate_revision: DiscussionHandoffGateRevision::FIRST,
        turn_state_revision: storage
            .turn_state(store, turn, limit)
            .unwrap()
            .unwrap()
            .revision(),
        resolving_target: SteeringTargetProof::new(
            PendingSteeringTargetProof::new(
                binding.binding().revision(),
                SyndicExecutionSnapshotId::from_bytes(*turn.as_bytes()),
                turn,
                source.thread_id().clone(),
            ),
            source.turn_id().clone(),
        ),
        parent: DiscussionParentFrontierProof {
            thread_id: parent.id(),
            thread_revision: parent.revision(),
            input_gate_revision: parent_gate.revision(),
            accepted_high_water: parent_gate.accepted_high_water(),
        },
        context_owner: owner,
        context_digest: context.envelope().descriptor().digest(),
        intent_id,
        job_id,
    }
}

pub fn admit(
    store: &HomeStore,
    storage: &SyndicStorage,
    intent_id: ResolutionIntentId,
    job_id: JobId,
) {
    let request = active_request(store, storage, intent_id, job_id);
    let prepared = storage
        .prepare_discussion_handoff(store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let mut command = beryl_home_store::HomeCommand::new(store.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

pub fn complete_resolving_turn(store: &HomeStore, storage: &SyndicStorage) {
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let current = storage
        .current_binding(store, id(36), limit)
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = current.binding().state() else {
        panic!("expected active discussion")
    };
    let turn = active.turn_id();
    let route = storage
        .active_cas_turn(store, active.snapshot_id(), limit)
        .unwrap()
        .unwrap();
    let source = CasTurnSource::new(route.cas_thread_id().clone(), route.cas_turn_id().clone());
    let at = storage
        .current_draft(store, id(36), limit)
        .unwrap()
        .unwrap()
        .draft()
        .updated_at()
        .unix_millis()
        .max(
            storage
                .turn_state(store, turn, limit)
                .unwrap()
                .unwrap()
                .updated_at()
                .unix_millis(),
        )
        + 1;
    exact_cas::admit_event(
        store,
        storage.clone(),
        id(36),
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(at),
    );
    exact_cas::correlate_user_item(
        store,
        storage.clone(),
        id(36),
        turn,
        SyndicItemId::from_bytes([202; 16]),
        &source,
        timestamp(at + 1),
    );
    exact_cas::admit_event(
        store,
        storage.clone(),
        id(36),
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(at + 2),
    );
    exact_cas::converge_and_release_terminal_history(store, storage.clone(), id(36), turn);
}
