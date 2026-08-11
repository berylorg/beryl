use beryl_home_store::CommandOutcome;
use beryl_model::{BerylHomeId, SyndicThreadId};
use syndic_storage::{
    BindingRecord, BindingState, CasTurnSource, CompactionConsumedWitness,
    CompactionContinuationReceipt, CompactionOperationId, CompactionOperationRecord,
    CompactionOperationState, CompactionOperationTarget, CompactionSettlement,
    CompactionSettlementReceiptRecord, CompactionTerminalObservation, SelectedPathProof,
    SettleCompactionOperation, SourceEventPayload, SourceEventRecord, SourceEventSequence,
    TurnDepth, TurnEndStatus, TurnLifecycle, TurnRecord, TurnStateRecord, TurnTerminalOutcome,
    root_turn_chain_digest,
    test_faults::{
        FixtureBatch, FixtureDelete, FixtureRecord, compaction_settlement_receipt_commitment,
    },
};

use super::compaction_support::CompactionFixture;

#[path = "corruption/continuation.rs"]
mod continuation;

fn successful_fixture(name: &str, id_byte: u8) -> (CompactionFixture, CompactionOperationId) {
    let fixture = CompactionFixture::new(name, id_byte);
    let id = fixture.admit(id_byte.wrapping_add(20), 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let operation = fixture.operation(id);
    match fixture
        .store
        .execute_current(fixture.storage.current_settle_compaction_operation(
            SettleCompactionOperation::new(
                id,
                operation.revision(),
                CompactionSettlement::ManualSuccess,
            ),
        )) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean corrupt-fixture compaction settlement, got {outcome:?}"),
    }
    fixture.store.validate_registered_domains().unwrap();
    (fixture, id)
}

fn continuation_fixture(name: &str, seed: u8) -> (CompactionFixture, CompactionOperationId) {
    let fixture = CompactionFixture::new(name, seed);
    let id = fixture.admit(seed.wrapping_add(20), 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let content = fixture.prepare_lifecycle_content();
    let operation = fixture.operation(id);
    match fixture
        .store
        .execute_current(fixture.storage.current_settle_lifecycle_compaction(
            syndic_storage::SettleLifecycleCompaction::new(
                &operation,
                content,
                crate::support::timestamp(40),
            ),
        )) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean continuation fixture settlement, got {outcome:?}"),
    }
    (fixture, id)
}

fn commit(fixture: &CompactionFixture, batch: FixtureBatch) {
    crate::support::commit(&fixture.store, fixture.storage, batch);
}

fn replace_operation(fixture: &CompactionFixture, operation: CompactionOperationRecord) {
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionOperation(operation))
        .unwrap();
    commit(fixture, batch);
}

fn replace_receipt(fixture: &CompactionFixture, receipt: CompactionSettlementReceiptRecord) {
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionSettlementReceipt(receipt))
        .unwrap();
    commit(fixture, batch);
}

fn with_terminal(
    operation: &CompactionOperationRecord,
    terminal: CompactionTerminalObservation,
) -> CompactionOperationRecord {
    CompactionOperationRecord::new(
        operation.id(),
        operation.home_id(),
        operation.target().clone(),
        operation.revision(),
        operation.attempt(),
        operation.dispatch_claim(),
        operation.request(),
        operation.provider_frontier(),
        operation.status(),
        operation.cas_turn().cloned(),
        operation.marker().cloned(),
        Some(terminal),
        operation.state().clone(),
    )
    .unwrap()
}

fn with_consumed_witness(
    operation: &CompactionOperationRecord,
    witness: CompactionConsumedWitness,
) -> CompactionOperationRecord {
    CompactionOperationRecord::new(
        operation.id(),
        operation.home_id(),
        operation.target().clone(),
        operation.revision(),
        operation.attempt(),
        operation.dispatch_claim(),
        operation.request(),
        operation.provider_frontier(),
        operation.status(),
        operation.cas_turn().cloned(),
        operation.marker().cloned(),
        operation.terminal(),
        CompactionOperationState::Consumed(witness),
    )
    .unwrap()
}

fn assert_validation_rejects(fixture: &CompactionFixture, expected: &str) {
    let error = fixture.store.validate_registered_domains().unwrap_err();
    assert!(
        error.to_string().contains(expected),
        "expected `{expected}`, got `{error}`"
    );
}

fn assert_recovery_rejects(fixture: &CompactionFixture, id: CompactionOperationId) {
    let result = fixture.storage.compaction_recovery_read(
        &fixture.store,
        id,
        super::compaction_support::point_limit(),
    );
    assert!(
        matches!(
            &result,
            Err(syndic_storage::SyndicReadError::Invariant(
                "consumed compaction witness and durable successor disagree"
            ))
        ),
        "unexpected recovery result: {result:?}"
    );
}

#[test]
fn consumed_witness_source_must_be_the_immediate_operation_predecessor() {
    let (fixture, id) = successful_fixture("phase72-compaction-consumed-source", 111);
    let operation = fixture.operation(id);
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("successful fixture must be consumed")
    };
    let invalid = CompactionConsumedWitness::new(
        syndic_storage::CompactionOperationRevision::FIRST,
        witness.successor_gate_revision(),
        witness.settlement().clone(),
        witness.receipt_commitment(),
    );
    replace_operation(&fixture, with_consumed_witness(&operation, invalid));

    assert_recovery_rejects(&fixture, id);
    assert_validation_rejects(&fixture, "consumed compaction witness revision disagrees");
}

