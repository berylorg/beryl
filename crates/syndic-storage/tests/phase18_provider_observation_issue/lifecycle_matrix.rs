use super::*;

fn canonical_snapshot(fixture: &Fixture) -> Option<CanonicalItemRecord> {
    fixture
        .storage
        .canonical_item(&fixture.store, fixture.assistant, limit())
        .unwrap()
        .map(|item| item.clone())
}

fn inspect_sealed(
    fixture: &Fixture,
    sealed: SealedProviderObservationHandle,
) -> InspectedProviderObservation {
    let route = ProviderObservationRoute::new(
        fixture.source.thread_id().clone(),
        fixture.source.turn_id().clone(),
    );
    let bound = sealed.bind(route.clone(), route).unwrap();
    inspect_provider_observation(&fixture.storage, &fixture.store, bound, limit()).unwrap()
}

fn inspect_agent_item(
    fixture: &Fixture,
    observation_byte: u8,
    lifecycle: ProviderObservationItemLifecycle,
    observed_at: u64,
    item_id: &CasItemId,
    value: &str,
) -> InspectedProviderObservation {
    let sealed = {
        let mut callback = observation_callback(&fixture.store, fixture.storage);
        let mut stager = committed_stage_value(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([observation_byte; 16]),
                ProviderObservationBegin::Item {
                    lifecycle,
                    kind: ProviderObservationItemKind::AgentMessage,
                },
                &mut callback,
            )
            .unwrap(),
        );
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(observed_at),
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::ItemId,
            item_id.as_str(),
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::AgentMessageText,
            value,
            &mut callback,
        );
        committed_seal_value(stager.seal(&mut callback).unwrap())
    };
    inspect_sealed(fixture, sealed)
}

fn inspect_subagent_start(
    fixture: &Fixture,
    observation_byte: u8,
    observed_at: u64,
    item_id: &CasItemId,
) -> InspectedProviderObservation {
    let sealed = {
        let mut callback = observation_callback(&fixture.store, fixture.storage);
        let mut stager = committed_stage_value(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([observation_byte; 16]),
                ProviderObservationBegin::Item {
                    lifecycle: ProviderObservationItemLifecycle::Started,
                    kind: ProviderObservationItemKind::SubAgentActivity,
                },
                &mut callback,
            )
            .unwrap(),
        );
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(observed_at),
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::ItemId,
            item_id.as_str(),
            &mut callback,
        );
        enum_value(
            &mut stager,
            ProviderField::SubAgentKind,
            ProviderEnumValue::SubAgentStarted,
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::SubAgentThreadId,
            "phase18-worker",
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::SubAgentPath,
            "root/phase18-worker",
            &mut callback,
        );
        committed_seal_value(stager.seal(&mut callback).unwrap())
    };
    inspect_sealed(fixture, sealed)
}

fn publish_issue(
    fixture: &Fixture,
    inspected: InspectedProviderObservation,
    reason: ProviderObservationIssueReason,
    observed_at: SyndicTimestamp,
) -> SourceEventSequence {
    let canonical_before = canonical_snapshot(fixture);
    let state_before = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    let issue = inspected.into_issue(reason);
    assert_eq!(issue.reason(), reason);
    let event = next_event(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::ProviderObservationIssue(Box::new(issue.clone())),
        observed_at,
    );
    committed_command(execute(
        &fixture.store,
        fixture.storage.admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            event.clone(),
        ),
    ));

    assert_eq!(canonical_snapshot(fixture), canonical_before);
    let state_after = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(state_after.item_count(), state_before.item_count());
    let stored = fixture
        .storage
        .source_event(&fixture.store, fixture.turn, event.sequence(), limit())
        .unwrap()
        .unwrap();
    let SourceEventPayload::ProviderObservationIssue(stored_issue) = stored.payload() else {
        panic!("published source event is not a provider-observation issue")
    };
    assert_eq!(stored_issue.as_ref(), &issue);
    event.sequence()
}

fn complete_agent_item(fixture: &Fixture) {
    exact_cas::admit_item_frame(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        fixture.turn,
        fixture.assistant,
        &fixture.source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(2).unwrap(),
            fixture.cas_item.clone(),
            ProviderItemObservationV1::Completed {
                observed_at: ProviderLifecycleTimestampMsV1::new(6),
                item: agent_value("canonical"),
            },
        ),
        timestamp(6),
    );
}

fn abandon_active_projection(fixture: &Fixture) {
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding is not active")
    };
    let snapshot = fixture
        .storage
        .execution_snapshot(&fixture.store, active.snapshot_id(), limit())
        .unwrap()
        .unwrap();
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.loaded_generation()),
        "phase18 source authority lost",
        timestamp(7),
    )
    .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let target = AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            binding.binding().revision(),
            active.snapshot_id(),
            active.turn_id(),
            active.usable().cas_thread_id().clone(),
        ),
        fixture.source.turn_id().clone(),
    ));
    committed_command(execute(
        &fixture.store,
        fixture.storage.abandon_active_binding(
            fixture.storage.revision(&fixture.store).unwrap(),
            AbandonActiveBinding::new(
                fixture.thread,
                binding.binding().revision(),
                gate.selected_route().unwrap().generation(),
                target,
                binding.binding().selected_path(),
                stale,
            ),
        ),
    ));
}

