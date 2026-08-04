use super::*;

#[test]
fn reopen_rejects_first_cas_membership_established_at_another_prefix() {
    let home = TestHome::new("phase9-first-membership-establishment");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, parent, _, selected) = non_root_pending(&store, storage);
    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let established = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let cas_thread = CasThreadId::new("corrupt-first-establishment").unwrap();
    let revision = current_binding_revision(&store, storage, thread)
        .checked_next()
        .unwrap();
    let usable = UsableCasBinding::new(
        execution_binding(),
        cas_thread.clone(),
        represented,
        beryl_model::CasNativeTurnCount::ZERO,
        test_tool_profile(),
        CasLineageProof::native(NativeCasLineage::Fresh, established).unwrap(),
    );
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::Binding(BindingRecord::new(
                thread,
                revision,
                selected,
                BindingState::valid(usable),
            )),
            FixtureRecord::BindingHead(BindingHeadRecord::new(
                thread,
                revision,
                BindingLifecycle::Valid,
                selected.digest(),
            )),
            FixtureRecord::CasThread(CasThreadIndexRecord::new(
                cas_thread.clone(),
                thread,
                revision,
            )),
            FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                cas_thread, thread, revision,
            )),
        ]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("mismatched first CAS establishment reopened successfully"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "first usable CAS membership was not established at its represented prefix"
            );
        }
        other => panic!("expected first-establishment rejection, got {other:?}"),
    }
    reopened.close().unwrap();
}

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

fn stale_binding(cas_thread: CasThreadId, observed_at: SyndicTimestamp) -> StaleCasBinding {
    StaleCasBinding::new(
        execution_binding(),
        cas_thread,
        None,
        None,
        None,
        None,
        None,
        "abandoned fixture",
        observed_at,
    )
    .unwrap()
}

#[test]
fn first_stale_fork_retains_its_exact_nonzero_native_position() {
    let home = TestHome::new("phase10-first-stale-fork-position");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, parent, _, selected) = non_root_pending(&store, storage);
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
    let stale = StaleCasBinding::new(
        execution_binding(),
        CasThreadId::new("uncommitted-inclusive-fork").unwrap(),
        Some(test_tool_profile()),
        Some(represented),
        Some(lineage),
        Some(beryl_model::CasNativeTurnCount::new(7)),
        None,
        "valid fork publication did not commit",
        timestamp(9),
    )
    .unwrap();
    execute(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                selected,
                stale,
            ),
        ),
    )
    .unwrap();
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn stale_cas_thread_cannot_be_reused_by_the_same_syndic_thread() {
    let home = TestHome::new("phase9-same-thread-stale-reservation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(20);
    create_thread(&store, storage, thread, draft_id(21));
    let selected = empty_selected_path(&store, storage, thread);
    let cas_thread = CasThreadId::new("same-thread-abandoned-cas").unwrap();
    execute(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                selected,
                stale_binding(cas_thread.clone(), timestamp(2)),
            ),
        ),
    )
    .unwrap();
    store.validate_registered_domains().unwrap();

    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let reuse = valid_request(
        &store,
        storage,
        thread,
        selected,
        cas_thread,
        represented,
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    );
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), reuse),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::CasThreadRetired
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn retirement_cannot_rewrite_an_existing_cas_execution() {
    let home = TestHome::new("phase9-retirement-continuity");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(25);
    create_thread(&store, storage, thread, draft_id(26));
    let selected = empty_selected_path(&store, storage, thread);
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let cas_thread = CasThreadId::new("retirement-continuity-cas").unwrap();
    publish_valid(
        &store,
        storage,
        valid_request(
            &store,
            storage,
            thread,
            selected,
            cas_thread.clone(),
            represented,
            CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
        ),
    );
    let different_execution = ExecutionBinding::new(
        RuntimeId::from_bytes([92; 16]),
        RootId::from_bytes([93; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\different-phase9-binding",
        )
        .unwrap(),
    );
    let stale = StaleCasBinding::new(
        different_execution,
        cas_thread,
        Some(test_tool_profile()),
        None,
        None,
        Some(beryl_model::CasNativeTurnCount::ZERO),
        None,
        "mismatched retirement",
        timestamp(2),
    )
    .unwrap();
    let request = PublishStaleBinding::new(
        thread,
        current_binding_revision(&store, storage, thread),
        selected,
        stale,
    );
    let error = execute(
        &store,
        storage.publish_stale_binding(storage.revision(&store).unwrap(), request),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::ExecutionBindingConflict
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn cas_thread_reservation_survives_stale_unbound_history_and_reopen() {
    let home = TestHome::new("phase9-permanent-cas-thread-reservation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let owner = id(30);
    let contender = id(40);
    create_thread(&store, storage, owner, draft_id(31));
    create_thread(&store, storage, contender, draft_id(41));
    let owner_selected = empty_selected_path(&store, storage, owner);
    let contender_selected = empty_selected_path(&store, storage, contender);
    let cas_thread = CasThreadId::new("permanently-reserved-cas").unwrap();

    execute(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                owner,
                current_binding_revision(&store, storage, owner),
                owner_selected,
                stale_binding(cas_thread.clone(), timestamp(2)),
            ),
        ),
    )
    .unwrap();

    let represented = CasRepresentedPrefixProof::new(
        None,
        contender_selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let contender_request = || {
        valid_request(
            &store,
            storage,
            contender,
            contender_selected,
            cas_thread.clone(),
            represented,
            CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
        )
    };
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), contender_request()),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::CasThreadOwnershipConflict
    ));

    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                owner,
                current_binding_revision(&store, storage, owner),
                owner_selected,
                "projection abandoned",
            )
            .unwrap(),
        ),
    )
    .unwrap();
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), contender_request()),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::CasThreadOwnershipConflict
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let request = valid_request(
        &reopened,
        storage,
        contender,
        contender_selected,
        cas_thread,
        represented,
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    );
    let error = execute(
        &reopened,
        storage.publish_valid_binding(storage.revision(&reopened).unwrap(), request),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::CasThreadOwnershipConflict
    ));
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}
