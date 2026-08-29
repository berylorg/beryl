use super::*;

fn terminal_event(
    store: &HomeStore,
    storage: &SyndicStorage,
    fixture: &ActiveFixture,
    source: Option<CasTurnSource>,
    outcome: TurnTerminalOutcome,
    observed_at: SyndicTimestamp,
) -> LiveSourceEvent {
    let state = storage
        .turn_state(store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        source,
        SourceEventPayload::TurnEnded(TurnEndStatus::new(outcome, None).unwrap()),
        observed_at,
    )
    .unwrap()
}

fn complete_active_terminal(
    store: &HomeStore,
    storage: &SyndicStorage,
    thread_byte: u8,
) -> ActiveFixture {
    let fixture = activate_pending(store, storage, thread_byte, false);
    execute(
        store,
        storage.publish_active_cas_turn(
            storage.revision(store).unwrap(),
            PublishActiveCasTurn::new(
                fixture.thread,
                current_binding_revision(store, storage, fixture.thread),
                current_gate_revision(store, storage, fixture.thread),
                fixture.snapshot,
                fixture.cas_thread.clone(),
                fixture.cas_turn.clone(),
                timestamp(6),
            ),
        ),
    );
    execute(
        store,
        storage.admit_live_source_event(
            storage.revision(store).unwrap(),
            terminal_event(
                store,
                storage,
                &fixture,
                Some(CasTurnSource::new(
                    fixture.cas_thread.clone(),
                    fixture.cas_turn.clone(),
                )),
                TurnTerminalOutcome::Complete,
                timestamp(7),
            ),
        ),
    );
    fixture
}

