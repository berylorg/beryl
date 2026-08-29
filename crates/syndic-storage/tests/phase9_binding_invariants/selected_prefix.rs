use super::*;

fn publish_live_event(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: Option<CasTurnSource>,
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
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        source,
        payload,
        observed_at,
    )
    .unwrap();
    execute(
        store,
        storage.admit_live_source_event(storage.revision(store).unwrap(), event),
    );
}

#[test]
fn pending_root_cannot_authenticate_the_undelivered_turn_as_represented_history() {
    let home = TestHome::new("phase9-pending-root-prefix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, None, turn, selected) = fault_pending_path(&store, &storage, 190, false) else {
        unreachable!()
    };
    let represented =
        CasRepresentedPrefixProof::new(Some(turn), selected.thread_revision(), selected.digest());
    let request = valid_request_with_count(
        &store,
        &storage,
        thread,
        selected,
        CasThreadId::new("claims-pending-root").unwrap(),
        represented,
        CasNativeTurnCount::ZERO,
        CasLineageProof::native(NativeCasLineage::Continuation, represented).unwrap(),
    );
    let before = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), request),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingPathConflict
    ));
    assert_eq!(
        storage
            .current_binding(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before
    );
    store.close().unwrap();
}

#[test]
fn pending_non_root_accepts_only_its_exact_authenticated_parent_prefix() {
    let home = TestHome::new("phase9-pending-child-prefix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, Some(parent), _, selected) = fault_pending_path(&store, &storage, 210, true)
    else {
        unreachable!()
    };
    let parent_record = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    let empty = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let empty_request = valid_request_with_count(
        &store,
        &storage,
        thread,
        selected,
        CasThreadId::new("non-parent-prefix").unwrap(),
        empty,
        CasNativeTurnCount::ZERO,
        CasLineageProof::native(NativeCasLineage::Fresh, empty).unwrap(),
    );
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), empty_request),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingPathConflict
    ));

    let sibling = SyndicTurnId::from_bytes([213; 16]);
    let sibling_digest = child_turn_chain_digest(sibling, parent, parent_record.chain_digest());
    commit(
        &store,
        storage.clone(),
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
                0,
                0,
                timestamp(6),
            )),
            FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                parent,
                sibling,
                TurnDepth::new(2).unwrap(),
                sibling_digest,
            )),
        ]),
    );
    let off_path =
        CasRepresentedPrefixProof::new(Some(sibling), selected.thread_revision(), sibling_digest);
    let off_path_request = valid_request_with_count(
        &store,
        &storage,
        thread,
        selected,
        CasThreadId::new("off-path-prefix").unwrap(),
        off_path,
        CasNativeTurnCount::ZERO,
        CasLineageProof::native(NativeCasLineage::Continuation, off_path).unwrap(),
    );
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), off_path_request),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingPathConflict
    ));

    let parent_prefix = CasRepresentedPrefixProof::new(
        Some(parent),
        selected.thread_revision(),
        parent_record.chain_digest(),
    );
    publish_valid(
        &store,
        &storage,
        valid_request_with_count(
            &store,
            &storage,
            thread,
            selected,
            CasThreadId::new("exact-parent-prefix").unwrap(),
            parent_prefix,
            CasNativeTurnCount::ZERO,
            CasLineageProof::native(NativeCasLineage::Continuation, parent_prefix).unwrap(),
        ),
    );
    assert!(matches!(
        storage
            .current_binding(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Valid(_)
    ));
    store.close().unwrap();
}

#[test]
fn live_and_unknown_terminal_tails_reject_ordinary_full_prefix_bindings() {
    let home = TestHome::new("phase9-live-and-unknown-tail-prefix");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, None, turn, selected) = fault_pending_path(&store, &storage, 214, false) else {
        unreachable!()
    };
    let activity_source = ActivityQuerySource::new(thread, turn);
    commit(
        &store,
        storage.clone(),
        batch([
            FixtureRecord::ActivityQueryHead(
                ActivityQueryHeadRecord::new(
                    thread,
                    ActivityWorkPeriod::FIRST,
                    Some(activity_source),
                    true,
                    0,
                    ActivityQueryRevision::FIRST,
                    1,
                    0,
                    0,
                    0,
                    0,
                    None,
                    ProjectionLifecycle::Current,
                )
                .unwrap(),
            ),
            FixtureRecord::ActivityQuerySource(ActivityQuerySourceRecord::new(
                thread,
                ActivityWorkPeriod::FIRST,
                activity_source,
                None,
                0,
                true,
                None,
            )),
        ]),
    );
    let full =
        CasRepresentedPrefixProof::new(Some(turn), selected.thread_revision(), selected.digest());
    let cas_thread = CasThreadId::new("live-tail-active-cas").unwrap();
    publish_valid(
        &store,
        &storage,
        valid_request(&store, &storage, thread, selected, cas_thread.clone()),
    );
    let snapshot = SyndicExecutionSnapshotId::from_bytes([217; 16]);
    execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(&store, &storage, thread),
                current_gate_revision(&store, &storage, thread),
                selected,
                snapshot,
                turn,
                loaded_generation(31, 32),
                timestamp(4),
            ),
        ),
    );
    let cas_turn = CasTurnId::new("live-tail-active-turn").unwrap();
    execute(
        &store,
        storage.publish_active_cas_turn(
            storage.revision(&store).unwrap(),
            PublishActiveCasTurn::new(
                thread,
                current_binding_revision(&store, &storage, thread),
                current_gate_revision(&store, &storage, thread),
                snapshot,
                cas_thread.clone(),
                cas_turn.clone(),
                timestamp(4),
            ),
        ),
    );
    let source = CasTurnSource::new(cas_thread.clone(), cas_turn.clone());
    publish_live_event(
        &store,
        &storage,
        thread,
        turn,
        Some(source.clone()),
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );

    let active_claim = valid_request_with_count(
        &store,
        &storage,
        thread,
        selected,
        CasThreadId::new("live-full-prefix-claim").unwrap(),
        full,
        CasNativeTurnCount::ZERO,
        CasLineageProof::native(NativeCasLineage::Continuation, full).unwrap(),
    );
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), active_claim),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingStateConflict
    ));

    publish_live_event(
        &store,
        &storage,
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
        panic!("unknown-terminal fixture binding is not active")
    };
    let persisted_snapshot = storage
        .execution_snapshot(&store, active.snapshot_id(), point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let published = storage
        .active_cas_turn(&store, active.snapshot_id(), point_limit())
        .unwrap()
        .unwrap();
    let target = AcceptedRouteLostTarget::AwaitingTerminal(SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            binding.binding().revision(),
            active.snapshot_id(),
            active.turn_id(),
            active.usable().cas_thread_id().clone(),
        ),
        published.cas_turn_id().clone(),
    ));
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(persisted_snapshot.loaded_generation()),
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
                gate.selected_route().unwrap().generation(),
                target,
                selected,
                stale,
            ),
        ),
    );
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                thread,
                current_binding_revision(&store, &storage, thread),
                selected,
                "unknown terminal has no usable projection",
            )
            .unwrap(),
        ),
    );

    let unknown_claim = valid_request_with_count(
        &store,
        &storage,
        thread,
        selected,
        CasThreadId::new("terminal-unknown-full-prefix-claim").unwrap(),
        full,
        CasNativeTurnCount::ZERO,
        CasLineageProof::native(NativeCasLineage::Continuation, full).unwrap(),
    );
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), unknown_claim),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::TurnLifecycleConflict
    ));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}
