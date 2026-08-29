#[cfg(feature = "test-faults")]
mod support;

#[path = "phase65_stop_storage/support.rs"]
mod stop_support;

#[cfg(feature = "test-faults")]
#[path = "phase65_awaiting_terminal/canonical.rs"]
mod canonical;
#[cfg(feature = "test-faults")]
#[path = "phase65_awaiting_terminal/corruption.rs"]
mod corruption;
#[cfg(feature = "test-faults")]
#[path = "phase65_awaiting_terminal/recovery.rs"]
mod recovery;
#[cfg(feature = "test-faults")]
#[path = "phase65_awaiting_terminal/resolution.rs"]
mod resolution;

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandError, CommandOutcome, CursorReadLimits, HomeCommand, HomeHealthState, HomeStore,
};
use beryl_model::{
    AcceptedInputRevision, CasItemId, SyndicAcceptedInputId, SyndicDraftId, SyndicItemId,
    SyndicThreadId, SyndicTurnId,
};
use syndic_storage::*;

use stop_support::{
    TestHome, active_stop_fixture, admit_event, converge_and_release_terminal_history, open,
    timestamp,
};
#[cfg(feature = "test-faults")]
use support::{commit, draft_id, exact_cas};

struct ActiveFixture {
    _home: TestHome,
    store: HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: CasTurnSource,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn cursor_limits() -> CursorReadLimits {
    CursorReadLimits::new(64, 2_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn assert_clean(outcome: CommandOutcome) {
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean awaiting-terminal command, got {outcome:?}"),
    }
}

fn active_fixture(name: &str) -> ActiveFixture {
    let fixture = active_stop_fixture(name);
    ActiveFixture {
        _home: fixture._home,
        store: fixture.store,
        storage: fixture.storage,
        thread: fixture.thread,
        turn: fixture.turn,
        source: fixture.source,
    }
}

#[cfg(feature = "test-faults")]
fn active_fixture_with_faults(name: &str, faults: FaultController) -> ActiveFixture {
    let fixture = stop_support::active_stop_fixture_with_faults(name, faults);
    ActiveFixture {
        _home: fixture._home,
        store: fixture.store,
        storage: fixture.storage,
        thread: fixture.thread,
        turn: fixture.turn,
        source: fixture.source,
    }
}

#[cfg(feature = "test-faults")]
fn accept_text(
    fixture: &ActiveFixture,
    text: &str,
    next_draft: SyndicDraftId,
    at: u64,
) -> SyndicAcceptedInputId {
    stop_support::admit_queued_text(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        text,
        next_draft,
        at,
    )
    .id()
}

fn admit_unknown(fixture: &ActiveFixture, at: u64) {
    assert_clean(execute(
        &fixture.store,
        fixture.storage.admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            unknown_event(fixture, at),
        ),
    ));
}

fn unknown_event(fixture: &ActiveFixture, at: u64) -> LiveSourceEvent {
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(fixture.source.clone()),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::UnknownTerminal,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(at),
    )
    .unwrap()
}

fn steering_target(fixture: &ActiveFixture) -> SteeringTargetProof {
    let binding = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding must be active");
    };
    SteeringTargetProof::new(
        PendingSteeringTargetProof::new(
            binding.binding().revision(),
            active.snapshot_id(),
            fixture.turn,
            active.usable().cas_thread_id().clone(),
        ),
        fixture.source.turn_id().clone(),
    )
}

fn route_page(fixture: &ActiveFixture, proof: AcceptedRouteHeadProof) -> AcceptedRoutePage {
    fixture
        .storage
        .accepted_route_page(
            &fixture.store,
            fixture.thread,
            proof.generation(),
            proof.revision(),
            None,
        )
        .unwrap()
}

fn next_sources(fixture: &ActiveFixture) -> Vec<AcceptedNextSource> {
    fixture
        .storage
        .accepted_next_source_page(
            &fixture.store,
            fixture.storage.revision(&fixture.store).unwrap(),
            None,
            cursor_limits(),
        )
        .unwrap()
        .records()
        .to_vec()
}

