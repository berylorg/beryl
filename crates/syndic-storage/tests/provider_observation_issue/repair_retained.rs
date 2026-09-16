use super::*;
use syndic_storage::test_faults::{ProviderObservationCorruption, RepairTargetFactForTest};

fn terminal_target(fixture: &Fixture, with_issue: bool) -> RepairRequiredTarget {
    let issue = if with_issue {
        admit_agent_start(fixture);
        let (_, event) = super::corruption::publish_duplicate_start_issue(
            fixture,
            inspect_agent_start(fixture, 81),
        );
        Some(
            fixture
                .storage
                .source_event(&fixture.store, fixture.turn, event.sequence(), limit())
                .unwrap()
                .unwrap()
                .repair_witness(),
        )
    } else {
        None
    };
    let status = TurnEndStatus::new(
        TurnTerminalOutcome::Interrupted,
        Some(if with_issue {
            TurnIncompleteReason::CompletionMismatch
        } else {
            TurnIncompleteReason::StreamLost
        }),
    )
    .unwrap();
    let event = next_event(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(status),
        timestamp(9),
    );
    committed_command(execute(
        &fixture.store,
        fixture.storage.admit_live_source_event(
            fixture.storage.revision(&fixture.store).unwrap(),
            event.clone(),
        ),
    ));
    let terminal = fixture
        .storage
        .source_event(&fixture.store, fixture.turn, event.sequence(), limit())
        .unwrap()
        .unwrap();
    RepairRequiredTarget::new(
        fixture.turn,
        fixture.source.clone(),
        RepairCaptureGap::new(
            terminal.repair_witness(),
            status,
            RepairCaptureGapReason::ForcedAbortOrderingUnproven,
            issue,
        )
        .unwrap(),
        RepairRequestDisposition::Available,
    )
}

fn probe(
    fixture: &Fixture,
    thread: SyndicThreadId,
    target: &RepairRequiredTarget,
) -> CommandOutcome {
    fixture.store.execute_current(
        fixture
            .storage
            .current_probe_repair_target_for_test(thread, target.clone()),
    )
}

fn rejected(fixture: &Fixture, thread: SyndicThreadId, target: &RepairRequiredTarget) {
    let before = fixture.storage.revision(&fixture.store).unwrap();
    let error = not_committed_command(probe(fixture, thread, target));
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::InputGateStateConflict
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
}

#[test]
fn retained_terminal_and_optional_issue_survive_reopen() {
    for with_issue in [false, true] {
        let mut fixture = setup("repair-retained-reopen");
        let target = terminal_target(&fixture, with_issue);
        committed_command(probe(&fixture, fixture.thread, &target));
        fixture.store.close().unwrap();
        let mut reopened = open(fixture.home.path());
        fixture.storage = SyndicStorage::register(&mut reopened).unwrap();
        fixture.store = reopened
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        committed_command(probe(&fixture, fixture.thread, &target));
    }
}

#[test]
fn each_missing_retained_fact_rejects_without_publication() {
    for fact in [
        RepairTargetFactForTest::Thread,
        RepairTargetFactForTest::Turn,
        RepairTargetFactForTest::State,
        RepairTargetFactForTest::CasThread,
        RepairTargetFactForTest::CasTurn,
        RepairTargetFactForTest::Terminal,
        RepairTargetFactForTest::Issue,
        RepairTargetFactForTest::Observation,
    ] {
        let fixture = setup("repair-retained-missing");
        let target = terminal_target(&fixture, true);
        committed_command(fixture.store.execute_current(
            fixture.storage.current_remove_repair_fact_for_test(
                fixture.thread,
                target.clone(),
                fact,
            ),
        ));
        rejected(&fixture, fixture.thread, &target);
    }
}

#[test]
fn substituted_target_status_frontier_digest_and_owner_reject() {
    let fixture = setup("repair-retained-substitution");
    let target = terminal_target(&fixture, false);
    rejected(&fixture, SyndicThreadId::from_bytes([91; 16]), &target);
    for (terminal, status) in [
        (
            RepairSourceEventWitness::new(
                SourceEventSequence::FIRST,
                target.gap().terminal().digest(),
            ),
            target.gap().status(),
        ),
        (
            RepairSourceEventWitness::new(
                target.gap().terminal().sequence(),
                RepairSourceEventDigest::from_bytes([0; 32]),
            ),
            target.gap().status(),
        ),
        (
            target.gap().terminal(),
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::WorkerStopped),
            )
            .unwrap(),
        ),
    ] {
        let substituted = RepairRequiredTarget::new(
            fixture.turn,
            fixture.source.clone(),
            RepairCaptureGap::new(
                terminal,
                status,
                RepairCaptureGapReason::ForcedAbortOrderingUnproven,
                None,
            )
            .unwrap(),
            RepairRequestDisposition::Available,
        );
        rejected(&fixture, fixture.thread, &substituted);
    }
    committed_command(probe(&fixture, fixture.thread, &target));
}

