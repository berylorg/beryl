use super::*;
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

fn terminal_target(fixture: &Fixture) -> RepairRequiredTarget {
    exact_cas::correlate_user_item(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
        SyndicItemId::from_bytes([4; 16]),
        &fixture.source,
        timestamp(5),
    );
    let status = TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap();
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
            crate::support::fixture_activity(
                &fixture.store,
                &fixture.storage,
                event.thread_id(),
                event.turn_id(),
            ),
        ),
    ));
    exact_cas::converge_items(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.turn,
    );
    exact_cas::converge_transcript(&fixture.store, fixture.storage.clone(), fixture.thread);
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
            None,
        )
        .unwrap(),
        RepairRequestDisposition::Available,
    )
}

fn fork_request(
    fixture: &Fixture,
    source: SyndicThreadId,
    child: SyndicThreadId,
    draft: SyndicDraftId,
) -> CreateThread {
    let tail = fixture
        .storage
        .thread_tail(&fixture.store, source, limit())
        .unwrap()
        .unwrap();
    assert!(
        tail.complete(),
        "terminal history must be complete before forking"
    );
    CreateThread::from_tail(
        child,
        draft,
        tail.last_activity_at(),
        DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
        tail,
    )
    .unwrap()
}

fn create_fork(
    fixture: &Fixture,
    source: SyndicThreadId,
    child: SyndicThreadId,
    draft: SyndicDraftId,
) {
    committed_command(execute(
        &fixture.store,
        fixture.storage.create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            fork_request(fixture, source, child, draft),
        ),
    ));
    exact_cas::converge_transcript(&fixture.store, fixture.storage.clone(), child);
}

fn enter(fixture: &Fixture, target: RepairRequiredTarget) {
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    committed_command(
        fixture
            .store
            .execute_current(fixture.storage.current_require_terminal_repair(
                RequireTerminalRepair::new(fixture.thread, gate.revision(), target),
            )),
    );
}

fn repair_rejection(outcome: CommandOutcome) {
    let error = not_committed_command(outcome);
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::RepairTargetConflict
    ));
}

fn replacement_mutation(fixture: &Fixture, thread: SyndicThreadId) -> FixtureBatch {
    let current = fixture
        .storage
        .current_draft(&fixture.store, thread, limit())
        .unwrap()
        .unwrap();
    let summary = fixture
        .storage
        .history_summary(&fixture.store, thread, limit())
        .unwrap()
        .unwrap();
    let head = fixture
        .storage
        .transcript_view_head(&fixture.store, thread, limit())
        .unwrap()
        .unwrap();
    let page = fixture
        .storage
        .transcript_entries(
            &fixture.store,
            thread,
            head.generation(),
            None,
            beryl_home_store::CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let position = page
        .records()
        .first()
        .expect("forked thread must retain a current transcript entry")
        .position();
    let revision = current.draft().revision().checked_next().unwrap();
    let intent = ReplacementEditIntent::new(
        fixture.turn,
        current.thread().selected_path(),
        CurrentTranscriptEntryProof::new(head.generation(), position),
    );
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::Draft(DraftRecord::new(
            current.draft().id(),
            thread,
            revision,
            DraftSubmissionIntent::Replacement(intent),
            current.draft().root_history(),
            current.draft().created_at(),
            summary.last_activity_at(),
        )))
        .unwrap();
    batch
        .put(FixtureRecord::DraftByThread(DraftByThreadRecord::new(
            thread,
            current.draft().id(),
            revision,
            current.thread().revision(),
        )))
        .unwrap();
    batch
}

fn replacement_request(fixture: &Fixture, thread: SyndicThreadId, seed: u8) -> FirstAcceptance {
    committed_command(execute(
        &fixture.store,
        fixture.storage.fixture_contribution(
            fixture.storage.revision(&fixture.store).unwrap(),
            replacement_mutation(fixture, thread),
        ),
    ));
    super::repair_queue::acceptance_request(fixture, thread, seed)
}

fn accept_replacement(fixture: &Fixture, request: FirstAcceptance) -> CommandOutcome {
    execute(
        &fixture.store,
        fixture
            .storage
            .first_acceptance(fixture.storage.revision(&fixture.store).unwrap(), request),
    )
}

#[test]
fn repair_required_excludes_same_thread_fork_after_a_valid_pre_entry_fork() {
    let fixture = setup("repair-exclusion-fork");
    let target = terminal_target(&fixture);
    let first = SyndicThreadId::from_bytes([60; 16]);
    create_fork(
        &fixture,
        fixture.thread,
        first,
        SyndicDraftId::from_bytes([61; 16]),
    );
    enter(&fixture, target);
    let before = fixture.storage.revision(&fixture.store).unwrap();
    repair_rejection(execute(
        &fixture.store,
        fixture.storage.create_thread(
            before,
            fork_request(
                &fixture,
                fixture.thread,
                SyndicThreadId::from_bytes([62; 16]),
                SyndicDraftId::from_bytes([63; 16]),
            ),
        ),
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
    repair_rejection(execute(
        &fixture.store,
        fixture.storage.create_thread(
            before,
            fork_request(
                &fixture,
                first,
                SyndicThreadId::from_bytes([64; 16]),
                SyndicDraftId::from_bytes([65; 16]),
            ),
        ),
    ));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
}

#[test]
fn repair_required_does_not_block_unrelated_thread_creation() {
    let fixture = setup("repair-exclusion-unrelated");
    let target = terminal_target(&fixture);
    let unrelated = SyndicThreadId::from_bytes([70; 16]);
    committed_command(execute(
        &fixture.store,
        fixture.storage.create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                unrelated,
                SyndicDraftId::from_bytes([71; 16]),
                exact_cas::execution_binding(),
                timestamp(10),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    enter(&fixture, target);
    committed_command(execute(
        &fixture.store,
        fixture.storage.create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([72; 16]),
                SyndicDraftId::from_bytes([73; 16]),
                exact_cas::execution_binding(),
                timestamp(11),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
}

#[test]
fn repair_required_excludes_cross_thread_replacement_of_its_target() {
    let fixture = setup("repair-exclusion-replacement");
    let target = terminal_target(&fixture);
    let valid = SyndicThreadId::from_bytes([80; 16]);
    create_fork(
        &fixture,
        fixture.thread,
        valid,
        SyndicDraftId::from_bytes([81; 16]),
    );
    committed_command(accept_replacement(
        &fixture,
        replacement_request(&fixture, valid, 140),
    ));
    let blocked = SyndicThreadId::from_bytes([82; 16]);
    create_fork(
        &fixture,
        fixture.thread,
        blocked,
        SyndicDraftId::from_bytes([83; 16]),
    );
    enter(&fixture, target);
    let request = replacement_request(&fixture, blocked, 160);
    let before = fixture.storage.revision(&fixture.store).unwrap();
    repair_rejection(accept_replacement(&fixture, request));
    assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
}
