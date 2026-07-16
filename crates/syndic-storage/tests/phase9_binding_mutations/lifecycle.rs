use super::*;

#[test]
fn valid_activation_and_one_way_turn_publication_are_atomic_and_reopen_cleanly() {
    let home = TestHome::new("phase9-binding-lifecycle");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    let replacement = draft_id(3);
    create_thread(&store, storage, thread, draft);
    save_text(&store, storage, thread, "start", timestamp(2));
    let (turn, selected) = submit_root_turn(
        &store,
        storage,
        thread,
        draft,
        replacement,
        SyndicItemId::from_bytes([4; 16]),
        timestamp(3),
    );

    let cas_thread = CasThreadId::new("phase9-active-thread").unwrap();
    let valid = valid_request(thread, selected, cas_thread.clone());
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

    let snapshot = SyndicExecutionSnapshotId::from_bytes([5; 16]);
    let activate = ActivateBinding::new(
        thread,
        BindingRevision::new(3).unwrap(),
        InputGateRevision::new(2).unwrap(),
        selected,
        snapshot,
        turn,
        loaded_generation(),
        timestamp(4),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activate, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activate.clone()),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activate, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let immutable_snapshot = storage
        .execution_snapshot(&store, snapshot, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    assert_eq!(
        immutable_snapshot.activation_gate_revision(),
        InputGateRevision::new(3).unwrap()
    );
    assert_eq!(
        immutable_snapshot.represented_base_native_turn_count(),
        beryl_model::CasNativeTurnCount::ZERO
    );
    assert_eq!(immutable_snapshot.tool_profile(), valid.tool_profile());

    save_text(&store, storage, thread, "steer", timestamp(5));
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        thread,
        current.thread().revision(),
        replacement,
        current.draft().revision(),
        current.draft().content(),
        InputGateRevision::new(3).unwrap(),
        draft_id(6),
        AdmissionMarkers::default(),
        timestamp(6),
    );
    let accepted = admission.accepted_input_id();
    execute(
        &store,
        storage.admit_accepted_input(storage.revision(&store).unwrap(), admission),
    );

    let publish = PublishActiveCasTurn::new(
        thread,
        BindingRevision::new(4).unwrap(),
        InputGateRevision::new(4).unwrap(),
        snapshot,
        cas_thread.clone(),
        CasTurnId::new("phase9-active-turn").unwrap(),
        timestamp(7),
    );
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &publish, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Absent
    );
    execute(
        &store,
        storage.publish_active_cas_turn(storage.revision(&store).unwrap(), publish.clone()),
    );
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &publish, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Exact
    );
    let cas_turn_index = storage
        .cas_turn_owner(
            &store,
            publish.cas_thread_id().clone(),
            publish.cas_turn_id().clone(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(cas_turn_index.record().post_turn_native_count().get(), 1);

    let input = storage
        .accepted_input(&store, accepted, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(input.record().revision().get(), 2);
    let AcceptedInputDisposition::SteerActiveTurn(target) = input.record().disposition() else {
        panic!("awaiting input was not rewritten to its exact active turn");
    };
    assert_eq!(target.cas_turn_id(), publish.cas_turn_id());
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().revision(), InputGateRevision::new(5).unwrap());
    assert!(matches!(
        gate.record().state(),
        InputGateState::Steerable(_)
    ));
    assert_eq!(
        storage
            .execution_snapshot(&store, snapshot, point_limit())
            .unwrap()
            .unwrap()
            .record(),
        &immutable_snapshot
    );
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&reopened, &publish, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Exact
    );
    reopened.close().unwrap();
}

#[test]
fn proven_not_dispatched_activation_returns_to_the_same_valid_projection() {
    let home = TestHome::new("phase13-binding-activation-cancellation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(31);
    let draft = draft_id(32);
    let replacement = draft_id(33);
    create_thread(&store, storage, thread, draft);
    save_text(&store, storage, thread, "not dispatched", timestamp(2));
    let (turn, selected) = submit_root_turn(
        &store,
        storage,
        thread,
        draft,
        replacement,
        SyndicItemId::from_bytes([34; 16]),
        timestamp(3),
    );
    let cas_thread = CasThreadId::new("phase13-cancelled-start").unwrap();
    let valid = valid_request(thread, selected, cas_thread.clone());
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid),
    );
    let snapshot = SyndicExecutionSnapshotId::from_bytes([35; 16]);
    let activation = ActivateBinding::new(
        thread,
        BindingRevision::new(3).unwrap(),
        InputGateRevision::new(2).unwrap(),
        selected,
        snapshot,
        turn,
        loaded_generation(),
        timestamp(4),
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activation),
    );
    let cancellation = CancelBindingActivation::new(
        thread,
        BindingRevision::new(4).unwrap(),
        InputGateRevision::new(3).unwrap(),
        selected,
        snapshot,
        turn,
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
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("cancelled activation did not restore a valid binding");
    };
    assert_eq!(usable.cas_thread_id(), &cas_thread);
    assert_eq!(
        usable.native_turn_count(),
        beryl_model::CasNativeTurnCount::ZERO
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().revision(), InputGateRevision::new(4).unwrap());
    assert_eq!(gate.record().state(), &InputGateState::PendingTurn(turn));
    assert!(
        storage
            .active_cas_turn(&store, snapshot, point_limit())
            .unwrap()
            .is_none()
    );
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .cancelled_binding_activation_status(&reopened, &cancellation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    reopened.close().unwrap();
}

