use beryl_app::conversation_tools::ConversationToolRegistry;
use beryl_home_store::HomeStore;
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount,
    CasProcessGeneration, CasThreadId, CasTurnId, RuntimeId, SyndicExecutionSnapshotId,
    SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    ActivateBinding, BindingState, CancelBindingActivation, CasLineageProof,
    CasRepresentedPrefixProof, DeliveryRecoveryCase, NativeCasLineage, PublishActiveCasTurn,
    PublishValidBinding, SyndicStorage,
};

use crate::{
    app_support::{execute, point_limit, startup_source, time},
    phase62_support::execution_binding,
};

pub struct ActiveSeed {
    pub snapshot: SyndicExecutionSnapshotId,
    pub cas_thread: CasThreadId,
    pub cas_turn: Option<CasTurnId>,
}

pub fn activate_promoted_turn(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    seed: u8,
    publish_cas_turn: bool,
) -> ActiveSeed {
    activate_promoted_turn_at(
        store,
        storage,
        thread,
        turn,
        seed,
        publish_cas_turn,
        time(63_030),
    )
}

pub fn activate_promoted_turn_at(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    seed: u8,
    publish_cas_turn: bool,
    started_at: syndic_storage::SyndicTimestamp,
) -> ActiveSeed {
    let current = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    assert_eq!(selected.tail(), Some(turn));
    let turn_record = storage.turn(store, turn, point_limit()).unwrap().unwrap();
    let (parent, parent_digest) = match turn_record.parent().turn() {
        Some(parent) => {
            let parent = storage.turn(store, parent, point_limit()).unwrap().unwrap();
            (Some(parent.id()), parent.chain_digest())
        }
        None => (None, syndic_storage::empty_selected_path_digest()),
    };
    let represented =
        CasRepresentedPrefixProof::new(parent, selected.thread_revision(), parent_digest);
    let lineage = CasLineageProof::native(
        if parent.is_some() {
            NativeCasLineage::Fork
        } else {
            NativeCasLineage::Fresh
        },
        represented,
    )
    .unwrap();
    let cas_thread = CasThreadId::new(format!("phase63-app-thread-{seed}")).unwrap();
    execute(
        store,
        storage.publish_valid_binding(
            storage.revision(store).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                execution_binding(RuntimeId::from_bytes([seed; 16])),
                cas_thread.clone(),
                represented,
                CasNativeTurnCount::ZERO,
                ConversationToolRegistry::canonical().profile(),
                lineage,
            ),
        ),
    );
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Valid(_)));
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes([seed.wrapping_add(2); 16]);
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
                loaded_generation(seed),
                started_at,
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
        let cas_turn = CasTurnId::new(format!("phase63-app-turn-{seed}")).unwrap();
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
                    started_at,
                ),
            ),
        );
        cas_turn
    });
    let source = startup_source(store, storage);
    let DeliveryRecoveryCase::Active(active) = storage
        .classify_delivery_recovery(store, &source, point_limit())
        .unwrap()
    else {
        panic!("activated promoted turn must classify as active recovery");
    };
    assert_eq!(active.thread_id(), thread);
    assert_eq!(active.turn_id(), turn);
    assert_eq!(active.snapshot_id(), snapshot);
    assert_eq!(active.cas_thread_id(), &cas_thread);
    assert_eq!(
        storage
            .active_cas_turn(store, snapshot, point_limit())
            .unwrap()
            .is_some(),
        publish_cas_turn
    );
    ActiveSeed {
        snapshot,
        cas_thread,
        cas_turn,
    }
}

fn loaded_generation(seed: u8) -> CasLoadedSessionGeneration {
    CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(u64::from(seed) + 1).unwrap(),
        CasLoadedThreadGeneration::new(1).unwrap(),
    )
}

pub fn cancel_activation(
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
    let binding = storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Valid(_)));
}
