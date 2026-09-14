use super::*;
use beryl_home_store::{CommandOutcome, ReconciliationResolution, test_faults::FaultPoint};
use beryl_model::{
    CasConversationToolProfile, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
    CasNativeTurnCount, CasProcessGeneration, CasThreadId, SyndicExecutionSnapshotId,
};
use syndic_storage::{
    ActivateBinding, CancelBindingActivation, CasLineageProof, CasRepresentedPrefixProof,
    NativeCasLineage, PublishValidBinding, SelectedPathProof, empty_selected_path_digest,
};

fn activate(fixture: &Fixture) -> CancelBindingActivation {
    let home = fixture.service.home.as_deref().unwrap();
    let storage = &fixture.service.storage;
    let pending = storage
        .pending_dispatch_evidence(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let thread = storage
        .thread(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    );
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let execution = storage
        .thread_execution(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    execute(
        home,
        storage.publish_valid_binding(
            storage.revision(home).unwrap(),
            PublishValidBinding::new(
                fixture.thread,
                pending.binding_revision(),
                selected,
                execution,
                CasThreadId::new("shutdown-reconciliation").unwrap(),
                represented,
                CasNativeTurnCount::ZERO,
                CasConversationToolProfile::v1([0xa5; 32]),
                CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
            ),
        ),
    );
    let binding = storage
        .current_binding(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(home, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes([99; 16]);
    execute(
        home,
        storage.activate_binding(
            storage.revision(home).unwrap(),
            ActivateBinding::new(
                fixture.thread,
                binding.head().revision(),
                gate.revision(),
                state.revision(),
                selected,
                snapshot,
                fixture.turn,
                CasLoadedSessionGeneration::new(
                    CasProcessGeneration::new(99).unwrap(),
                    CasLoadedThreadGeneration::new(99).unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(5),
            ),
        ),
    );
    CancelBindingActivation::new(
        fixture.thread,
        storage
            .current_binding(home, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .head()
            .revision(),
        storage
            .input_gate(home, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .revision(),
        storage
            .turn_state(home, fixture.turn, point_limit())
            .unwrap()
            .unwrap()
            .revision(),
        selected,
        snapshot,
        fixture.turn,
    )
}
