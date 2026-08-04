use beryl_home_store::CursorReadLimits;
use beryl_model::{InputGateRevision, SyndicItemId, SyndicTurnId};
use syndic_storage::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, CompactionAttemptNonce, CompactionConsumedWitness,
    CompactionOperationRecord, CompactionOperationState, CompactionRequestDisposition,
    CompactionRequestTransitionStatus, CompactionSettlement, CompactionSettlementReceiptRecord,
    ConversationParent, InputGateRecord, InputGateState, PromoteAcceptedInput,
    PublishCompactionRequestDisposition, SettleCompactionOperation, SettleLifecycleCompaction,
    TurnLifecycle, TurnStateRecord,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

use super::compaction_support::{CompactionFixture, execute, point_limit};
use crate::support::timestamp;

#[path = "request_reconciliation/successor_corruption.rs"]
mod successor_corruption;

fn settled_fixture(
    name: &str,
    seed: u8,
) -> (CompactionFixture, syndic_storage::CompactionOperationId) {
    let fixture = CompactionFixture::new(name, seed);
    let id = fixture.admit(seed.wrapping_add(20), 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let operation = fixture.operation(id);
    fixture
        .store
        .execute_current(fixture.storage.current_settle_compaction_operation(
            SettleCompactionOperation::new(
                id,
                operation.revision(),
                CompactionSettlement::ManualSuccess,
            ),
        ))
        .unwrap();
    (fixture, id)
}

fn request(
    fixture: &CompactionFixture,
    id: syndic_storage::CompactionOperationId,
    attempt: CompactionAttemptNonce,
    disposition: CompactionRequestDisposition,
) -> PublishCompactionRequestDisposition {
    PublishCompactionRequestDisposition::new(
        id,
        fixture.operation(id).revision(),
        attempt,
        disposition,
    )
}

#[test]
fn live_request_publication_reconciles_prior_then_exact() {
    let fixture = CompactionFixture::new("phase72-live-request", 125);
    let id = fixture.admit(145, 10);
    fixture.claim(id);
    let operation = fixture.operation(id);
    let request = request(
        &fixture,
        id,
        operation.attempt(),
        CompactionRequestDisposition::Accepted,
    );
    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(&fixture.store, &request, point_limit())
            .unwrap(),
        CompactionRequestTransitionStatus::Prior
    );
    fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_publish_compaction_request_disposition(request),
        )
        .unwrap();
    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(&fixture.store, &request, point_limit())
            .unwrap(),
        CompactionRequestTransitionStatus::Exact
    );
}

#[test]
fn late_matching_acknowledgement_reconciles_without_mutating_terminal_successor() {
    let (fixture, id) = settled_fixture("phase72-late-ack", 130);
    let before = fixture.operation(id);
    let request = request(
        &fixture,
        id,
        before.attempt(),
        CompactionRequestDisposition::Accepted,
    );

    let _ = fixture.store.execute_current(
        fixture
            .storage
            .current_publish_compaction_request_disposition(request),
    );
    assert_eq!(fixture.operation(id), before);
    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(&fixture.store, &request, point_limit())
            .unwrap(),
        CompactionRequestTransitionStatus::TerminalAlreadySettled
    );
}

#[test]
fn late_same_attempt_completion_unknown_preserves_terminal_successor() {
    let (fixture, id) = settled_fixture("phase72-late-unknown", 140);
    let before = fixture.operation(id);
    let request = request(
        &fixture,
        id,
        before.attempt(),
        CompactionRequestDisposition::CompletionUnknown,
    );

    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(&fixture.store, &request, point_limit())
            .unwrap(),
        CompactionRequestTransitionStatus::TerminalAlreadySettled
    );
    assert_eq!(fixture.operation(id), before);
}

