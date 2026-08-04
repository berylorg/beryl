use super::*;

#[test]
fn pre_activation_abandonment_retires_projection_and_preserves_queued_input() {
    let home = TestHome::new("phase9-active-abandonment");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_root_turn(&store, storage, true);

    save_text(
        &store,
        storage,
        fixture.thread,
        "preserve after projection loss",
        timestamp(6),
    );
    let current = storage
        .current_draft(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        fixture.thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        draft_id(25),
        None,
        timestamp(7),
    );
    let accepted = admission.accepted_input_id();
    execute(
        &store,
        storage.admit_accepted_input(storage.revision(&store).unwrap(), admission),
    );

    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding is not active");
    };
    let snapshot = storage
        .execution_snapshot(&store, fixture.snapshot, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let target = AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            binding.binding().revision(),
            active.snapshot_id(),
            active.turn_id(),
            active.usable().cas_thread_id().clone(),
        ),
        fixture.cas_turn.clone(),
    ));
    let wrong_stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count().checked_next().unwrap()),
        Some(snapshot.loaded_generation()),
        "active CAS projection lost",
        timestamp(8),
    )
    .unwrap();
    let error = execute_result(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            AbandonActiveBinding::new(
                fixture.thread,
                binding.binding().revision(),
                gate.selected_route().unwrap().generation(),
                target.clone(),
                fixture.selected,
                wrong_stale,
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingStateConflict
    ));
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.loaded_generation()),
        "active CAS projection lost",
        timestamp(8),
    )
    .unwrap();
    let abandon = AbandonActiveBinding::new(
        fixture.thread,
        binding.binding().revision(),
        gate.selected_route().unwrap().generation(),
        target,
        fixture.selected,
        stale,
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &abandon, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.abandon_active_binding(storage.revision(&store).unwrap(), abandon.clone()),
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&store, &abandon, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );

    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(binding.binding().state(), BindingState::Stale(_)));
    let owner = storage
        .cas_thread_owner(&store, fixture.cas_thread.clone(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        owner.retired_binding_revision(),
        Some(binding.binding().revision())
    );
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(fixture.turn));
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 1);
    let proof = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(
            &store,
            fixture.thread,
            proof.generation(),
            proof.revision(),
            None,
        )
        .unwrap();
    let input = page
        .records()
        .iter()
        .find(|row| row.input().id() == accepted)
        .unwrap();
    assert_eq!(
        input.effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(input.leaf().lifecycle(), AcceptedInputLifecycle::Admitted);

    let represented = CasRepresentedPrefixProof::new(
        None,
        fixture.selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let retry_thread = CasThreadId::new("phase9-pre-activation-retry").unwrap();
    let retry = PublishValidBinding::new(
        fixture.thread,
        binding.binding().revision(),
        fixture.selected,
        execution_binding(),
        retry_thread,
        represented,
        beryl_model::CasNativeTurnCount::ZERO,
        test_tool_profile(),
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), retry),
    );
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let retry_snapshot = SyndicExecutionSnapshotId::from_bytes([26; 16]);
    execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                fixture.thread,
                binding.binding().revision(),
                gate.revision(),
                fixture.selected,
                retry_snapshot,
                fixture.turn,
                loaded_generation(),
                timestamp(9),
            ),
        ),
    );
    let state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(gate.state(), InputGateState::AwaitingSteering(_)));
    assert_eq!(gate.live_next_turn_count(), 1);
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_idle_gate_after_abandoned_turn_becomes_unbound() {
    let home = TestHome::new("phase9-abandoned-gate-correlation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_root_turn(&store, storage, true);
    let state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute_result(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            LiveSourceEvent::new(
                fixture.thread,
                fixture.turn,
                state.revision(),
                gate.revision(),
                SourceEventSequence::FIRST,
                Some(CasTurnSource::new(
                    fixture.cas_thread.clone(),
                    fixture.cas_turn.clone(),
                )),
                SourceEventPayload::TurnActivated,
                timestamp(6),
            )
            .unwrap(),
        ),
    )
    .expect("exact activation is admitted before projection loss");
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding is not active");
    };
    let snapshot = storage
        .execution_snapshot(&store, fixture.snapshot, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let target = AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            binding.binding().revision(),
            active.snapshot_id(),
            active.turn_id(),
            active.usable().cas_thread_id().clone(),
        ),
        fixture.cas_turn.clone(),
    ));
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.loaded_generation()),
        "active CAS projection lost",
        timestamp(7),
    )
    .unwrap();
    execute_result(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            AbandonActiveBinding::new(
                fixture.thread,
                binding.binding().revision(),
                gate.selected_route().unwrap().generation(),
                target,
                fixture.selected,
                stale,
            ),
        ),
    )
    .expect("active projection is abandoned");
    let state = storage
        .turn_state(&store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let stale_projection_activation = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(CasTurnSource::new(
            fixture.cas_thread.clone(),
            fixture.cas_turn.clone(),
        )),
        SourceEventPayload::TurnActivated,
        timestamp(8),
    )
    .unwrap();
    let error = execute_result(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            stale_projection_activation,
        ),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));
    let source_less_complete = terminal_event(
        &store,
        storage,
        &fixture,
        None,
        TurnTerminalOutcome::Complete,
        timestamp(8),
    );
    let error = execute_result(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), source_less_complete),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));
    execute_result(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            terminal_event(
                &store,
                storage,
                &fixture,
                None,
                TurnTerminalOutcome::UnknownTerminal,
                timestamp(8),
            ),
        ),
    )
    .expect("source-less unknown-terminal convergence is admitted after abandonment");
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute_result(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                fixture.thread,
                binding.binding().revision(),
                fixture.selected,
                "abandoned projection has no usable lineage",
            )
            .unwrap(),
        ),
    )
    .expect("stale projection may publish an unbound successor");
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        None,
        fixture.selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let error = execute_result(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                fixture.thread,
                binding.binding().revision(),
                fixture.selected,
                execution_binding(),
                CasThreadId::new("phase9-post-activation-no-replay").unwrap(),
                represented,
                beryl_model::CasNativeTurnCount::ZERO,
                test_tool_profile(),
                CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::TurnLifecycleConflict
    ));
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let corrupt = InputGateRecord::new(
        gate.thread_id(),
        gate.revision(),
        InputGateState::Idle,
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    commit(&store, storage, batch([FixtureRecord::InputGate(corrupt)]));
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("blocking abandoned turn reopened with an idle gate"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "idle input gate leaves committed turn blocking"
            );
        }
        other => panic!("expected abandoned-gate rejection, got {other:?}"),
    }
    reopened.close().unwrap();
}