#[test]
fn activation_cancellation_rejects_any_published_cas_turn() {
    let home = TestHome::new("phase13-cancellation-after-cas-turn");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_root_turn(&store, storage, true);
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let cancellation = CancelBindingActivation::new(
        fixture.thread,
        BindingRevision::new(4).unwrap(),
        gate.record().revision(),
        fixture.selected,
        fixture.snapshot,
        fixture.turn,
    );
    let error = execute_result(
        &store,
        storage.cancel_binding_activation(storage.revision(&store).unwrap(), cancellation),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingStateConflict
    ));
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn active_terminal_requires_the_exact_published_cas_turn() {
    let home = TestHome::new("phase9-active-terminal-authority");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_root_turn(&store, storage, false);

    let source_less = terminal_event(
        &store,
        storage,
        &fixture,
        None,
        TurnTerminalOutcome::Complete,
        timestamp(5),
    );
    let error = execute_result(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), source_less),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));
    assert!(matches!(
        storage
            .current_binding(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Active(_)
    ));

    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.publish_active_cas_turn(
            storage.revision(&store).unwrap(),
            PublishActiveCasTurn::new(
                fixture.thread,
                binding.binding().revision(),
                gate.record().revision(),
                fixture.snapshot,
                fixture.cas_thread.clone(),
                fixture.cas_turn.clone(),
                timestamp(6),
            ),
        ),
    );
    let correlated = terminal_event(
        &store,
        storage,
        &fixture,
        Some(CasTurnSource::new(
            fixture.cas_thread.clone(),
            fixture.cas_turn.clone(),
        )),
        TurnTerminalOutcome::Complete,
        timestamp(7),
    );
    execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), correlated),
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
    let terminal_native_turn_count = usable.native_turn_count();
    assert_eq!(terminal_native_turn_count.get(), 1);
    assert_eq!(usable.tool_profile(), fixture.valid.tool_profile());
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().state(), &InputGateState::Idle);
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

    save_text(
        &store,
        storage,
        fixture.thread,
        "continue on the same CAS lineage",
        timestamp(8),
    );
    let draft = storage
        .current_draft(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .draft()
        .id();
    let (_child, child_selected) = submit_root_turn(
        &store,
        storage,
        fixture.thread,
        draft,
        draft_id(26),
        SyndicItemId::from_bytes([27; 16]),
        timestamp(9),
    );
    let represented = CasRepresentedPrefixProof::new(
        Some(fixture.turn),
        child_selected.thread_revision(),
        fixture.selected.digest(),
    );
    let continuation = PublishValidBinding::new(
        fixture.thread,
        storage
            .current_binding(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .revision(),
        child_selected,
        fixture.valid.execution().clone(),
        fixture.cas_thread.clone(),
        represented,
        terminal_native_turn_count,
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
    let error = execute_result(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), changed_profile),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingStateConflict
    ));
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
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    let state = storage
        .turn_state(&reopened, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let sequence = SourceEventSequence::new(state.record().source_event_count()).unwrap();
    let mut corruption = FixtureBatch::new();
    corruption
        .put(FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                fixture.turn,
                sequence,
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
        ))
        .unwrap();
    let mut command = HomeCommand::new(reopened.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(&reopened).unwrap(), corruption))
        .unwrap();
    reopened.execute(command).unwrap();
    reopened.close().unwrap();

    let mut rejected = open(home.path());
    let error = match SyndicStorage::register(&mut rejected) {
        Ok(_) => panic!("source-less terminal authority registered successfully"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "source-less event claims external turn activity"
            );
        }
        other => panic!("expected source-less terminal authority rejection, got {other:?}"),
    }
    rejected.close().unwrap();
}

#[test]
fn reopen_rejects_terminal_native_count_that_did_not_advance_once() {
    let home = TestHome::new("phase10-terminal-native-count-corruption");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_root_turn(&store, storage, false);
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.publish_active_cas_turn(
            storage.revision(&store).unwrap(),
            PublishActiveCasTurn::new(
                fixture.thread,
                binding.binding().revision(),
                gate.record().revision(),
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
        beryl_model::CasNativeTurnCount::new(2),
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
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("corrupt terminal native count reopened successfully"),
        Err(error) => error,
    };
    let DomainRegistrationError::Validation { domain, source } = error else {
        panic!("expected terminal native-count validation failure, got {error:?}");
    };
    assert_eq!(domain, "syndic");
    assert_eq!(
        source.to_string(),
        "valid active successor lacks exact terminal CAS authority"
    );
    reopened.close().unwrap();
}
