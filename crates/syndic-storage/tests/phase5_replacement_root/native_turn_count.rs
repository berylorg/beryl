use super::*;

use beryl_model::{
    CasNativeTurnCount, CasThreadId, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath,
};

fn execution_binding() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([87; 16]),
        RootId::from_bytes([88; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase10-root-fresh",
        )
        .unwrap(),
    )
}

fn replace_root_turn(store: &HomeStore, storage: SyndicStorage) -> SyndicTurnId {
    let selected = SelectedPathProof::new(
        Some(root_turn()),
        ThreadRevision::new(1).unwrap(),
        root_turn_chain_digest(root_turn()),
    );
    execute(
        store,
        storage.start_replacement_edit(
            storage.revision(store).unwrap(),
            StartReplacementEdit::new(
                thread(),
                ThreadRevision::new(1).unwrap(),
                draft(),
                DraftRevision::new(1).unwrap(),
                InputGateRevision::new(1).unwrap(),
                root_turn(),
                root_item(),
                selected,
                CurrentTranscriptEntryProof::new(
                    TranscriptGeneration::FIRST,
                    TranscriptPosition::FIRST,
                ),
                AdmissionMarkers::default(),
                timestamp(3),
            ),
        ),
    );

    let editing = storage
        .current_draft(store, thread(), point_limit())
        .unwrap()
        .unwrap();
    let replacement_turn = draft().submitted_turn_id();
    execute(
        store,
        storage.submit_idle_draft(
            storage.revision(store).unwrap(),
            IdleSubmission::new(
                thread(),
                ThreadRevision::new(1).unwrap(),
                draft(),
                DraftRevision::new(2).unwrap(),
                editing.draft().content(),
                InputGateRevision::new(1).unwrap(),
                draft_id(85),
                SyndicItemId::from_bytes([86; 16]),
                AdmissionMarkers::default(),
                timestamp(4),
            ),
        ),
    );
    replacement_turn
}

#[test]
fn native_resume_preserves_count_and_root_replacement_starts_fresh_at_zero() {
    let home = TestHome::new("phase10-root-native-count");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, root_fixture());

    let selected = SelectedPathProof::new(
        Some(root_turn()),
        ThreadRevision::new(1).unwrap(),
        root_turn_chain_digest(root_turn()),
    );
    let represented = CasRepresentedPrefixProof::new(
        Some(root_turn()),
        selected.thread_revision(),
        selected.digest(),
    );
    let cas_thread = CasThreadId::new("phase10-root-native-count").unwrap();
    let initial_count = CasNativeTurnCount::new(3);
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                thread(),
                BindingRevision::new(1).unwrap(),
                selected,
                execution_binding(),
                cas_thread.clone(),
                represented,
                initial_count,
                test_tool_profile(),
                CasLineageProof::native(NativeCasLineage::Fork, represented).unwrap(),
            ),
        ),
    );
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                thread(),
                BindingRevision::new(2).unwrap(),
                selected,
                execution_binding(),
                cas_thread,
                represented,
                initial_count,
                test_tool_profile(),
                CasLineageProof::native(NativeCasLineage::Resume, represented).unwrap(),
            ),
        ),
    );

    let replacement_turn = replace_root_turn(&store, storage);
    let current = storage
        .current_binding(&store, thread(), point_limit())
        .unwrap()
        .unwrap();
    let replacement_selected = current.binding().selected_path();
    assert_eq!(replacement_selected.tail(), Some(replacement_turn));
    let empty = CasRepresentedPrefixProof::new(
        None,
        replacement_selected.thread_revision(),
        empty_selected_path_digest(),
    );
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                thread(),
                current.binding().revision(),
                replacement_selected,
                execution_binding(),
                CasThreadId::new("phase10-root-fresh-replacement").unwrap(),
                empty,
                CasNativeTurnCount::ZERO,
                test_tool_profile(),
                CasLineageProof::native(NativeCasLineage::Fresh, empty).unwrap(),
            ),
        ),
    );

    let fresh = storage
        .current_binding(&store, thread(), point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(fresh) = fresh.binding().state() else {
        panic!("fresh replacement did not publish a valid binding");
    };
    assert_eq!(fresh.represented_prefix().tail(), None);
    assert_eq!(fresh.native_turn_count(), CasNativeTurnCount::ZERO);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}
