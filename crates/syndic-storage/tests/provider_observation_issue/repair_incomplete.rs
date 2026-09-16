use super::*;
use beryl_home_store::{
    HomeHealthState, HomeOpenOptions, HomeSchemaVersion, ReconciliationResolution,
    WholeHomeScrubTrigger,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

fn gate(fixture: &Fixture) -> InputGateRecord {
    fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap()
}

fn state(fixture: &Fixture) -> TurnStateRecord {
    fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap()
}

fn enter(fixture: &Fixture, target: RepairRequiredTarget) {
    let observed = gate(fixture);
    committed_command(
        fixture
            .store
            .execute_current(fixture.storage.current_require_terminal_repair(
                RequireTerminalRepair::new(fixture.thread, observed.revision(), target),
            )),
    );
}

fn setup_with_faults(name: &str, faults: FaultController) -> Fixture {
    let home = TestHome::new(name);
    let mut store = beryl_home_store::HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let store = store
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = SyndicThreadId::from_bytes([1; 16]);
    committed_command(execute(
        &store,
        storage.create_thread(
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
        cas_item: CasItemId::new("assistant").unwrap(),
    }
}

fn consume(fixture: &Fixture, target: RepairRequiredTarget) -> RepairRequiredTarget {
    let current = gate(fixture);
    let next = current.revision().checked_next().unwrap();
    let target = RepairRequiredTarget::new(
        target.turn_id(),
        target.source().clone(),
        target.gap(),
        RepairRequestDisposition::Consumed(
            ConsumedRepairRequest::new(
                RepairRequestAttemptNonce::from_bytes([77; 16]),
                current.revision(),
                next,
            )
            .unwrap(),
        ),
    );
    let gate = InputGateRecord::new(
        current.thread_id(),
        next,
        InputGateState::RepairRequired(target.clone()),
        current.accepted_high_water(),
        current.route_generation_high_water(),
        current.selected_route(),
        current.live_steering_count(),
        current.live_next_turn_count(),
        current.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::InputGate(gate)).unwrap();
    committed_command(execute(
        &fixture.store,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.store).unwrap(), batch),
    ));
    target
}

fn request(
    fixture: &Fixture,
    target: RepairRequiredTarget,
    reason: TurnIncompleteReason,
) -> ConvergeRepairIncomplete {
    ConvergeRepairIncomplete::new(
        fixture.thread,
        gate(fixture).revision(),
        state(fixture).revision(),
        target,
        reason,
        timestamp(20),
    )
}

fn converge(fixture: &Fixture, request: ConvergeRepairIncomplete) -> CommandOutcome {
    fixture
        .store
        .execute_current(fixture.storage.current_converge_repair_incomplete(request))
}

fn assert_incomplete_transition(
    fixture: &Fixture,
    target: &RepairRequiredTarget,
    before_gate: &InputGateRecord,
    before_state: &TurnStateRecord,
    original_status: TurnEndStatus,
    reason: TurnIncompleteReason,
) {
    let after_gate = gate(fixture);
    let after_state = state(fixture);
    assert_eq!(
        after_gate.revision(),
        before_gate.revision().checked_next().unwrap()
    );
    assert_eq!(
        after_gate.state(),
        &InputGateState::FinalizingHistory(fixture.turn)
    );
    assert_eq!(
        after_gate.accepted_high_water(),
        before_gate.accepted_high_water()
    );
    assert_eq!(after_gate.selected_route(), before_gate.selected_route());
    assert_eq!(
        after_gate.live_next_turn_count(),
        before_gate.live_next_turn_count()
    );
    assert_eq!(
        after_gate.live_logical_utf8_bytes(),
        before_gate.live_logical_utf8_bytes()
    );
    assert_eq!(
        after_state.revision(),
        before_state.revision().checked_next().unwrap()
    );
    assert_eq!(
        after_state.terminal_outcome(),
        Some(original_status.outcome())
    );
    assert_eq!(after_state.incomplete_reason(), Some(reason));
    assert_eq!(
        after_state.source_event_count(),
        before_state.source_event_count()
    );
    assert_eq!(
        after_state.resolved_repair(),
        Some(&ResolvedRepair::new(
            target.clone(),
            RepairResolution::Incomplete(reason),
        ))
    );
}

#[test]
fn incomplete_convergence_preserves_available_consumed_and_issue_provenance() {
    for (consumed, with_issue, reason) in [
        (false, false, TurnIncompleteReason::CompletionMismatch),
        (true, false, TurnIncompleteReason::CompletionMismatch),
        (false, true, TurnIncompleteReason::ItemAuditFailed),
    ] {
        let mut fixture = setup("repair-incomplete-provenance");
        let original = super::repair_retained::terminal_target(&fixture, with_issue);
        enter(&fixture, original.clone());
        let target = if consumed {
            consume(&fixture, original)
        } else {
            original
        };
        let before_gate = gate(&fixture);
        let before_state = state(&fixture);
        let original_status = before_state.end_status().unwrap();
        let original_terminal = fixture
            .storage
            .source_event(
                &fixture.store,
                fixture.turn,
                target.gap().terminal().sequence(),
                limit(),
            )
            .unwrap()
            .unwrap();
        let request = request(&fixture, target.clone(), reason);
        committed_command(converge(&fixture, request.clone()));
        assert_incomplete_transition(
            &fixture,
            &target,
            &before_gate,
            &before_state,
            original_status,
            reason,
        );
        assert_eq!(
            fixture
                .storage
                .source_event(
                    &fixture.store,
                    fixture.turn,
                    target.gap().terminal().sequence(),
                    limit(),
                )
                .unwrap()
                .unwrap(),
            original_terminal
        );
        let replay = not_committed_command(converge(&fixture, request));
        assert!(matches!(
            typed_error(&replay),
            SyndicMutationError::InputGateRevisionConflict { .. }
        ));
        fixture.store.close().unwrap();
        let mut reopened = open(fixture.home.path());
        fixture.storage = SyndicStorage::register_with_schema_validation(&mut reopened).unwrap();
        fixture.store = reopened
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        assert_eq!(
            state(&fixture).resolved_repair(),
            Some(&ResolvedRepair::new(
                target,
                RepairResolution::Incomplete(reason),
            ))
        );
        fixture
            .store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        fixture.store.close().unwrap();
    }
}

#[test]
fn incomplete_convergence_rejects_substituted_target_and_stale_revisions() {
    let fixture = setup("repair-incomplete-rejections");
    let target = super::repair_retained::terminal_target(&fixture, false);
    enter(&fixture, target.clone());
    let before = fixture.storage.revision(&fixture.store).unwrap();
    let substituted = RepairRequiredTarget::new(
        target.turn_id(),
        target.source().clone(),
        target.gap(),
        RepairRequestDisposition::Consumed(
            ConsumedRepairRequest::new(
                RepairRequestAttemptNonce::from_bytes([78; 16]),
                gate(&fixture).revision(),
                gate(&fixture).revision().checked_next().unwrap(),
            )
            .unwrap(),
        ),
    );
    let rejected = not_committed_command(converge(
        &fixture,
        request(
            &fixture,
            substituted,
            TurnIncompleteReason::CompletionMismatch,
        ),
    ));
    assert!(matches!(
        typed_error(&rejected),
        SyndicMutationError::RepairTargetConflict
    ));
    let current_gate = gate(&fixture);
    let stale_gate = ConvergeRepairIncomplete::new(
        fixture.thread,
        current_gate.revision().checked_next().unwrap(),
        state(&fixture).revision(),
        target.clone(),
        TurnIncompleteReason::CompletionMismatch,
        timestamp(20),
    );
    let rejected = not_committed_command(converge(&fixture, stale_gate));
    assert!(matches!(
        typed_error(&rejected),
        SyndicMutationError::InputGateRevisionConflict { .. }
    ));
    let stale_state = ConvergeRepairIncomplete::new(
        fixture.thread,
        current_gate.revision(),
        state(&fixture).revision().checked_next().unwrap(),
        target,
        TurnIncompleteReason::CompletionMismatch,
        timestamp(20),
    );
    let rejected = not_committed_command(converge(&fixture, stale_state));
    assert!(matches!(
        typed_error(&rejected),
        SyndicMutationError::TurnStateRevisionConflict { .. }
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
}

#[test]
fn incomplete_convergence_rejects_a_codec_valid_summary_substitution() {
    let fixture = setup("repair-incomplete-summary");
    let target = super::repair_retained::terminal_target(&fixture, false);
    enter(&fixture, target.clone());
    let summary = fixture
        .storage
        .history_summary(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let substituted = HistorySummaryRecord::new(
        summary.thread_id(),
        summary.revision(),
        summary.thread_revision().checked_next().unwrap(),
        summary.committed_tail(),
        summary.selected_path_digest(),
        summary.complete(),
        summary.last_activity_at(),
    );
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::HistorySummary(substituted))
        .unwrap();
    committed_command(execute(
        &fixture.store,
        fixture
            .storage
            .fixture_contribution(fixture.storage.revision(&fixture.store).unwrap(), batch),
    ));
    let before = fixture.storage.revision(&fixture.store).unwrap();
    let rejected = not_committed_command(converge(
        &fixture,
        request(&fixture, target, TurnIncompleteReason::CompletionMismatch),
    ));
    assert!(matches!(
        typed_error(&rejected),
        SyndicMutationError::RepairTargetConflict
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
}

#[test]
fn incomplete_convergence_preserves_queued_input_accounting() {
    let fixture = setup("repair-incomplete-queued-input");
    let target = super::repair_retained::terminal_target(&fixture, false);
    enter(&fixture, target.clone());
    let acceptance = super::repair_queue::acceptance_request(&fixture, fixture.thread, 180);
    committed_command(execute(
        &fixture.store,
        fixture.storage.first_acceptance(
            fixture.storage.revision(&fixture.store).unwrap(),
            acceptance,
        ),
    ));
    let before = gate(&fixture);
    assert_eq!(before.live_next_turn_count(), 1);
    committed_command(converge(
        &fixture,
        request(&fixture, target, TurnIncompleteReason::CompletionMismatch),
    ));
    let after = gate(&fixture);
    assert_eq!(after.live_next_turn_count(), before.live_next_turn_count());
    assert_eq!(
        after.live_logical_utf8_bytes(),
        before.live_logical_utf8_bytes()
    );
    assert_eq!(after.accepted_high_water(), before.accepted_high_water());
}

#[test]
fn incomplete_convergence_acknowledgement_loss_reconciles_one_exact_state() {
    let faults = FaultController::new();
    let fixture = setup_with_faults("repair-incomplete-ack-loss", faults.clone());
    let target = super::repair_retained::terminal_target(&fixture, false);
    enter(&fixture, target.clone());
    let before_gate = gate(&fixture);
    let before_state = state(&fixture);
    let original_status = before_state.end_status().unwrap();
    let request = request(
        &fixture,
        target.clone(),
        TurnIncompleteReason::CompletionMismatch,
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate {
        failure: CommandError::Persistence { .. },
        reconciliation,
    } = converge(&fixture, request)
    else {
        panic!("expected indeterminate acknowledgement loss")
    };
    let handle = reconciliation.install_and_handle();
    assert_eq!(fixture.store.health().state(), HomeHealthState::Healthy);
    assert!(matches!(
        fixture.store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_incomplete_transition(
        &fixture,
        &target,
        &before_gate,
        &before_state,
        original_status,
        TurnIncompleteReason::CompletionMismatch,
    );
    assert!(fixture.store.pending_reconciliations().is_empty());
    fixture
        .store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
    let mut reopened = open(fixture.home.path());
    let storage = SyndicStorage::register_with_schema_validation(&mut reopened).unwrap();
    let reopened = reopened
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(
        storage
            .turn_state(&reopened, fixture.turn, limit())
            .unwrap()
            .unwrap()
            .resolved_repair(),
        Some(&ResolvedRepair::new(
            target,
            RepairResolution::Incomplete(TurnIncompleteReason::CompletionMismatch),
        ))
    );
    reopened.close().unwrap();
}

#[test]
fn incomplete_convergence_finalizes_and_releases_before_a_later_successor() {
    let fixture = setup("repair-incomplete-release");
    let target = super::repair_retained::terminal_target(&fixture, false);
    enter(&fixture, target.clone());
    committed_command(converge(
        &fixture,
        request(
            &fixture,
            target.clone(),
            TurnIncompleteReason::CompletionMismatch,
        ),
    ));
    exact_cas::converge_and_release_terminal_history(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
    );
    assert_eq!(gate(&fixture).state(), &InputGateState::Idle);
    let successor = exact_cas::submit_current_draft(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        SyndicDraftId::from_bytes([89; 16]),
        SyndicItemId::from_bytes([90; 16]),
        "successor",
        timestamp(30),
    );
    assert_ne!(successor, fixture.turn);
    assert_eq!(
        fixture
            .storage
            .thread(&fixture.store, fixture.thread, limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(successor)
    );
    assert_eq!(
        state(&fixture).resolved_repair(),
        Some(&ResolvedRepair::new(
            target.clone(),
            RepairResolution::Incomplete(TurnIncompleteReason::CompletionMismatch),
        ))
    );
    fixture
        .store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    fixture.store.close().unwrap();
    let mut reopened = open(fixture.home.path());
    let storage = SyndicStorage::register_with_schema_validation(&mut reopened).unwrap();
    let reopened = reopened
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(
        storage
            .thread(&reopened, fixture.thread, limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(successor)
    );
    assert_eq!(
        storage
            .turn_state(&reopened, fixture.turn, limit())
            .unwrap()
            .unwrap()
            .resolved_repair(),
        Some(&ResolvedRepair::new(
            target,
            RepairResolution::Incomplete(TurnIncompleteReason::CompletionMismatch),
        ))
    );
    reopened.close().unwrap();
}