#[test]
fn activation_reconciles_prior_then_exact_and_reopens_cleanly() {
    let home = TestHome::new("phase9-current-binding-activation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, _, turn, selected) = same_home_pending_path(&store, &storage, 70);
    let cas_thread = CasThreadId::new("phase9-current-activation").unwrap();
    let valid = valid_request(&store, &storage, thread, selected, cas_thread);
    assert_eq!(
        storage
            .valid_binding_publication_status(&store, &valid, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid.clone()),
    );
    assert_eq!(
        storage
            .valid_binding_publication_status(&store, &valid, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );

    let snapshot = SyndicExecutionSnapshotId::from_bytes([72; 16]);
    let activation = ActivateBinding::new(
        thread,
        current_binding_revision(&store, &storage, thread),
        current_gate_revision(&store, &storage, thread),
        selected,
        snapshot,
        turn,
        loaded_generation(40, 41),
        timestamp(5),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activation.clone()),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let immutable_snapshot = storage
        .execution_snapshot(&store, snapshot, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        immutable_snapshot.activation_gate_revision(),
        current_gate_revision(&store, &storage, thread)
    );
    assert_eq!(
        immutable_snapshot.represented_base_native_turn_count(),
        CasNativeTurnCount::ZERO
    );
    assert_eq!(immutable_snapshot.tool_profile(), valid.tool_profile());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .binding_activation_status(&reopened, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn queued_admission_descendant_preserves_activation_reconciliation() {
    let home = TestHome::new("phase9-current-queued-descendant-activation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, _, turn, projected_path) = same_home_pending_path(&store, &storage, 80);
    let valid = valid_request(
        &store,
        &storage,
        thread,
        projected_path,
        CasThreadId::new("phase9-current-descendant-activation").unwrap(),
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid),
    );
    let accepted = seed_queued_input(&store, &storage, thread, draft_id(93));
    let current = storage
        .thread(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let current_path = SelectedPathProof::new(
        current.committed_tail(),
        current.revision(),
        current.selected_path_digest(),
    );
    assert!(current_path.is_compatible_descendant_of(projected_path));
    assert_ne!(
        current_path.thread_revision(),
        projected_path.thread_revision()
    );
    let admitted = storage
        .accepted_input(&store, accepted, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        admitted.admission().expected_thread_revision(),
        ThreadRevision::new(1).unwrap()
    );
    assert_eq!(
        admitted.admission().expected_gate_revision(),
        InputGateRevision::new(1).unwrap()
    );

    let activation = ActivateBinding::new(
        thread,
        current_binding_revision(&store, &storage, thread),
        current_gate_revision(&store, &storage, thread),
        current_path,
        SyndicExecutionSnapshotId::from_bytes([82; 16]),
        turn,
        loaded_generation(42, 43),
        timestamp(6),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activation.clone()),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn cancelled_activation_reconciles_prior_then_exact_and_survives_reopen() {
    let home = TestHome::new("phase9-current-activation-cancellation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, &storage, 100, false);
    let cancellation = CancelBindingActivation::new(
        fixture.thread,
        current_binding_revision(&store, &storage, fixture.thread),
        current_gate_revision(&store, &storage, fixture.thread),
        fixture.selected,
        fixture.snapshot,
        fixture.turn,
    );
    assert_eq!(
        storage
            .cancelled_binding_activation_status(&store, &cancellation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.cancel_binding_activation(storage.revision(&store).unwrap(), cancellation.clone()),
    );
    assert_eq!(
        storage
            .cancelled_binding_activation_status(&store, &cancellation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("cancelled activation did not restore a valid binding");
    };
    assert_eq!(usable.cas_thread_id(), &fixture.cas_thread);
    assert_eq!(usable.native_turn_count(), CasNativeTurnCount::ZERO);
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(fixture.turn));
    assert!(
        storage
            .active_cas_turn(&store, fixture.snapshot, point_limit())
            .unwrap()
            .is_none()
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .cancelled_binding_activation_status(&reopened, &cancellation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn cancellation_rejects_after_cas_turn_publication() {
    let home = TestHome::new("phase9-current-cancellation-after-cas-turn");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, &storage, 110, true);
    let before_binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let before_gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let outcome = execute_outcome(
        &store,
        storage.cancel_binding_activation(
            storage.revision(&store).unwrap(),
            CancelBindingActivation::new(
                fixture.thread,
                before_binding.binding().revision(),
                before_gate.revision(),
                fixture.selected,
                fixture.snapshot,
                fixture.turn,
            ),
        ),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingStateConflict
    ));
    assert_eq!(
        storage
            .current_binding(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

#[test]
fn exact_terminal_cas_authority_advances_native_count_once() {
    let home = TestHome::new("phase9-current-terminal-cas-authority");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_pending(&store, &storage, 120, false);
    let source_less = terminal_event(
        &store,
        &storage,
        &fixture,
        None,
        TurnTerminalOutcome::Complete,
        timestamp(6),
    );
    let outcome = execute_outcome(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), source_less),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::SourceIdentityConflict
    ));

    execute(
        &store,
        storage.publish_active_cas_turn(
            storage.revision(&store).unwrap(),
            PublishActiveCasTurn::new(
                fixture.thread,
                current_binding_revision(&store, &storage, fixture.thread),
                current_gate_revision(&store, &storage, fixture.thread),
                fixture.snapshot,
                fixture.cas_thread.clone(),
                fixture.cas_turn.clone(),
                timestamp(6),
            ),
        ),
    );
    execute(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            terminal_event(
                &store,
                &storage,
                &fixture,
                Some(CasTurnSource::new(
                    fixture.cas_thread.clone(),
                    fixture.cas_turn.clone(),
                )),
                TurnTerminalOutcome::Complete,
                timestamp(7),
            ),
        ),
    );
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("terminal CAS authority did not publish a valid binding");
    };
    assert_eq!(usable.represented_prefix().tail(), Some(fixture.turn));
    assert_eq!(
        usable.represented_prefix().digest(),
        fixture.selected.digest()
    );
    assert_eq!(usable.native_turn_count().get(), 1);
    assert_eq!(usable.tool_profile(), fixture.valid.tool_profile());
    let owner = storage
        .cas_turn_owner(
            &store,
            fixture.cas_thread.clone(),
            fixture.cas_turn.clone(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(owner.post_turn_native_count().get(), 1);
    assert_eq!(
        storage
            .input_gate(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::FinalizingHistory(fixture.turn)
    );
    assert_eq!(
        storage
            .valid_binding_publication_status(&store, &fixture.valid, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &fixture.activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let binding = storage
        .current_binding(&reopened, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("terminal CAS authority did not survive reopen");
    };
    assert_eq!(usable.native_turn_count().get(), 1);
    reopened.close().unwrap();
}

#[test]
fn source_less_terminal_requires_projection_unbinding() {
    let home = TestHome::new("phase9-current-source-less-terminal");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, _, turn, selected) = same_home_pending_path(&store, &storage, 130);
    let valid = valid_request(
        &store,
        &storage,
        thread,
        selected,
        CasThreadId::new("phase9-source-less-parent").unwrap(),
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid),
    );
    let state = storage
        .turn_state(&store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let terminal = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::FIRST,
        None,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(6),
    )
    .unwrap();
    let outcome = execute_outcome(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), terminal.clone()),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::SourceIdentityConflict
    ));
    let binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                thread,
                binding.binding().revision(),
                selected,
                "source-less terminal has no projection authority",
            )
            .unwrap(),
        ),
    );
    execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), terminal),
    );
    assert_eq!(
        storage
            .turn_state(&store, turn, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Interrupted
    );
    assert_eq!(
        storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::FinalizingHistory(turn)
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_terminal_valid_successor_with_wrong_native_count() {
    let home = TestHome::new("phase9-terminal-native-count-corruption");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = complete_active_terminal(&store, &storage, 160);
    let current = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("terminal fixture did not become valid");
    };
    assert_eq!(usable.native_turn_count().get(), 1);
    let corrupt = UsableCasBinding::new(
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        usable.represented_prefix(),
        CasNativeTurnCount::new(2),
        usable.tool_profile(),
        usable.lineage(),
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::Binding(BindingRecord::new(
            current.binding().thread_id(),
            current.binding().revision(),
            current.binding().selected_path(),
            BindingState::valid(corrupt),
        ))]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("valid active successor lacks exact terminal CAS authority"),
        "unexpected terminal native-count scrub error: {error}"
    );
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_source_less_event_claiming_external_activity() {
    let home = TestHome::new("phase9-source-less-external-corruption");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = complete_active_terminal(&store, &storage, 170);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                fixture.turn,
                SourceEventSequence::new(state.source_event_count()).unwrap(),
                None,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(
                        TurnTerminalOutcome::Complete,
                        Some(TurnIncompleteReason::ItemAuditFailed),
                    )
                    .unwrap(),
                ),
            )
            .unwrap(),
        )]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    let error = reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source-less event claims external turn activity"),
        "unexpected source-less external scrub error: {error}"
    );
    reopened.close().unwrap();
}

