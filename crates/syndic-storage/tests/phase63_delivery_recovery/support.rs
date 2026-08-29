use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount,
    CasProcessGeneration, CasThreadId, CasTurnId, SyndicDraftId, SyndicExecutionSnapshotId,
    SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    ActivateBinding, BindingState, CancelBindingActivation, CasLineageProof,
    CasRepresentedPrefixProof, NativeCasLineage, PublishActiveCasTurn, PublishStaleBinding,
    PublishValidBinding, StaleCasBinding, SyndicPointReadLimit, SyndicStorage,
};

use crate::support::{
    TestHome, batch, commit, exact_cas, open, seed_canonical_empty_thread, timestamp,
};

pub struct RecoveryHome {
    pub home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
    pub turn: SyndicTurnId,
}

pub struct ActiveFixture {
    pub snapshot: SyndicExecutionSnapshotId,
    pub cas_thread: CasThreadId,
    pub cas_turn: Option<CasTurnId>,
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

pub fn ordered_id(value: u64) -> SyndicThreadId {
    let mut bytes = [0_u8; 16];
    bytes[8..].copy_from_slice(&value.to_be_bytes());
    SyndicThreadId::from_bytes(bytes)
}

pub fn ordered_draft(value: u64) -> SyndicDraftId {
    SyndicDraftId::from_bytes(*ordered_id(value).as_bytes())
}

pub fn pending_home(name: &str, value: u64) -> RecoveryHome {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = ordered_id(value);
    seed_canonical_empty_thread(
        &store,
        storage.clone(),
        thread,
        ordered_draft(value + 10_000),
    );
    let turn = exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        thread,
        ordered_draft(value + 20_000),
        SyndicItemId::from_bytes(*ordered_id(value + 30_000).as_bytes()),
        "pending recovery",
        timestamp(3),
    );
    RecoveryHome {
        home,
        store,
        storage,
        thread,
        turn,
    }
}

pub fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean phase-63 fixture command, got {outcome:?}"),
    }
}

pub fn loaded_generation() -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(1).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

pub fn activate(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    publish_cas_turn: bool,
) -> ActiveFixture {
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    assert_eq!(selected.tail(), Some(turn));
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        syndic_storage::empty_selected_path_digest(),
    );
    let cas_thread = CasThreadId::new(format!("phase63-thread-{turn}")).unwrap();
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    execute(
        store,
        storage.publish_valid_binding(
            storage.revision(store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                exact_cas::execution_binding(),
                cas_thread.clone(),
                represented,
                CasNativeTurnCount::ZERO,
                exact_cas::tool_profile(),
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
                gate.revision(),
                selected,
                snapshot,
                turn,
                loaded_generation(),
                timestamp(5),
            ),
        ),
    );
    let cas_turn = publish_cas_turn.then(|| {
        let binding = storage
            .current_binding(store, thread, point_limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(store, thread, point_limit())
            .unwrap()
            .unwrap();
        let cas_turn = CasTurnId::new(format!("phase63-turn-{turn}")).unwrap();
        execute(
            store,
            storage.publish_active_cas_turn(
                storage.revision(store).unwrap(),
                PublishActiveCasTurn::new(
                    thread,
                    binding.binding().revision(),
                    gate.revision(),
                    snapshot,
                    cas_thread.clone(),
                    cas_turn.clone(),
                    timestamp(6),
                ),
            ),
        );
        cas_turn
    });
    ActiveFixture {
        snapshot,
        cas_thread,
        cas_turn,
    }
}

pub fn cancel_active(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    snapshot: SyndicExecutionSnapshotId,
) {
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.cancel_binding_activation(
            storage.revision(store).unwrap(),
            CancelBindingActivation::new(
                thread,
                binding.binding().revision(),
                gate.revision(),
                binding.binding().selected_path(),
                snapshot,
                turn,
            ),
        ),
    );
}

pub fn publish_stale_valid(store: &HomeStore, storage: SyndicStorage, thread: SyndicThreadId) {
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("safe-pending fixture binding is not valid");
    };
    let stale = StaleCasBinding::new(
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        Some(usable.tool_profile()),
        Some(usable.represented_prefix()),
        Some(usable.lineage()),
        Some(usable.native_turn_count()),
        None,
        "phase63 valid projection retired before dispatch",
        timestamp(5),
    )
    .unwrap();
    execute(
        store,
        storage.publish_stale_binding(
            storage.revision(store).unwrap(),
            PublishStaleBinding::new(
                thread,
                binding.binding().revision(),
                binding.binding().selected_path(),
                stale,
            ),
        ),
    );
}

pub fn replace_gate_state(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    state: syndic_storage::InputGateState,
) {
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let replacement = syndic_storage::InputGateRecord::new(
        thread,
        gate.revision().checked_next().unwrap(),
        state,
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    commit(
        store,
        storage,
        batch([FixtureRecord::InputGate(replacement)]),
    );
}

pub fn startup_source(
    store: &HomeStore,
    storage: SyndicStorage,
) -> syndic_storage::DeliveryRecoverySource {
    let page = storage
        .delivery_recovery_startup_page(
            store,
            None,
            beryl_home_store::CursorReadLimits::new(
                syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
                syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    page.records()[0].clone()
}