#[test]
fn completion_only_started_issue_is_exact_and_does_not_create_a_canonical_item() {
    let fixture = setup("phase18-issue-completion-only-started");
    let item_id = CasItemId::new("phase18-completion-only-started").unwrap();
    let preparation = prepare_provider_observation_frame(
        &fixture.storage,
        &fixture.store,
        inspect_subagent_start(&fixture, 39, 5, &item_id),
        ProviderObservationFramePreparationPlan::first(
            fixture.assistant,
            fixture.turn,
            CasItemSource::new(fixture.source.clone(), item_id.clone()),
            SourceEventSequence::new(2).unwrap(),
            beryl_model::SyndicContentId::from_bytes([39; 16]),
        ),
        limit(),
    );
    assert!(matches!(
        preparation,
        Err(ProviderObservationFramePreparationError::FrameValidation(
            ProviderItemValidationError::CompletionOnlyItemStarted
        ))
    ));
    publish_issue(
        &fixture,
        inspect_subagent_start(&fixture, 40, 5, &item_id),
        ProviderObservationIssueReason::CompletionOnlyItemStarted,
        timestamp(5),
    );
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::CompletionOnlyItemStarted)
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}

#[test]
fn completion_without_start_issue_is_exact_and_does_not_create_a_canonical_item() {
    let fixture = setup("phase18-issue-missing-start");
    let item_id = CasItemId::new("phase18-missing-start").unwrap();
    publish_issue(
        &fixture,
        inspect_agent_item(
            &fixture,
            41,
            ProviderObservationItemLifecycle::Completed,
            5,
            &item_id,
            "completion without start",
        ),
        ProviderObservationIssueReason::MissingItemStart,
        timestamp(5),
    );
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::MissingItemStart)
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}

#[test]
fn event_after_completion_issue_is_exact_and_preserves_the_completed_item() {
    let fixture = setup("phase18-issue-after-completion");
    admit_agent_start(&fixture);
    complete_agent_item(&fixture);
    publish_issue(
        &fixture,
        inspect_agent_item(
            &fixture,
            42,
            ProviderObservationItemLifecycle::Started,
            7,
            &fixture.cas_item,
            "late replacement",
        ),
        ProviderObservationIssueReason::EventAfterCompletion,
        timestamp(7),
    );
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::EventAfterCompletion)
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}

#[test]
fn event_after_completion_precedes_item_kind_mismatch() {
    let fixture = setup("phase18-issue-after-completion-kind-mismatch");
    admit_agent_start(&fixture);
    complete_agent_item(&fixture);
    publish_issue(
        &fixture,
        inspect_subagent_start(&fixture, 46, 7, &fixture.cas_item),
        ProviderObservationIssueReason::EventAfterCompletion,
        timestamp(7),
    );
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::EventAfterCompletion)
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}

#[test]
fn later_issue_preserves_the_first_reason_and_both_events_leave_the_item_unchanged() {
    let fixture = setup("phase18-issue-first-reason");
    admit_agent_start(&fixture);
    let first_sequence = publish_issue(
        &fixture,
        inspect_agent_item(
            &fixture,
            43,
            ProviderObservationItemLifecycle::Completed,
            4,
            &fixture.cas_item,
            "completion before start",
        ),
        ProviderObservationIssueReason::CompletionBeforeStart,
        timestamp(6),
    );
    let second_sequence = publish_issue(
        &fixture,
        inspect_subagent_start(&fixture, 44, 7, &fixture.cas_item),
        ProviderObservationIssueReason::ItemKindMismatch,
        timestamp(7),
    );
    assert_eq!(first_sequence.get() + 1, second_sequence.get());
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::CompletionBeforeStart)
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}

#[test]
fn source_less_loss_keeps_its_primary_reason_and_the_first_observation_issue() {
    let fixture = setup("phase18-issue-source-less-loss");
    admit_agent_start(&fixture);
    publish_issue(
        &fixture,
        inspect_agent_start(&fixture, 45),
        ProviderObservationIssueReason::DuplicateItemStart,
        timestamp(6),
    );
    abandon_active_projection(&fixture);

    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let terminal = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        None,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::UnknownTerminal,
                Some(TurnIncompleteReason::StreamLost),
            )
            .unwrap(),
        ),
        timestamp(8),
    )
    .unwrap();
    committed_command(execute(
        &fixture.store,
        fixture
            .storage
            .admit_live_source_event(fixture.storage.revision(&fixture.store).unwrap(), terminal),
    ));

    let terminal_state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        terminal_state.terminal_outcome(),
        Some(TurnTerminalOutcome::UnknownTerminal)
    );
    assert_eq!(
        terminal_state.incomplete_reason(),
        Some(TurnIncompleteReason::StreamLost)
    );
    assert_eq!(
        terminal_state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::DuplicateItemStart)
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
}