#[test]
fn changed_sealed_build_is_not_authenticated_by_the_old_issue_witness() {
    let fixture = setup("repair-retained-build");
    let target = terminal_target(&fixture, true);
    let event = fixture
        .storage
        .source_event(
            &fixture.store,
            fixture.turn,
            target.gap().issue().unwrap().sequence(),
            limit(),
        )
        .unwrap()
        .unwrap();
    let SourceEventPayload::ProviderObservationIssue(issue) = event.payload() else {
        unreachable!()
    };
    let build = fixture
        .storage
        .provider_observation_build(&fixture.store, issue.observation().identity(), limit())
        .unwrap()
        .unwrap();
    committed_command(
        fixture.store.execute_current(
            fixture
                .storage
                .current_corrupt_provider_observation(
                    &build,
                    ProviderObservationCorruption::BuildDigest,
                )
                .unwrap(),
        ),
    );
    rejected(&fixture, fixture.thread, &target);
}

#[test]
fn unrelated_missing_thread_does_not_affect_exact_target() {
    let fixture = setup("repair-retained-unrelated");
    let target = terminal_target(&fixture, false);
    let unrelated = SyndicThreadId::from_bytes([92; 16]);
    committed_command(execute(
        &fixture.store,
        fixture.storage.clone().create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                unrelated,
                SyndicDraftId::from_bytes([93; 16]),
                exact_cas::execution_binding(),
                timestamp(10),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    committed_command(fixture.store.execute_current(
        fixture.storage.current_remove_repair_fact_for_test(
            unrelated,
            target.clone(),
            RepairTargetFactForTest::Thread,
        ),
    ));
    committed_command(probe(&fixture, fixture.thread, &target));
}

#[test]
fn substituted_turn_kind_owner_and_reverse_index_owners_reject() {
    use syndic_storage::test_faults::{
        RepairTargetReplacementForTest as Replacement,
        inject_repair_target_replacement_for_test as inject,
    };
    let fixture = setup("repair-retained-ownership");
    let target = terminal_target(&fixture, false);
    let turn = fixture
        .storage
        .turn(&fixture.store, fixture.turn, limit())
        .unwrap()
        .unwrap();
    for (id, owner, kind) in [
        (
            SyndicTurnId::from_bytes([94; 16]),
            fixture.thread,
            turn.kind(),
        ),
        (
            fixture.turn,
            SyndicThreadId::from_bytes([94; 16]),
            turn.kind(),
        ),
        (
            fixture.turn,
            fixture.thread,
            TurnKind::ProviderOperation(ProviderOperationKind::ContextCompaction),
        ),
    ] {
        inject(
            &fixture.store,
            &fixture.storage,
            fixture.thread,
            &target,
            Replacement::Turn(TurnRecord::new(
                id,
                owner,
                kind,
                turn.parent(),
                turn.ancestor_skip(),
                turn.depth(),
                turn.chain_digest(),
                turn.submitted_at(),
            )),
        )
        .unwrap();
        rejected(&fixture, fixture.thread, &target);
    }
    inject(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        &target,
        Replacement::Turn(turn.clone()),
    )
    .unwrap();
    let thread_index = fixture
        .storage
        .cas_thread_owner(&fixture.store, fixture.source.thread_id().clone(), limit())
        .unwrap()
        .unwrap();
    inject(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        &target,
        Replacement::CasThread(CasThreadIndexRecord::new(
            fixture.source.thread_id().clone(),
            SyndicThreadId::from_bytes([94; 16]),
            thread_index.first_binding_revision(),
        )),
    )
    .unwrap();
    rejected(&fixture, fixture.thread, &target);
    inject(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        &target,
        Replacement::CasThread(thread_index),
    )
    .unwrap();
    let index = fixture
        .storage
        .cas_turn_owner(
            &fixture.store,
            fixture.source.thread_id().clone(),
            fixture.source.turn_id().clone(),
            limit(),
        )
        .unwrap()
        .unwrap();
    for (thread, turn) in [
        (SyndicThreadId::from_bytes([94; 16]), fixture.turn),
        (fixture.thread, SyndicTurnId::from_bytes([94; 16])),
    ] {
        inject(
            &fixture.store,
            &fixture.storage,
            fixture.thread,
            &target,
            Replacement::CasTurn(CasTurnIndexRecord::new(
                index.cas_thread_id().clone(),
                index.cas_turn_id().clone(),
                thread,
                turn,
                index.binding_revision(),
                index.snapshot_id(),
                index.post_turn_native_count(),
            )),
        )
        .unwrap();
        rejected(&fixture, fixture.thread, &target);
    }
    inject(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        &target,
        Replacement::CasTurn(index),
    )
    .unwrap();
    committed_command(probe(&fixture, fixture.thread, &target));
}

#[test]
fn a_different_current_tail_rejects_an_otherwise_exact_terminal() {
    use syndic_storage::test_faults::{
        RepairTargetReplacementForTest as Replacement,
        inject_repair_target_replacement_for_test as inject,
    };
    let fixture = setup("repair-retained-tail");
    let thread = fixture
        .storage
        .thread(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let target = terminal_target(&fixture, false);
    let empty = SelectedPathProof::new(None, thread.revision(), empty_selected_path_digest());
    inject(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        &target,
        Replacement::Thread(ThreadRecord::new(
            thread.id(),
            empty,
            thread.current_draft_id(),
            thread.lineage(),
            thread.context_owner_id(),
        )),
    )
    .unwrap();
    rejected(&fixture, fixture.thread, &target);
}
