use super::*;

fn empty_selected_path(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> SelectedPathProof {
    storage
        .current_binding(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .binding()
        .selected_path()
}

fn stale_binding(
    execution: ExecutionBinding,
    cas_thread: CasThreadId,
    usable: &UsableCasBinding,
    observed_at: SyndicTimestamp,
) -> StaleCasBinding {
    StaleCasBinding::new(
        execution,
        cas_thread,
        Some(usable.tool_profile()),
        Some(usable.represented_prefix()),
        Some(usable.lineage()),
        Some(usable.native_turn_count()),
        None,
        "retired canonical projection",
        observed_at,
    )
    .unwrap()
}

#[test]
fn retired_cas_identity_is_one_way_and_cannot_rewrite_its_execution() {
    let home = TestHome::new("phase9-one-way-retirement");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(220);
    create_thread(&store, storage, thread, draft_id(221));
    let selected = empty_selected_path(&store, storage, thread);
    let cas_thread = CasThreadId::new("one-way-retired-cas").unwrap();
    publish_valid(
        &store,
        storage,
        valid_request(&store, storage, thread, selected, cas_thread.clone()),
    );

    let different_execution = ExecutionBinding::new(
        RuntimeId::from_bytes([222; 16]),
        RootId::from_bytes([223; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase9-different-execution",
        )
        .unwrap(),
    );
    let before = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = before.binding().state() else {
        panic!("published binding was not valid")
    };
    let outcome = execute_outcome(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                selected,
                stale_binding(
                    different_execution,
                    cas_thread.clone(),
                    usable,
                    timestamp(2),
                ),
            ),
        ),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::ExecutionBindingConflict
    ));
    assert_eq!(
        storage
            .current_binding(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before
    );

    execute(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                selected,
                stale_binding(
                    execution_binding(),
                    cas_thread.clone(),
                    usable,
                    timestamp(2),
                ),
            ),
        ),
    );
    let retired = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(retired.binding().state(), BindingState::Stale(_)));

    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            valid_request(&store, storage, thread, selected, cas_thread),
        ),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::CasThreadRetired
    ));
    assert_eq!(
        storage
            .current_binding(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        retired
    );
    store.close().unwrap();
}

#[test]
fn first_stale_inclusive_fork_retains_exact_nonzero_provenance_after_reopen() {
    let home = TestHome::new("phase9-first-stale-inclusive-fork");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, Some(parent), _, selected) = fault_pending_path(&store, storage, 224, true) else {
        unreachable!()
    };
    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Fork, represented).unwrap();
    let native_count = CasNativeTurnCount::new(7);
    let cas_thread = CasThreadId::new("uncommitted-inclusive-fork").unwrap();
    execute(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                selected,
                StaleCasBinding::new(
                    storage
                        .thread_execution(&store, thread, point_limit())
                        .unwrap()
                        .unwrap()
                        .execution()
                        .clone(),
                    cas_thread.clone(),
                    Some(test_tool_profile()),
                    Some(represented),
                    Some(lineage),
                    Some(native_count),
                    None,
                    "valid fork publication did not commit",
                    timestamp(9),
                )
                .unwrap(),
            ),
        ),
    );
    let assert_provenance = |store: &HomeStore, storage: SyndicStorage| {
        let binding = storage
            .current_binding(store, thread, point_limit())
            .unwrap()
            .unwrap();
        let BindingState::Stale(stale) = binding.binding().state() else {
            panic!("inclusive fork retirement did not persist stale provenance")
        };
        assert_eq!(stale.cas_thread_id(), &cas_thread);
        assert_eq!(stale.observed_prefix(), Some(represented));
        assert_eq!(stale.observed_lineage(), Some(lineage));
        assert_eq!(stale.observed_native_turn_count(), Some(native_count));
    };
    assert_provenance(&store, storage);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_provenance(&reopened, storage);
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}
