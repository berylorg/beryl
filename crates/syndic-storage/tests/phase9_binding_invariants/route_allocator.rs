use super::*;

fn activate_empty_epoch(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    selected: SelectedPathProof,
    snapshot: SyndicExecutionSnapshotId,
    generation: CasLoadedSessionGeneration,
    started_at: SyndicTimestamp,
) -> AcceptedRouteGeneration {
    execute(
        store,
        storage.activate_binding(
            storage.revision(store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(store, storage, thread),
                current_gate_revision(store, storage, thread),
                selected,
                snapshot,
                turn,
                generation,
                started_at,
            ),
        ),
    );
    storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap()
        .selected_route()
        .unwrap()
        .generation()
}

fn cancel_empty_epoch(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    selected: SelectedPathProof,
    snapshot: SyndicExecutionSnapshotId,
) {
    execute(
        store,
        storage.cancel_binding_activation(
            storage.revision(store).unwrap(),
            CancelBindingActivation::new(
                thread,
                current_binding_revision(store, storage, thread),
                current_gate_revision(store, storage, thread),
                selected,
                snapshot,
                turn,
            ),
        ),
    );
}

fn seed_unselected_accepted_input(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    source_draft: SyndicDraftId,
) -> beryl_model::SyndicAcceptedInputId {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let history = storage
        .history_summary(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let next_thread_revision = thread_record.revision().checked_next().unwrap();
    let next_gate_revision = gate.revision().checked_next().unwrap();
    let generation = AcceptedRouteGeneration::FIRST;
    let ordinal = AcceptedInputOrdinal::FIRST;
    let input = source_draft.accepted_input_id();
    let (content, content_records) = composer_content_records(&ComposerPayload::default());
    let mut records = content_records;
    records.extend([
        FixtureRecord::Thread(ThreadRecord::new(
            thread,
            SelectedPathProof::new(
                thread_record.committed_tail(),
                next_thread_revision,
                thread_record.selected_path_digest(),
            ),
            current.draft().id(),
            thread_record.lineage(),
            thread_record.image_label_frontiers(),
            thread_record.context_owner_id(),
        )),
        FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            current.draft().id(),
            current.draft().revision(),
            next_thread_revision,
        )),
        FixtureRecord::HistorySummary(HistorySummaryRecord::new(
            thread,
            history.revision().checked_next().unwrap(),
            next_thread_revision,
            history.committed_tail(),
            history.selected_path_digest(),
            history.complete(),
            history.last_activity_at(),
        )),
        FixtureRecord::AcceptedInput(
            AcceptedInputRecord::new(
                input,
                thread,
                ordinal,
                AcceptedInputAdmissionProof::new(
                    thread_record.revision(),
                    source_draft,
                    beryl_model::DraftRevision::new(1).unwrap(),
                    gate.revision(),
                    current.draft().id(),
                )
                .unwrap(),
                generation,
                content,
                None,
                timestamp(1),
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
            thread, ordinal, input, generation,
        )),
        FixtureRecord::AcceptedRouteGeneration(
            AcceptedRouteGenerationRecord::new(
                thread,
                generation,
                AcceptedRouteRevision::FIRST,
                AcceptedRouteTarget::NextTurn(NextTurnReason::PendingTurn),
                Some(ordinal),
                Some(ordinal),
                1,
                0,
                0,
                1,
                0,
                content.summary().logical_utf8_bytes(),
                0,
            )
            .unwrap(),
        ),
        FixtureRecord::AcceptedRouteLeaf(AcceptedRouteLeafRecord::new(
            input,
            thread,
            generation,
            ordinal,
            beryl_model::AcceptedInputRevision::new(1).unwrap(),
            AcceptedRouteLeafState::NextTurn(NextTurnReason::PendingTurn),
            AcceptedInputLifecycle::Admitted,
        )),
        FixtureRecord::AcceptedNextSource(AcceptedNextSourceRecord::new(
            thread,
            generation,
            AcceptedRouteRevision::FIRST,
            ordinal,
            ordinal,
        )),
        FixtureRecord::InputGate(
            InputGateRecord::new(
                thread,
                next_gate_revision,
                gate.state().clone(),
                1,
                Some(generation),
                None,
                0,
                1,
                content.summary().logical_utf8_bytes(),
            )
            .unwrap(),
        ),
    ]);
    commit(store, storage, batch(records));
    input
}