#[test]
fn post_terminal_continuation_preserves_profile_and_reconciliation_history() {
    let home = TestHome::new("phase9-post-terminal-continuation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = complete_active_terminal(&store, &storage, 180);
    let terminal_binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(terminal_usable) = terminal_binding.binding().state() else {
        panic!("terminal fixture did not become valid");
    };
    let terminal_native_count = terminal_usable.native_turn_count();
    let child = SyndicTurnId::from_bytes([184; 16]);
    let child_selected = seed_child_pending_after_terminal(&store, &storage, &fixture, child);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(fixture.turn),
        child_selected.thread_revision(),
        fixture.selected.digest(),
    );
    let continuation = PublishValidBinding::new(
        fixture.thread,
        current_binding_revision(&store, &storage, fixture.thread),
        child_selected,
        fixture.valid.execution().clone(),
        fixture.cas_thread.clone(),
        represented,
        terminal_native_count,
        fixture.valid.tool_profile(),
        fixture.valid.lineage(),
    );
    let changed_profile = PublishValidBinding::new(
        continuation.thread_id(),
        continuation.expected_binding_revision(),
        continuation.selected_path(),
        continuation.execution().clone(),
        continuation.cas_thread_id().clone(),
        continuation.represented_prefix(),
        continuation.native_turn_count(),
        beryl_model::CasConversationToolProfile::v1([0x3c; 32]),
        continuation.lineage(),
    );
    let before_binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let before_gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let outcome = execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), changed_profile),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingStateConflict
    ));
    assert_eq!(
        storage
            .current_binding(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    assert_eq!(
        storage
            .input_gate(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        before_gate
    );
    assert_eq!(
        storage
            .valid_binding_publication_status(&store, &continuation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), continuation.clone()),
    );
    assert_eq!(
        storage
            .valid_binding_publication_status(&store, &continuation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &fixture.activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .valid_binding_publication_status(&reopened, &continuation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .binding_activation_status(&reopened, &fixture.activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    reopened.close().unwrap();
}
