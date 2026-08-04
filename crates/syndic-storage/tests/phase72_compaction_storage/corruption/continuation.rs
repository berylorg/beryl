use super::*;

#[test]
fn receipt_rejects_wrong_binding_provenance() {
    let (fixture, id) = continuation_fixture("phase72-continuation-binding-provenance", 115);
    let receipt = fixture
        .storage
        .compaction_settlement_receipt(
            &fixture.store,
            id,
            super::super::compaction_support::point_limit(),
        )
        .unwrap()
        .unwrap();
    let continuation = receipt.continuation().unwrap();
    let invalid = CompactionSettlementReceiptRecord::new(
        receipt.operation_id(),
        receipt.source_operation_revision(),
        receipt.successor_operation_revision(),
        receipt.source_gate().clone(),
        receipt.successor_gate().clone(),
        receipt.settlement().clone(),
        Some(CompactionContinuationReceipt::new(
            syndic_storage::ConversationParent::Root,
            continuation.selected_path(),
            continuation.binding_revision().checked_next().unwrap(),
            continuation.content(),
        )),
    )
    .unwrap();
    replace_receipt(&fixture, invalid);

    assert_validation_rejects(&fixture, "consumed compaction witness revision disagrees");
}

#[test]
fn receipt_rejects_wrong_parent_provenance() {
    let (fixture, id) = continuation_fixture("phase72-continuation-parent-provenance", 116);
    let receipt = fixture
        .storage
        .compaction_settlement_receipt(
            &fixture.store,
            id,
            super::super::compaction_support::point_limit(),
        )
        .unwrap()
        .unwrap();
    let continuation = receipt.continuation().unwrap();
    let invalid = CompactionSettlementReceiptRecord::new(
        receipt.operation_id(),
        receipt.source_operation_revision(),
        receipt.successor_operation_revision(),
        receipt.source_gate().clone(),
        receipt.successor_gate().clone(),
        receipt.settlement().clone(),
        Some(CompactionContinuationReceipt::new(
            syndic_storage::ConversationParent::Root,
            continuation.selected_path(),
            continuation.binding_revision(),
            continuation.content(),
        )),
    )
    .unwrap();
    replace_receipt(&fixture, invalid);

    assert_validation_rejects(&fixture, "consumed compaction witness revision disagrees");
}

#[test]
fn rejects_coherently_forged_same_thread_parent_path_and_binding() {
    let (fixture, id) = continuation_fixture("phase72-continuation-coherent-topology", 119);
    let operation = fixture.operation(id);
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("continuation fixture must be consumed")
    };
    let receipt = fixture
        .storage
        .compaction_settlement_receipt(
            &fixture.store,
            id,
            super::super::compaction_support::point_limit(),
        )
        .unwrap()
        .unwrap();
    let continuation = receipt.continuation().unwrap();
    let turn_id = continuation.selected_path().tail().unwrap();
    let turn = fixture
        .storage
        .turn(
            &fixture.store,
            turn_id,
            super::super::compaction_support::point_limit(),
        )
        .unwrap()
        .unwrap();
    let digest = root_turn_chain_digest(turn_id);
    let selected_path = SelectedPathProof::new(
        Some(turn_id),
        continuation.selected_path().thread_revision(),
        digest,
    );
    let forged_turn = TurnRecord::new(
        turn.id(),
        turn.origin_thread_id(),
        turn.kind(),
        syndic_storage::ConversationParent::Root,
        None,
        TurnDepth::FIRST,
        digest,
        turn.submitted_at(),
    );
    let forged_binding = BindingRecord::new(
        fixture.thread,
        continuation.binding_revision(),
        selected_path,
        BindingState::unbound("forged coherent continuation topology").unwrap(),
    );
    let forged_receipt = CompactionSettlementReceiptRecord::new(
        receipt.operation_id(),
        receipt.source_operation_revision(),
        receipt.successor_operation_revision(),
        receipt.source_gate().clone(),
        receipt.successor_gate().clone(),
        receipt.settlement().clone(),
        Some(CompactionContinuationReceipt::new(
            syndic_storage::ConversationParent::Root,
            selected_path,
            continuation.binding_revision(),
            continuation.content(),
        )),
    )
    .unwrap();
    let forged_witness = CompactionConsumedWitness::new(
        witness.source_revision(),
        witness.successor_gate_revision(),
        witness.settlement().clone(),
        compaction_settlement_receipt_commitment(&forged_receipt),
    );
    let forged_operation = with_consumed_witness(&operation, forged_witness);
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionSettlementReceipt(forged_receipt))
        .unwrap();
    batch
        .put(FixtureRecord::CompactionOperation(forged_operation))
        .unwrap();
    batch.put(FixtureRecord::Turn(forged_turn)).unwrap();
    batch.put(FixtureRecord::Binding(forged_binding)).unwrap();
    commit(&fixture, batch);

    assert_validation_rejects(
        &fixture,
        "compaction continuation settlement and successor disagree",
    );
}

#[test]
fn identities_are_rederived_from_durable_home_authority() {
    let (fixture, id) = continuation_fixture("phase72-continuation-derived-identity", 117);
    let operation = fixture.operation(id);
    let invalid = CompactionOperationRecord::new(
        operation.id(),
        BerylHomeId::from_bytes([0xEE; 16]),
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
        operation.state().clone(),
    )
    .unwrap();
    replace_operation(&fixture, invalid);

    assert_validation_rejects(
        &fixture,
        "compaction continuation settlement and successor disagree",
    );
}

#[test]
fn rejects_cross_thread_origin_and_forged_pending_lifecycle() {
    let (fixture, id) = continuation_fixture("phase72-continuation-origin-lifecycle", 118);
    let receipt = fixture
        .storage
        .compaction_settlement_receipt(
            &fixture.store,
            id,
            super::super::compaction_support::point_limit(),
        )
        .unwrap()
        .unwrap();
    let continuation = receipt.continuation().unwrap();
    let turn_id = continuation.selected_path().tail().unwrap();
    let turn = fixture
        .storage
        .turn(
            &fixture.store,
            turn_id,
            super::super::compaction_support::point_limit(),
        )
        .unwrap()
        .unwrap();
    let wrong_turn = TurnRecord::new(
        turn.id(),
        SyndicThreadId::from_bytes([0xED; 16]),
        turn.kind(),
        turn.parent(),
        turn.ancestor_skip(),
        turn.depth(),
        turn.chain_digest(),
        turn.submitted_at(),
    );
    let state = fixture
        .storage
        .turn_state(
            &fixture.store,
            turn_id,
            super::super::compaction_support::point_limit(),
        )
        .unwrap()
        .unwrap();
    let wrong_state = TurnStateRecord::with_capture_frontiers(
        turn_id,
        state.revision().checked_next().unwrap(),
        TurnLifecycle::Pending,
        0,
        1,
        0,
        1,
        0,
        None,
        state.updated_at(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Turn(wrong_turn)).unwrap();
    batch.put(FixtureRecord::TurnState(wrong_state)).unwrap();
    commit(&fixture, batch);

    assert_validation_rejects(
        &fixture,
        "compaction continuation settlement and successor disagree",
    );
}