fn with_typed_error(outcome: CommandOutcome, assertion: impl FnOnce(&SyndicMutationError)) {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => {
            let CommandError::ContributorValidation { source, .. } = evidence else {
                panic!("expected Syndic validation rejection, got {evidence}");
            };
            assertion(source.downcast_ref().expect("Syndic mutation error"));
        }
        CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected definitive Syndic validation rejection, got indeterminate {failure}");
        }
        outcome => panic!("expected not-committed Syndic validation outcome, got {outcome:?}"),
    }
}

fn started_agent_frame(cas_item: CasItemId, at: u64) -> ProviderItemFrameV1 {
    ProviderItemFrameV1::new(
        ProviderFrameOrdinalV1::FIRST,
        cas_item,
        ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(at),
            item: ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
                text: ProviderTextV1::inline("late evidence"),
                phase: None,
                memory_citation: None,
            }),
        },
    )
}

#[test]
fn unknown_terminal_without_queued_input_reactivates_the_exact_target() {
    let fixture = active_fixture("phase65-awaiting-terminal-default-reactivation");
    let original = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .selected_route()
        .unwrap();

    admit_unknown(&fixture, 6);
    let awaiting = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        awaiting.state(),
        &InputGateState::AwaitingTerminal(fixture.turn)
    );
    assert_eq!(awaiting.live_count(), 0);
    assert_eq!(
        awaiting.selected_route().unwrap().generation(),
        original.generation()
    );

    admit_event(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnActivated,
        timestamp(7),
    );
    let reactivated = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        reactivated.state(),
        &InputGateState::Steerable(fixture.turn)
    );
    assert!(
        reactivated.selected_route().unwrap().generation()
            > awaiting.selected_route().unwrap().generation()
    );
    assert_eq!(reactivated.live_count(), 0);
}

#[test]
fn late_terminal_without_queued_input_enters_and_releases_terminal_history() {
    let fixture = active_fixture("phase65-awaiting-terminal-default-resolution");
    admit_unknown(&fixture, 6);
    admit_event(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Incomplete,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(7),
    );
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        &InputGateState::FinalizingHistory(fixture.turn)
    );
    assert_eq!(gate.live_count(), 0);
    converge_and_release_terminal_history(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
    );
    assert_eq!(
        fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::Idle
    );
}

