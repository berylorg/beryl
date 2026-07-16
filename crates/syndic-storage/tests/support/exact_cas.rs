use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    BindingRevision, CasConversationToolProfile, CasItemId, CasLoadedSessionGeneration,
    CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration, CasThreadId, CasTurnId,
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ActivateBinding, BindingState, CasLineageProof, CasRepresentedPrefixProof, CasTurnSource,
    LiveSourceEvent, NativeCasLineage, ProviderItemKind, PublishActiveCasTurn, PublishValidBinding,
    SourceEventPayload, SourceEventSequence, SourceItemDescriptor, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, empty_selected_path_digest,
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

pub fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([246; 16]),
        RootId::from_bytes([247; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-test-exact-cas",
        )
        .unwrap(),
    )
}

pub const fn tool_profile() -> CasConversationToolProfile {
    CasConversationToolProfile::v1([0xa5; 32])
}

fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(1).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

/// Establishes exact CAS authority for the current pending turn without admitting activation.
pub fn establish_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    started_at: SyndicTimestamp,
) -> CasTurnSource {
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    assert_eq!(selected.tail(), Some(turn));
    let turn_record = storage.turn(store, turn, point_limit()).unwrap().unwrap();
    let (parent, parent_digest) = match turn_record.record().parent().turn() {
        Some(parent) => {
            let parent = storage.turn(store, parent, point_limit()).unwrap().unwrap();
            (Some(parent.record().id()), parent.record().chain_digest())
        }
        None => (None, empty_selected_path_digest()),
    };
    let represented =
        CasRepresentedPrefixProof::new(parent, selected.thread_revision(), parent_digest);
    let prior = current
        .binding()
        .revision()
        .get()
        .checked_sub(1)
        .and_then(|revision| BindingRevision::new(revision).ok())
        .and_then(|revision| {
            storage
                .binding(store, thread, revision, point_limit())
                .unwrap()
        });
    let (cas_thread, lineage, native_turn_count) =
        match prior.as_ref().map(|record| record.record().state()) {
            Some(BindingState::Valid(usable))
                if usable.represented_prefix().tail() == represented.tail()
                    && usable.represented_prefix().digest() == represented.digest() =>
            {
                (
                    usable.cas_thread_id().clone(),
                    usable.lineage(),
                    usable.native_turn_count(),
                )
            }
            _ => (
                CasThreadId::new(format!("test-thread-{turn}")).unwrap(),
                CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
                CasNativeTurnCount::ZERO,
            ),
        };
    execute(
        store,
        storage.publish_valid_binding(
            storage.revision(store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution_binding(),
                cas_thread.clone(),
                represented,
                native_turn_count,
                tool_profile(),
                lineage,
            ),
        ),
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes(*turn.as_bytes());
    execute(
        store,
        storage.activate_binding(
            storage.revision(store).unwrap(),
            ActivateBinding::new(
                thread,
                binding.binding().revision(),
                gate.record().revision(),
                selected,
                snapshot,
                turn,
                loaded_generation(),
                started_at,
            ),
        ),
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let cas_turn = CasTurnId::new(format!("test-turn-{turn}")).unwrap();
    execute(
        store,
        storage.publish_active_cas_turn(
            storage.revision(store).unwrap(),
            PublishActiveCasTurn::new(
                thread,
                binding.binding().revision(),
                gate.record().revision(),
                snapshot,
                cas_thread.clone(),
                cas_turn.clone(),
                started_at,
            ),
        ),
    );
    CasTurnSource::new(cas_thread, cas_turn)
}

/// Admits one event under the exact currently published CAS turn authority.
pub fn admit_event(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        thread,
        turn,
        state.record().revision(),
        gate.record().revision(),
        SourceEventSequence::new(state.record().source_event_count() + 1).unwrap(),
        Some(source.clone()),
        payload,
        observed_at,
    )
    .unwrap();
    execute(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), event),
    );
}

/// Correlates and closes the submitted local user item under exact CAS authority.
pub fn correlate_user_item(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item_id: SyndicItemId,
    source: &CasTurnSource,
    observed_at: SyndicTimestamp,
) {
    let item = storage
        .canonical_item(store, item_id, point_limit())
        .unwrap()
        .unwrap();
    let descriptor = SourceItemDescriptor::new(
        item_id,
        CasItemId::new(format!("test-user-{item_id}")).unwrap(),
        ProviderItemKind::UserMessage,
        item.record().disposition(),
    )
    .unwrap();
    admit_event(
        store,
        storage,
        thread,
        turn,
        source,
        SourceEventPayload::ItemStarted {
            item: descriptor.clone(),
            assistant_phase: None,
        },
        observed_at,
    );
    admit_event(
        store,
        storage,
        thread,
        turn,
        source,
        SourceEventPayload::ItemCompleted {
            item: descriptor,
            assistant_phase: None,
        },
        observed_at,
    );
}
