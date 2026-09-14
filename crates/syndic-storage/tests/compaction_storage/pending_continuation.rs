use super::compaction_support::{CompactionFixture, point_limit, timestamp};
use crate::support::exact_cas;
use beryl_home_store::{CommandOutcome, WholeHomeScrubTrigger};
use syndic_storage::{
    CancelBindingActivation, CompactionRequestDisposition, CompactionRequestTransitionStatus,
    PublishCompactionRequestDisposition, SettleLifecycleCompaction, TurnDispatchProvenance,
    TurnLifecycle, TurnStateRecord, TurnStateRevision,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

fn fixture(
    seed: u8,
) -> (
    CompactionFixture,
    beryl_model::SyndicTurnId,
    PublishCompactionRequestDisposition,
) {
    let fixture = CompactionFixture::new("pending-continuation-provenance", seed);
    let id = fixture.admit(seed.wrapping_add(20), 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let operation = fixture.operation(id);
    let settlement = SettleLifecycleCompaction::new(
        &operation,
        fixture.prepare_lifecycle_content(),
        timestamp(40),
    );
    let turn = settlement.turn_id();
    assert!(matches!(
        fixture.store.execute_current(
            fixture
                .storage
                .current_settle_lifecycle_compaction(settlement)
        ),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let operation = fixture.operation(id);
    let request = PublishCompactionRequestDisposition::new(
        id,
        operation.revision(),
        operation.attempt(),
        CompactionRequestDisposition::Accepted,
    );
    (fixture, turn, request)
}

fn state(fixture: &CompactionFixture, turn: beryl_model::SyndicTurnId) -> TurnStateRecord {
    fixture
        .storage
        .turn_state(&fixture.store, turn, point_limit())
        .unwrap()
        .unwrap()
}

fn assert_valid(fixture: &CompactionFixture, request: &PublishCompactionRequestDisposition) {
    fixture
        .store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(&fixture.store, request, point_limit())
            .unwrap(),
        CompactionRequestTransitionStatus::TerminalAlreadySettled
    );
}

fn assert_invalid(fixture: &CompactionFixture, request: &PublishCompactionRequestDisposition) {
    let reconciliation = fixture.storage.compaction_request_disposition_status(
        &fixture.store,
        request,
        point_limit(),
    );
    assert!(
        matches!(
            reconciliation,
            Err(syndic_storage::SyndicReadError::Invariant(_))
        ),
        "unexpected scoped reconciliation: {reconciliation:?}"
    );
    assert!(
        fixture
            .store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}

fn cancel(fixture: &CompactionFixture, turn: beryl_model::SyndicTurnId) {
    let current = fixture
        .storage
        .current_binding(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let state = state(fixture, turn);
    let TurnDispatchProvenance::Activated(anchor) = state.dispatch_provenance() else {
        panic!("activation must retain its exact anchor");
    };
    let request = CancelBindingActivation::new(
        fixture.thread,
        current.binding().revision(),
        fixture.gate().revision(),
        state.revision(),
        current.binding().selected_path(),
        anchor.snapshot_id(),
        turn,
    );
    assert!(matches!(
        fixture
            .store
            .execute_current(fixture.storage.current_cancel_binding_activation(request)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn pending_continuation_authenticates_initial_activated_and_cancelled_descendants() {
    let (fixture, turn, request) = fixture(210);
    assert_valid(&fixture, &request);
    exact_cas::activate_turn(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        turn,
        timestamp(41),
    );
    assert_eq!(state(&fixture, turn).lifecycle(), TurnLifecycle::Pending);
    assert!(state(&fixture, turn).revision() > TurnStateRevision::FIRST);
    assert_valid(&fixture, &request);
    cancel(&fixture, turn);
    assert!(matches!(
        state(&fixture, turn).dispatch_provenance(),
        TurnDispatchProvenance::Cancelled(_)
    ));
    assert_valid(&fixture, &request);
    let fixture = fixture.reopen();
    assert_valid(&fixture, &request);
}

#[test]
fn pending_continuation_rejects_invalid_revision_provenance_and_counters() {
    for index in 0..4 {
        let (fixture, turn, request) = fixture(211 + index);
        if index == 1 {
            exact_cas::activate_turn(
                &fixture.store,
                fixture.storage.clone(),
                fixture.thread,
                turn,
                timestamp(41),
            );
        }
        let original = state(&fixture, turn);
        let forged = TurnStateRecord::with_capture_frontiers(
            turn,
            if index == 0 {
                original.revision().checked_next().unwrap()
            } else {
                TurnStateRevision::FIRST
            },
            TurnLifecycle::Pending,
            0,
            1,
            0,
            if index == 3 { 0 } else { 1 },
            0,
            None,
            original.updated_at(),
            if index == 2 {
                TurnDispatchProvenance::ProviderOperation
            } else {
                original.dispatch_provenance()
            },
        )
        .unwrap();
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::TurnState(forged)).unwrap();
        crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
        assert_invalid(&fixture, &request);
    }
}

#[test]
fn pending_continuation_requires_historical_activation_and_cancellation_authority() {
    for index in 0..3 {
        let (fixture, turn, request) = fixture(215 + index);
        exact_cas::activate_turn(
            &fixture.store,
            fixture.storage.clone(),
            fixture.thread,
            turn,
            timestamp(41),
        );
        let TurnDispatchProvenance::Activated(anchor) = state(&fixture, turn).dispatch_provenance()
        else {
            unreachable!()
        };
        if index == 2 {
            cancel(&fixture, turn);
        }
        let deletion = match index {
            0 => FixtureDelete::ExecutionSnapshot(anchor.snapshot_id()),
            1 => FixtureDelete::Binding {
                thread: fixture.thread,
                revision: anchor.binding_revision(),
            },
            _ => FixtureDelete::Binding {
                thread: fixture.thread,
                revision: anchor.binding_revision().checked_next().unwrap(),
            },
        };
        let mut batch = FixtureBatch::new();
        batch.delete(deletion).unwrap();
        crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
        assert_invalid(&fixture, &request);
    }
}

#[test]
fn cancelled_continuation_cannot_retain_a_published_cas_turn() {
    let (fixture, turn, request) = fixture(219);
    let (cas_thread, snapshot) = exact_cas::activate_turn(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        turn,
        timestamp(41),
    );
    let original = state(&fixture, turn);
    let TurnDispatchProvenance::Activated(anchor) = original.dispatch_provenance() else {
        unreachable!()
    };
    let active = syndic_storage::ActiveCasTurnRecord::new(
        snapshot,
        fixture.thread,
        turn,
        anchor.binding_revision(),
        cas_thread,
        beryl_model::CasTurnId::new("contradictory-cancelled-turn").unwrap(),
        timestamp(42),
    );
    cancel(&fixture, turn);
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::ActiveCasTurn(active)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
    assert_invalid(&fixture, &request);
}

#[test]
fn pending_continuation_rejects_a_foreign_activation_snapshot() {
    let (fixture, turn, request) = fixture(220);
    let (_, snapshot_id) = exact_cas::activate_turn(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        turn,
        timestamp(41),
    );
    let snapshot = fixture
        .storage
        .execution_snapshot(&fixture.store, snapshot_id, point_limit())
        .unwrap()
        .unwrap();
    let forged = syndic_storage::ExecutionSnapshotRecord::new(
        snapshot.id(),
        beryl_model::SyndicThreadId::from_bytes([0xFD; 16]),
        snapshot.binding_revision(),
        snapshot.activation_gate_revision(),
        snapshot.active_turn_id(),
        snapshot.cas_thread_id().clone(),
        snapshot.selected_path(),
        snapshot.represented_base_prefix(),
        snapshot.represented_base_native_turn_count(),
        snapshot.tool_profile(),
        snapshot.lineage(),
        snapshot.execution().clone(),
        snapshot.loaded_generation(),
        snapshot.started_at(),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::ExecutionSnapshot(forged)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
    assert_invalid(&fixture, &request);
}