#[test]
fn consumed_witness_gate_must_name_the_actual_durable_successor() {
    let (fixture, id) = successful_fixture("phase72-compaction-consumed-gate", 112);
    let operation = fixture.operation(id);
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("successful fixture must be consumed")
    };
    let invalid = CompactionConsumedWitness::new(
        witness.source_revision(),
        witness.successor_gate_revision().checked_next().unwrap(),
        witness.settlement().clone(),
        witness.receipt_commitment(),
    );
    replace_operation(&fixture, with_consumed_witness(&operation, invalid));

    assert_recovery_rejects(&fixture, id);
    assert_validation_rejects(&fixture, "consumed compaction witness revision disagrees");
}

#[test]
fn consumed_witness_settlement_must_match_the_retained_terminal_successor() {
    let (fixture, id) = successful_fixture("phase72-compaction-consumed-settlement", 113);
    let operation = fixture.operation(id);
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("successful fixture must be consumed")
    };
    let invalid = CompactionConsumedWitness::new(
        witness.source_revision(),
        witness.successor_gate_revision(),
        CompactionSettlement::LifecycleUserWorkWon,
        witness.receipt_commitment(),
    );
    replace_operation(&fixture, with_consumed_witness(&operation, invalid));

    assert_recovery_rejects(&fixture, id);
    assert_validation_rejects(&fixture, "consumed compaction witness revision disagrees");
}

#[test]
fn consumed_operation_requires_its_independent_settlement_receipt() {
    let (fixture, id) = successful_fixture("phase72-compaction-missing-receipt", 114);
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::CompactionSettlementReceipt(id))
        .unwrap();
    commit(&fixture, batch);

    assert_recovery_rejects(&fixture, id);
    assert_validation_rejects(
        &fixture,
        "consumed compaction settlement receipt is missing",
    );
}

#[test]
fn complete_provider_turn_without_its_operation_record_is_corruption() {
    let (fixture, id) = successful_fixture("phase72-compaction-missing-authority", 80);
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::CompactionOperation(id))
        .unwrap();
    commit(&fixture, batch);

    assert_validation_rejects(
        &fixture,
        "compaction settlement receipt operation is missing",
    );
}

#[test]
fn ordinary_terminal_source_beside_compaction_authority_is_corruption() {
    let (fixture, id) = successful_fixture("phase72-compaction-duplicate-terminal", 90);
    let operation = fixture.operation(id);
    let terminal = operation.terminal().unwrap();
    let source = CasTurnSource::new(
        operation.target().cas_thread_id().clone(),
        operation.cas_turn().unwrap().cas_turn_id().clone(),
    );
    let event = SourceEventRecord::new(
        id.provider_turn_id(),
        SourceEventSequence::FIRST,
        Some(source),
        SourceEventPayload::TurnEnded(terminal.status()),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::SourceEvent(event)).unwrap();
    commit(&fixture, batch);

    assert_validation_rejects(
        &fixture,
        "complete compaction turn has ordinary terminal source authority",
    );
}

#[test]
fn terminal_status_disagreement_is_corruption() {
    let (fixture, id) = successful_fixture("phase72-compaction-status-mismatch", 100);
    let operation = fixture.operation(id);
    let terminal = operation.terminal().unwrap();
    replace_operation(
        &fixture,
        with_terminal(
            &operation,
            CompactionTerminalObservation::new(
                terminal.sequence(),
                TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
                terminal.turn_state_revision(),
            ),
        ),
    );

    assert_validation_rejects(
        &fixture,
        "compaction terminal and provider turn state disagree",
    );
}

#[test]
fn terminal_turn_state_revision_disagreement_is_corruption() {
    let (fixture, id) = successful_fixture("phase72-compaction-revision-mismatch", 110);
    let operation = fixture.operation(id);
    let terminal = operation.terminal().unwrap();
    replace_operation(
        &fixture,
        with_terminal(
            &operation,
            CompactionTerminalObservation::new(
                terminal.sequence(),
                terminal.status(),
                syndic_storage::TurnStateRevision::FIRST,
            ),
        ),
    );

    assert_validation_rejects(
        &fixture,
        "compaction terminal authority and complete turn state disagree",
    );
}

#[test]
fn second_record_targeting_the_same_provider_turn_is_corruption() {
    let (fixture, id) = successful_fixture("phase72-compaction-multiple-authority", 120);
    let operation = fixture.operation(id);
    let other_thread = SyndicThreadId::from_bytes([240; 16]);
    let other_id = CompactionOperationId::new(other_thread, id.nonce());
    let target = CompactionOperationTarget::new(
        other_thread,
        id.provider_turn_id(),
        operation.target().snapshot_id(),
        operation.target().binding_revision(),
        operation.target().runtime_id(),
        operation.target().loaded_generation(),
        operation.target().cas_thread_id().clone(),
    );
    let duplicate = CompactionOperationRecord::new(
        other_id,
        operation.home_id(),
        target,
        operation.revision(),
        operation.attempt(),
        operation.dispatch_claim(),
        operation.request(),
        operation.provider_frontier(),
        operation.status(),
        operation.cas_turn().cloned(),
        operation.marker().cloned(),
        operation.terminal(),
        operation.state().clone(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionOperation(duplicate))
        .unwrap();
    commit(&fixture, batch);

    assert_validation_rejects(
        &fixture,
        "compaction turn, snapshot, and immutable target disagree",
    );
}