#[test]
fn consecutive_empty_active_epochs_allocate_distinct_route_generations() {
    let home = TestHome::new("phase9-consecutive-empty-route-generations");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, None, turn, selected) = fault_pending_path(&store, storage, 120, false) else {
        unreachable!()
    };
    publish_valid(
        &store,
        storage,
        valid_request(
            &store,
            storage,
            thread,
            selected,
            CasThreadId::new("phase9-consecutive-empty-cas").unwrap(),
        ),
    );

    let first_snapshot = SyndicExecutionSnapshotId::from_bytes([123; 16]);
    assert_eq!(
        activate_empty_epoch(
            &store,
            storage,
            thread,
            turn,
            selected,
            first_snapshot,
            loaded_generation(41, 42),
            timestamp(5),
        ),
        AcceptedRouteGeneration::FIRST
    );
    cancel_empty_epoch(&store, storage, thread, turn, selected, first_snapshot);

    let second_generation = AcceptedRouteGeneration::new(2).unwrap();
    assert_eq!(
        activate_empty_epoch(
            &store,
            storage,
            thread,
            turn,
            selected,
            SyndicExecutionSnapshotId::from_bytes([124; 16]),
            loaded_generation(41, 43),
            timestamp(6),
        ),
        second_generation
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.route_generation_high_water(), Some(second_generation));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn unselected_generations_and_later_activation_share_one_route_allocator() {
    let home = TestHome::new("phase9-route-generation-interleaving");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, None, turn, selected) = fault_pending_path(&store, storage, 130, false) else {
        unreachable!()
    };
    let source_draft = draft_id(143);
    let accepted = seed_unselected_accepted_input(&store, storage, thread, source_draft);
    let first_generation = AcceptedRouteGeneration::FIRST;
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.route_generation_high_water(), Some(first_generation));
    assert!(gate.selected_route().is_none());
    assert_eq!(gate.live_next_turn_count(), 1);
    let accepted_record = storage
        .accepted_input(&store, accepted, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(accepted_record.route_generation(), first_generation);
    assert_eq!(accepted_record.admission().source_draft_id(), source_draft);
    assert_eq!(
        accepted_record.admission().replacement_draft_id(),
        storage
            .current_draft(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .draft()
            .id()
    );
    let first_page = storage
        .accepted_route_page(
            &store,
            thread,
            first_generation,
            AcceptedRouteRevision::FIRST,
            None,
        )
        .unwrap();
    assert_eq!(first_page.records().len(), 1);
    assert_eq!(first_page.records()[0].input().id(), accepted);
    assert_eq!(
        first_page.records()[0].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::PendingTurn)
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    publish_valid(
        &store,
        storage,
        valid_request(
            &store,
            storage,
            thread,
            selected,
            CasThreadId::new("phase9-shared-route-allocator-cas").unwrap(),
        ),
    );
    let second_generation = AcceptedRouteGeneration::new(2).unwrap();
    assert_eq!(
        activate_empty_epoch(
            &store,
            storage,
            thread,
            turn,
            selected,
            SyndicExecutionSnapshotId::from_bytes([135; 16]),
            loaded_generation(50, 53),
            timestamp(7),
        ),
        second_generation
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.route_generation_high_water(), Some(second_generation));
    assert_eq!(
        gate.selected_route().unwrap().generation(),
        second_generation
    );
    assert_eq!(gate.live_next_turn_count(), 1);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn route_generation_exhaustion_rejects_without_overwrite() {
    let home = TestHome::new("phase9-route-generation-exhaustion");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, None, turn, selected) = fault_pending_path(&store, storage, 230, false) else {
        unreachable!()
    };
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let exhausted = InputGateRecord::new(
        thread,
        gate.revision(),
        gate.state().clone(),
        gate.accepted_high_water(),
        Some(AcceptedRouteGeneration::new(u64::MAX).unwrap()),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::InputGate(exhausted)]),
    );
    publish_valid(
        &store,
        storage,
        valid_request(
            &store,
            storage,
            thread,
            selected,
            CasThreadId::new("phase9-route-exhaustion-cas").unwrap(),
        ),
    );
    let before_binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let before_gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let snapshot = SyndicExecutionSnapshotId::from_bytes([233; 16]);
    let outcome = execute_outcome(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                current_gate_revision(&store, storage, thread),
                selected,
                snapshot,
                turn,
                loaded_generation(24, 25),
                timestamp(6),
            ),
        ),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::Value(SyndicValueError::OrdinalExhausted {
            kind: "accepted-route generation"
        })
    ));
    assert_eq!(
        storage
            .current_binding(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );
    assert!(
        storage
            .execution_snapshot(&store, snapshot, point_limit())
            .unwrap()
            .is_none()
    );
    store.close().unwrap();
}
