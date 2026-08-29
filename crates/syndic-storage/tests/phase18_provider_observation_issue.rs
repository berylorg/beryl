#![cfg(feature = "test-faults")]

#[path = "phase18_provider_observation_issue/mod.rs"]
mod issue_cases;
#[path = "support/mod.rs"]
mod support;

use beryl_home_store::{
    CommandError, CommandOutcome, HomeCommand, HomeStore, MutationContribution,
};
use beryl_model::{
    CasItemId, ProviderObservationId, SyndicDraftId, SyndicItemId, SyndicThreadId, SyndicTurnId,
};
use syndic_storage::*;

use support::{TestHome, exact_cas, open, timestamp};

struct Fixture {
    store: HomeStore,
    home: TestHome,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: CasTurnSource,
    assistant: SyndicItemId,
    cas_item: CasItemId,
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn typed_error(error: &CommandError) -> &SyndicMutationError {
    let CommandError::ContributorValidation { source, .. } = error else {
        panic!("expected Syndic validation rejection, got {error}");
    };
    source.downcast_ref().expect("Syndic mutation error")
}

fn setup(name: &str) -> Fixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([1; 16]);
    committed_command(execute(
        &store,
        storage.clone().create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([2; 16]),
                exact_cas::execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let turn = exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        thread,
        SyndicDraftId::from_bytes([3; 16]),
        SyndicItemId::from_bytes([4; 16]),
        "question",
        timestamp(2),
    );
    let source = exact_cas::establish_turn(&store, storage.clone(), thread, turn, timestamp(3));
    exact_cas::admit_event(
        &store,
        storage.clone(),
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    Fixture {
        store,
        home,
        storage,
        thread,
        turn,
        source,
        assistant: SyndicItemId::from_bytes([5; 16]),
        cas_item: CasItemId::new("phase18-assistant").unwrap(),
    }
}

fn agent_value(text: &str) -> ProviderItemV1 {
    ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline(text),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    })
}

fn admit_agent_start(fixture: &Fixture) {
    exact_cas::admit_item_frame(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        fixture.assistant,
        &fixture.source,
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            fixture.cas_item.clone(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(5),
                item: agent_value("canonical"),
            },
        ),
        timestamp(5),
    );
}

fn observation_callback(
    store: &HomeStore,
    storage: SyndicStorage,
) -> impl FnMut(&ProviderObservationStageBatch) -> CommandOutcome + '_ {
    move |batch| {
        store.execute_current(
            storage
                .clone()
                .current_stage_provider_observation_batch(batch.clone()),
        )
    }
}

fn committed_stage_value<T>(outcome: ProviderObservationStageOutcome<T>) -> T {
    match outcome {
        ProviderObservationStageOutcome::Committed {
            value,
            receipt,
            later_failure: None,
        } => {
            drop(receipt);
            value
        }
        ProviderObservationStageOutcome::Committed {
            later_failure: Some(failure),
            ..
        } => panic!("expected staging to commit without later failure, got {failure:?}"),
        ProviderObservationStageOutcome::NotCommitted { evidence } => {
            panic!("expected staging to commit, got NotCommitted: {evidence:?}")
        }
        ProviderObservationStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected staging to commit, got Indeterminate: {failure:?}")
        }
    }
}

fn committed_seal_value(
    outcome: ProviderObservationSealOutcome,
) -> SealedProviderObservationHandle {
    match outcome {
        ProviderObservationSealOutcome::Committed {
            value,
            receipt,
            later_failure: None,
        } => {
            drop(receipt);
            value
        }
        ProviderObservationSealOutcome::Committed {
            later_failure: Some(failure),
            ..
        } => panic!("expected sealing to commit without later failure, got {failure:?}"),
        ProviderObservationSealOutcome::NotCommitted { evidence } => {
            panic!("expected sealing to commit, got NotCommitted: {evidence:?}")
        }
        ProviderObservationSealOutcome::Indeterminate { failure, custody } => {
            custody.install();
            panic!("expected sealing to commit, got Indeterminate: {failure:?}")
        }
    }
}

fn committed_command(outcome: CommandOutcome) {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => drop(receipt),
        CommandOutcome::Committed {
            later_failure: Some(failure),
            ..
        } => panic!("expected command to commit without later failure, got {failure:?}"),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("expected command to commit, got NotCommitted: {evidence:?}")
        }
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected command to commit, got Indeterminate: {failure:?}")
        }
    }
}