#[test]
fn late_acknowledgement_survives_progress_beyond_user_work_settlement() {
    let fixture = CompactionFixture::new("phase72-late-ack-progressed-gate", 145);
    let id = fixture.admit(165, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let accepted = fixture.admit_current_draft_as_accepted("accepted work wins", 195, 35);
    let operation = fixture.operation(id);
    fixture
        .store
        .execute_current(fixture.storage.current_settle_lifecycle_compaction(
            SettleLifecycleCompaction::new(
                &operation,
                fixture.prepare_lifecycle_content(),
                timestamp(40),
            ),
        ))
        .unwrap();
    let consumed = fixture.operation(id);
    let late_ack = request(
        &fixture,
        id,
        consumed.attempt(),
        CompactionRequestDisposition::Accepted,
    );

    let revision = fixture.storage.revision(&fixture.store).unwrap();
    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = fixture
        .storage
        .accepted_next_source_page(&fixture.store, revision, None, limits)
        .unwrap();
    let candidate = fixture
        .storage
        .accepted_next_candidate_page(&fixture.store, sources.records()[0], None, limits)
        .unwrap()
        .into_candidate()
        .expect("user-work settlement retains one promotable accepted input");
    let promotion = PromoteAcceptedInput::new(
        candidate,
        SyndicTurnId::from_bytes([196; 16]),
        SyndicItemId::from_bytes([197; 16]),
        timestamp(41),
    );
    execute(
        &fixture.store,
        fixture.storage.promote_accepted_input(promotion),
    );
    assert!(matches!(
        fixture.gate().state(),
        InputGateState::PendingTurn(_)
    ));

    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(&fixture.store, &late_ack, point_limit())
            .unwrap(),
        CompactionRequestTransitionStatus::TerminalAlreadySettled
    );
    assert_eq!(fixture.operation(id), consumed);

    let receipt = fixture
        .storage
        .compaction_settlement_receipt(&fixture.store, id, point_limit())
        .unwrap()
        .unwrap();
    let forged_high_water = receipt.source_gate().accepted_high_water() + 1;
    let forged_source = InputGateRecord::new(
        receipt.source_gate().thread_id(),
        receipt.source_gate().revision(),
        receipt.source_gate().state().clone(),
        forged_high_water,
        receipt.source_gate().route_generation_high_water(),
        receipt.source_gate().selected_route(),
        receipt.source_gate().live_steering_count(),
        receipt.source_gate().live_next_turn_count(),
        receipt.source_gate().live_logical_utf8_bytes(),
    )
    .unwrap();
    let forged_successor = InputGateRecord::new(
        receipt.successor_gate().thread_id(),
        receipt.successor_gate().revision(),
        receipt.successor_gate().state().clone(),
        forged_high_water,
        receipt.successor_gate().route_generation_high_water(),
        receipt.successor_gate().selected_route(),
        receipt.successor_gate().live_steering_count(),
        receipt.successor_gate().live_next_turn_count(),
        receipt.successor_gate().live_logical_utf8_bytes(),
    )
    .unwrap();
    let forged_receipt = CompactionSettlementReceiptRecord::new(
        receipt.operation_id(),
        receipt.source_operation_revision(),
        receipt.successor_operation_revision(),
        forged_source,
        forged_successor,
        receipt.settlement().clone(),
        receipt.continuation(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionSettlementReceipt(forged_receipt))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage, batch);
    assert!(matches!(
        fixture.storage.compaction_request_disposition_status(
            &fixture.store,
            &late_ack,
            point_limit(),
        ),
        Err(syndic_storage::SyndicReadError::Invariant(
            "consumed compaction witness and durable successor disagree"
        ))
    ));
    let mut restore = FixtureBatch::new();
    restore
        .put(FixtureRecord::CompactionSettlementReceipt(receipt))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage, restore);

    let CompactionOperationState::Consumed(witness) = consumed.state() else {
        panic!("progressed operation must remain consumed")
    };
    let lower = InputGateRevision::new(witness.successor_gate_revision().get() - 1).unwrap();
    let forged = CompactionOperationRecord::new(
        consumed.id(),
        consumed.home_id(),
        consumed.target().clone(),
        consumed.revision(),
        consumed.attempt(),
        consumed.dispatch_claim(),
        consumed.request(),
        consumed.provider_frontier(),
        consumed.status(),
        consumed.cas_turn().cloned(),
        consumed.marker().cloned(),
        consumed.terminal(),
        CompactionOperationState::Consumed(CompactionConsumedWitness::new(
            witness.source_revision(),
            lower,
            witness.settlement().clone(),
            witness.receipt_commitment(),
        )),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::CompactionOperation(forged))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage, batch);
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit(),),
        Err(syndic_storage::SyndicReadError::Invariant(
            "consumed compaction witness and durable successor disagree"
        ))
    ));

    let mut restore_operation = FixtureBatch::new();
    restore_operation
        .put(FixtureRecord::CompactionOperation(consumed.clone()))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage, restore_operation);
    let mut delete_accepted = FixtureBatch::new();
    delete_accepted
        .delete(FixtureDelete::AcceptedInput(accepted.accepted_input_id()))
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage, delete_accepted);
    assert!(matches!(
        fixture.storage.compaction_request_disposition_status(
            &fixture.store,
            &late_ack,
            point_limit(),
        ),
        Err(syndic_storage::SyndicReadError::Invariant(
            "consumed compaction settlement successor is not durably authenticated"
        ))
    ));
}

#[test]
fn terminal_successor_rejects_contradictory_dispositions_and_attempts() {
    let (fixture, id) = settled_fixture("phase72-late-contradiction", 150);
    let operation = fixture.operation(id);
    for disposition in [
        CompactionRequestDisposition::RejectedBeforeCore,
        CompactionRequestDisposition::ProvenLocalNondispatch,
    ] {
        let request = request(&fixture, id, operation.attempt(), disposition);
        assert_eq!(
            fixture
                .storage
                .compaction_request_disposition_status(&fixture.store, &request, point_limit())
                .unwrap(),
            CompactionRequestTransitionStatus::Collision
        );
    }
    let conflicting = request(
        &fixture,
        id,
        CompactionAttemptNonce::from_bytes([251; 16]),
        CompactionRequestDisposition::Accepted,
    );
    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(&fixture.store, &conflicting, point_limit())
            .unwrap(),
        CompactionRequestTransitionStatus::Collision
    );
    let conflicting_revision = PublishCompactionRequestDisposition::new(
        id,
        operation.revision().checked_next().unwrap(),
        operation.attempt(),
        CompactionRequestDisposition::Accepted,
    );
    assert_eq!(
        fixture
            .storage
            .compaction_request_disposition_status(
                &fixture.store,
                &conflicting_revision,
                point_limit(),
            )
            .unwrap(),
        CompactionRequestTransitionStatus::Collision
    );
}
