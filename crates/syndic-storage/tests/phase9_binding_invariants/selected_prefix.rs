use super::*;

#[test]
fn pending_root_rejects_a_binding_that_claims_the_undelivered_turn() {
    let home = TestHome::new("phase9-pending-root-prefix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn, selected) = root_pending(&store, storage);
    let represented =
        CasRepresentedPrefixProof::new(Some(turn), selected.thread_revision(), selected.digest());
    let request = valid_request(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("claims-pending-root").unwrap(),
        represented,
        CasLineageProof::native(NativeCasLineage::Continuation, represented).unwrap(),
    );

    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), request),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingPathConflict
    ));
    assert_eq!(
        current_binding_revision(&store, storage, thread),
        BindingRevision::new(2).unwrap()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn pending_non_root_rejects_non_parent_and_off_path_prefixes() {
    let home = TestHome::new("phase9-pending-child-prefix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, parent, _, selected) = non_root_pending(&store, storage);

    let empty = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let non_parent = valid_request(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("non-parent-prefix").unwrap(),
        empty,
        CasLineageProof::native(NativeCasLineage::Fresh, empty).unwrap(),
    );
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), non_parent),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingPathConflict
    ));

    let parent_record = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    let parent_prefix = CasRepresentedPrefixProof::new(
        Some(parent),
        selected.thread_revision(),
        parent_record.chain_digest(),
    );
    let empty_establishment = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let fresh_mismatch = valid_request(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("fresh-cannot-claim-parent").unwrap(),
        parent_prefix,
        CasLineageProof::native(NativeCasLineage::Fresh, empty_establishment).unwrap(),
    );
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), fresh_mismatch),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingPathConflict
    ));

    let sibling = SyndicTurnId::from_bytes([7; 16]);
    let sibling_digest = child_turn_chain_digest(sibling, parent, parent_record.chain_digest());
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::Turn(TurnRecord::new(
                sibling,
                thread,
                TurnKind::OrdinaryUser,
                ConversationParent::Turn(parent),
                Some(parent),
                TurnDepth::new(2).unwrap(),
                sibling_digest,
                timestamp(6),
            )),
            FixtureRecord::TurnState(fixture_turn_state(
                sibling,
                TurnStateRevision::FIRST,
                TurnLifecycle::Interrupted,
                1,
                0,
                timestamp(6),
            )),
            FixtureRecord::SourceEvent(
                SourceEventRecord::new(
                    sibling,
                    SourceEventSequence::FIRST,
                    None,
                    SourceEventPayload::TurnEnded(
                        TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
                    ),
                )
                .unwrap(),
            ),
            FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                parent,
                sibling,
                TurnDepth::new(2).unwrap(),
                sibling_digest,
            )),
        ]),
    );
    store.validate_registered_domains().unwrap();

    let off_path =
        CasRepresentedPrefixProof::new(Some(sibling), selected.thread_revision(), sibling_digest);
    let request = valid_request(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("off-path-prefix").unwrap(),
        off_path,
        CasLineageProof::native(NativeCasLineage::Continuation, off_path).unwrap(),
    );
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), request),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingPathConflict
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn live_or_terminal_unknown_tail_rejects_an_ordinary_full_prefix_binding() {
    let home = TestHome::new("phase9-live-tail-valid-binding");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn, selected) = root_pending(&store, storage);
    let full =
        CasRepresentedPrefixProof::new(Some(turn), selected.thread_revision(), selected.digest());

    let source = activate_exact_turn(&store, storage, thread, turn);
    admit_event(
        &store,
        storage,
        thread,
        turn,
        Some(source.clone()),
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    let active_claim = valid_request(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("source-less-active-claim").unwrap(),
        full,
        CasLineageProof::native(NativeCasLineage::Continuation, full).unwrap(),
    );
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), active_claim),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingStateConflict
    ));

    admit_event(
        &store,
        storage,
        thread,
        turn,
        Some(source),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::UnknownTerminal,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(5),
    );
    let binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding is not active");
    };
    let snapshot = storage
        .execution_snapshot(&store, active.snapshot_id(), point_limit())
        .unwrap()
        .unwrap();
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.record().loaded_generation()),
        "unknown active projection abandoned",
        timestamp(6),
    )
    .unwrap();
    execute(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            AbandonActiveBinding::new(
                thread,
                binding.binding().revision(),
                current_gate_revision(&store, storage, thread),
                selected,
                stale,
            ),
        ),
    )
    .unwrap();
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                selected,
                "unknown terminal has no usable projection",
            )
            .unwrap(),
        ),
    )
    .unwrap();
    let unknown_claim = valid_request(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("terminal-unknown-claim").unwrap(),
        full,
        CasLineageProof::native(NativeCasLineage::Continuation, full).unwrap(),
    );
    let error = execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), unknown_claim),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::TurnLifecycleConflict
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}