fn not_committed_command(outcome: CommandOutcome) -> CommandError {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => evidence,
        CommandOutcome::Committed {
            receipt,
            later_failure,
        } => {
            drop(receipt);
            panic!(
                "expected command rejection, got Committed with later failure: {later_failure:?}"
            )
        }
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected command rejection, got Indeterminate: {failure:?}")
        }
    }
}

fn scalar(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderScalar,
    callback: &mut impl ProviderObservationStageCallback,
) {
    committed_stage_value(
        stager
            .control(
                ProviderObservationControl::Scalar {
                    context: ProviderValueContext::Field(field),
                    value,
                },
                callback,
            )
            .unwrap(),
    );
}

fn enum_value(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: ProviderEnumValue,
    callback: &mut impl ProviderObservationStageCallback,
) {
    committed_stage_value(
        stager
            .control(
                ProviderObservationControl::Enum {
                    context: ProviderValueContext::Field(field),
                    value,
                },
                callback,
            )
            .unwrap(),
    );
}

fn text(
    stager: &mut ProviderObservationStager,
    field: ProviderField,
    value: &str,
    callback: &mut impl ProviderObservationStageCallback,
) {
    let context = ProviderValueContext::Field(field);
    committed_stage_value(
        stager
            .control(ProviderObservationControl::BeginField(context), callback)
            .unwrap(),
    );
    committed_stage_value(
        stager
            .fragment(
                ProviderObservationStagingBytes::new(context, value.as_bytes()).unwrap(),
                callback,
            )
            .unwrap(),
    );
    committed_stage_value(
        stager
            .control(ProviderObservationControl::EndField(context), callback)
            .unwrap(),
    );
}

fn inspect_agent_start(fixture: &Fixture, observation_byte: u8) -> InspectedProviderObservation {
    let sealed = {
        let mut callback = observation_callback(&fixture.store, fixture.storage.clone());
        let mut stager = committed_stage_value(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([observation_byte; 16]),
                ProviderObservationBegin::Item {
                    lifecycle: ProviderObservationItemLifecycle::Started,
                    kind: ProviderObservationItemKind::AgentMessage,
                },
                &mut callback,
            )
            .unwrap(),
        );
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(6),
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::ItemId,
            fixture.cas_item.as_str(),
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::AgentMessageText,
            "conflicting replacement",
            &mut callback,
        );
        committed_seal_value(stager.seal(&mut callback).unwrap())
    };
    let route = ProviderObservationRoute::new(
        fixture.source.thread_id().clone(),
        fixture.source.turn_id().clone(),
    );
    let bound = sealed.bind(route.clone(), route).unwrap();
    inspect_provider_observation(&fixture.storage, &fixture.store, bound, limit()).unwrap()
}

fn inspect_completion_only(fixture: &Fixture) -> InspectedProviderObservation {
    let sealed = {
        let mut callback = observation_callback(&fixture.store, fixture.storage.clone());
        let mut stager = committed_stage_value(
            ProviderObservationStager::begin(
                ProviderObservationId::from_bytes([31; 16]),
                ProviderObservationBegin::Item {
                    lifecycle: ProviderObservationItemLifecycle::Completed,
                    kind: ProviderObservationItemKind::SubAgentActivity,
                },
                &mut callback,
            )
            .unwrap(),
        );
        scalar(
            &mut stager,
            ProviderField::LifecycleObservedAt,
            ProviderScalar::Unsigned(5),
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::ItemId,
            "completion-only",
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
            "subagent-thread",
            &mut callback,
        );
        text(
            &mut stager,
            ProviderField::SubAgentPath,
            "root/worker",
            &mut callback,
        );
        committed_seal_value(stager.seal(&mut callback).unwrap())
    };
    let route = ProviderObservationRoute::new(
        fixture.source.thread_id().clone(),
        fixture.source.turn_id().clone(),
    );
    let bound = sealed.bind(route.clone(), route).unwrap();
    inspect_provider_observation(&fixture.storage, &fixture.store, bound, limit()).unwrap()
}

fn next_event(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: &CasTurnSource,
    payload: SourceEventPayload,
    observed_at: SyndicTimestamp,
) -> LiveSourceEvent {
    let state = storage
        .clone()
        .turn_state(store, turn, limit())
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, thread, limit()).unwrap().unwrap();
    LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(source.clone()),
        payload,
        observed_at,
    )
    .unwrap()
}

fn canonical_item(fixture: &Fixture) -> CanonicalItemRecord {
    fixture
        .storage
        .clone()
        .canonical_item(&fixture.store, fixture.assistant, limit())
        .unwrap()
        .unwrap()
        .clone()
}

