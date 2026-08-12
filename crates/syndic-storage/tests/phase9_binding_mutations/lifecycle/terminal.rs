use super::*;

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
        gate.revision(),
        fixture.selected,
        fixture.snapshot,
        fixture.turn,
    );
    let error = not_committed_error(execute_outcome(
        &store,
        storage.cancel_binding_activation(storage.revision(&store).unwrap(), cancellation),
    ));
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingStateConflict
    ));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
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
    let error = not_committed_error(execute_outcome(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), source_less),
    ));
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
                gate.revision(),
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
    assert_eq!(
        gate.state(),
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
    converge_and_release_terminal_history(&store, storage, fixture.thread, fixture.turn);

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
    let error = not_committed_error(execute_outcome(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), changed_profile),
    ));
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
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let state = storage
        .turn_state(&reopened, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let sequence = SourceEventSequence::new(state.source_event_count()).unwrap();
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
    match reopened.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed terminal corruption, got {outcome:?}"),
    }
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