#[test]
fn restart_classifies_empty_awaiting_terminal_as_active_loss_authority() {
    let fixture = active_fixture("phase65-awaiting-terminal-default-recovery");
    admit_unknown(&fixture, 6);
    let ActiveFixture {
        _home: home,
        store,
        thread,
        turn,
        ..
    } = fixture;
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let page = storage
        .delivery_recovery_startup_page(&reopened, None, cursor_limits())
        .unwrap();
    assert_eq!(page.records().len(), 1);
    let DeliveryRecoveryCase::Active(active) = storage
        .classify_delivery_recovery(&reopened, &page.records()[0], point_limit())
        .unwrap()
    else {
        panic!("awaiting-terminal recovery must retain active loss authority")
    };
    assert_eq!(active.thread_id(), thread);
    assert_eq!(active.turn_id(), turn);
    let abandonment = active
        .generic_abandonment(
            "awaiting-terminal foreground generation was lost",
            active.minimum_timestamp(),
        )
        .unwrap();
    assert!(matches!(
        abandonment.target(),
        AcceptedRouteLostTarget::AwaitingTerminal(_)
    ));
    assert_clean(reopened.execute_current(storage.current_abandon_active_binding(abandonment)));
    assert_eq!(
        storage
            .input_gate(&reopened, thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::PendingTurn(turn)
    );
}

#[test]
#[cfg(feature = "test-faults")]
fn uncertain_terminal_reclassifies_ready_work_and_reactivation_uses_a_fresh_route() {
    let fixture = active_fixture("phase65-awaiting-terminal-reactivation");
    let first = accept_text(&fixture, "before uncertainty", draft_id(43), 6);
    let target = steering_target(&fixture);
    let steering_gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let original = steering_gate.selected_route().unwrap();
    assert_eq!(steering_gate.live_steering_count(), 1);

    admit_unknown(&fixture, 8);
    let awaiting = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        awaiting.state(),
        &InputGateState::AwaitingTerminal(fixture.turn)
    );
    assert_eq!(awaiting.live_steering_count(), 0);
    assert_eq!(awaiting.live_next_turn_count(), 1);
    assert_eq!(
        awaiting.selected_route().unwrap().generation(),
        original.generation()
    );
    assert!(awaiting.selected_route().unwrap().revision() > original.revision());
    let original_waiting = awaiting.selected_route().unwrap();
    let page = route_page(&fixture, original_waiting);
    assert_eq!(page.records().len(), 1);
    assert_eq!(page.records()[0].input().id(), first);
    assert_eq!(
        page.records()[0].leaf().state(),
        AcceptedRouteLeafState::Routed
    );
    assert_eq!(
        page.records()[0].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::UnknownTerminal)
    );

    let outcome = execute(
        &fixture.store,
        fixture.storage.begin_accepted_input_delivery(
            fixture.storage.revision(&fixture.store).unwrap(),
            BeginAcceptedInputDelivery::new(
                fixture.thread,
                first,
                AcceptedInputRevision::new(1).unwrap(),
                target,
            ),
        ),
    );
    with_typed_error(outcome, |error| {
        assert!(
            matches!(
                error,
                SyndicMutationError::InputGateStateConflict
                    | SyndicMutationError::ActiveSteeringRouteConflict
                    | SyndicMutationError::AcceptedInputDeliveryConflict
            ),
            "unexpected refusal: {error:?}",
        );
    });

    exact_cas::admit_item_frame(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        SyndicItemId::from_bytes([60; 16]),
        &fixture.source,
        started_agent_frame(CasItemId::new("late-agent").unwrap(), 9),
        timestamp(9),
    );
    let after_item = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(after_item, awaiting);

    let second = accept_text(&fixture, "during uncertainty", draft_id(44), 10);
    let second_record = fixture
        .storage
        .accepted_input(&fixture.store, second, point_limit())
        .unwrap()
        .unwrap();
    assert!(second_record.route_generation() > original_waiting.generation());
    let queued_gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(queued_gate.selected_route(), Some(original_waiting));
    assert_eq!(queued_gate.live_steering_count(), 0);
    assert_eq!(queued_gate.live_next_turn_count(), 2);
    let sources = next_sources(&fixture);
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].generation(), original_waiting.generation());
    assert_eq!(sources[1].generation(), second_record.route_generation());

    admit_event(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnActivated,
        timestamp(12),
    );
    let reactivated = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        reactivated.state(),
        &InputGateState::Steerable(fixture.turn)
    );
    let fresh = reactivated.selected_route().unwrap();
    assert!(fresh.generation() > second_record.route_generation());
    assert_eq!(fresh.revision(), AcceptedRouteRevision::FIRST);
    assert_eq!(
        reactivated.route_generation_high_water(),
        Some(fresh.generation())
    );
    assert_eq!(reactivated.live_steering_count(), 0);
    assert_eq!(reactivated.live_next_turn_count(), 2);
    assert!(route_page(&fixture, fresh).records().is_empty());
    assert_eq!(
        route_page(&fixture, original_waiting).records()[0].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::UnknownTerminal)
    );

    let third = accept_text(&fixture, "after reactivation", draft_id(45), 13);
    let third_record = fixture
        .storage
        .accepted_input(&fixture.store, third, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(third_record.route_generation(), fresh.generation());
    let final_gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(final_gate.state(), &InputGateState::Steerable(fixture.turn));
    assert_eq!(final_gate.live_steering_count(), 1);
    assert_eq!(final_gate.live_next_turn_count(), 2);
    let fresh_page = route_page(&fixture, final_gate.selected_route().unwrap());
    assert_eq!(fresh_page.records().len(), 1);
    assert_eq!(fresh_page.records()[0].input().id(), third);
    assert_eq!(
        fresh_page.records()[0].effective_state(),
        AcceptedRouteEffectiveState::Ready
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