#[test]
fn duplicate_start_issue_is_exact_durable_and_does_not_replace_the_canonical_item() {
    let fixture = setup("phase18-provider-observation-issue-durable");
    admit_agent_start(&fixture);
    let canonical_before = canonical_item(&fixture);
    let issue = inspect_agent_start(&fixture, 11)
        .into_issue(ProviderObservationIssueReason::DuplicateItemStart);
    let event = next_event(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::ProviderObservationIssue(Box::new(issue.clone())),
        timestamp(6),
    );
    committed_command(execute(
        &fixture.store,
        fixture.storage.clone().admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            event.clone(),
        ),
    ));

    assert_eq!(canonical_item(&fixture), canonical_before);
    let state = fixture
        .storage
        .clone()
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.item_count(), 2);
    assert_eq!(state.source_event_count(), 3);
    assert_eq!(
        state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::DuplicateItemStart)
    );

    let expected_record = SourceEventRecord::new(
        fixture.turn,
        event.sequence(),
        Some(fixture.source.clone()),
        SourceEventPayload::ProviderObservationIssue(Box::new(issue)),
    )
    .unwrap();
    let stored = fixture
        .storage
        .clone()
        .source_event(&fixture.store, fixture.turn, event.sequence(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(stored, expected_record);
    assert_eq!(
        fixture
            .storage
            .clone()
            .live_source_event_status(&fixture.store, &event, limit())
            .unwrap(),
        LiveSourceEventStatus::Exact
    );

    let retry_error = not_committed_command(execute(
        &fixture.store,
        fixture.storage.clone().admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            event.clone(),
        ),
    ));
    assert!(matches!(
        typed_error(&retry_error),
        SyndicMutationError::SourceEventAlreadyAdmitted
    ));
    let collision = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        event.expected_state_revision(),
        event.expected_gate_revision(),
        event.sequence(),
        Some(fixture.source.clone()),
        SourceEventPayload::TurnActivated,
        event.observed_at(),
    )
    .unwrap();
    assert_eq!(
        fixture
            .storage
            .clone()
            .live_source_event_status(&fixture.store, &collision, limit())
            .unwrap(),
        LiveSourceEventStatus::Collision
    );
    let collision_error = not_committed_command(execute(
        &fixture.store,
        fixture
            .storage
            .clone()
            .admit_live_source_event(fixture.storage.revision(&fixture.store).unwrap(), collision),
    ));
    assert!(matches!(
        typed_error(&collision_error),
        SyndicMutationError::SourceEventCollision
    ));

    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
    let mut reopened = open(fixture.home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let reopened_event = storage
        .clone()
        .source_event(&reopened, fixture.turn, event.sequence(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(reopened_event, expected_record);
    assert_eq!(
        storage
            .clone()
            .live_source_event_status(&reopened, &event, limit())
            .unwrap(),
        LiveSourceEventStatus::Exact
    );
    let reopened_item = storage
        .clone()
        .canonical_item(&reopened, fixture.assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(reopened_item, canonical_before);

    let rejected_terminal = next_event(
        &reopened,
        storage.clone(),
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Complete,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(7),
    );
    let terminal_error = not_committed_command(execute(
        &reopened,
        storage
            .clone()
            .admit_live_source_event(storage.revision(&reopened).unwrap(), rejected_terminal),
    ));
    assert!(matches!(
        typed_error(&terminal_error),
        SyndicMutationError::ProviderObservationIssueConflict
    ));

    let accepted_terminal = next_event(
        &reopened,
        storage.clone(),
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Complete,
                Some(TurnIncompleteReason::CompletionMismatch),
            )
            .unwrap(),
        ),
        timestamp(8),
    );
    committed_command(execute(
        &reopened,
        storage
            .clone()
            .admit_live_source_event(storage.revision(&reopened).unwrap(), accepted_terminal),
    ));
    let terminal_state = storage
        .clone()
        .turn_state(&reopened, fixture.turn, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        terminal_state.terminal_outcome(),
        Some(TurnTerminalOutcome::Complete)
    );
    assert_eq!(
        terminal_state.incomplete_reason(),
        Some(TurnIncompleteReason::CompletionMismatch)
    );
    assert_eq!(
        terminal_state.provider_observation_issue(),
        Some(ProviderObservationIssueReason::DuplicateItemStart)
    );
    let terminal_item = storage
        .canonical_item(&reopened, fixture.assistant, limit())
        .unwrap()
        .unwrap();
    assert_eq!(terminal_item, canonical_before);
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}
