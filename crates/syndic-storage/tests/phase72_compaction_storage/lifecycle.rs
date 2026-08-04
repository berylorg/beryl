use syndic_storage::{
    BindingState, CompactionOperationState, CompactionSettlement, InputGateState,
    SettleCompactionOperation, SettleLifecycleCompaction, TurnKind, TurnLifecycle,
};

use super::compaction_support::{CompactionFixture, point_limit};
use crate::support::{converge_and_release_terminal_history, exact_cas, timestamp};

fn consumed_settlement(
    fixture: &CompactionFixture,
    id: syndic_storage::CompactionOperationId,
) -> CompactionSettlement {
    let operation = fixture.operation(id);
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("operation must be consumed");
    };
    witness.settlement().clone()
}

#[test]
fn manual_success_consumes_exact_provider_evidence_and_reopens_idle_gate() {
    let fixture = CompactionFixture::new("phase72-compaction-manual-success", 10);
    let id = fixture.admit(20, 10);
    fixture.claim(id);
    fixture.publish_request(id, syndic_storage::CompactionRequestDisposition::Accepted);
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

    assert_eq!(
        consumed_settlement(&fixture, id),
        CompactionSettlement::ManualSuccess
    );
    assert_eq!(fixture.gate().state(), &InputGateState::Idle);
    assert!(matches!(fixture.binding_state(), BindingState::Valid(_)));
    let provider_state = fixture
        .storage
        .turn_state(&fixture.store, id.provider_turn_id(), point_limit())
        .unwrap()
        .unwrap();
    let terminal = fixture.operation(id).terminal().unwrap();
    assert_eq!(provider_state.lifecycle(), TurnLifecycle::Complete);
    assert_eq!(provider_state.source_event_count(), 0);
    assert_eq!(terminal.status(), provider_state.end_status().unwrap());
    assert_eq!(terminal.turn_state_revision(), provider_state.revision());
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn lifecycle_continuation_uses_durable_home_and_preserves_current_draft() {
    let fixture = CompactionFixture::new("phase72-compaction-lifecycle-home", 30);
    let id = fixture.admit(40, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    fixture.publish_request(id, syndic_storage::CompactionRequestDisposition::Accepted);
    let content = fixture.prepare_lifecycle_content();
    let before = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let operation = fixture.operation(id);
    let request = SettleLifecycleCompaction::new(&operation, content, timestamp(40));
    let turn_id = request.turn_id();
    let item_id = request.item_id();
    fixture
        .store
        .execute_current(fixture.storage.current_settle_lifecycle_compaction(request))
        .unwrap();

    let after = fixture
        .storage
        .current_draft(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(after.draft().id(), before.draft().id());
    assert_eq!(after.draft().revision(), before.draft().revision());
    let turn = fixture
        .storage
        .turn(&fixture.store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(turn.kind(), TurnKind::BerylLifecycleContinuation);
    let state = fixture
        .storage
        .turn_state(&fixture.store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Pending);
    let item = fixture
        .storage
        .canonical_item(&fixture.store, item_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(item.presentation_content(), Some(content));
    assert_eq!(
        fixture.gate().state(),
        &InputGateState::PendingTurn(turn_id)
    );
    assert!(matches!(
        fixture.binding_state(),
        BindingState::Unbound { .. }
    ));
    assert_eq!(
        consumed_settlement(&fixture, id),
        CompactionSettlement::LifecycleContinuation {
            turn_id,
            item_id,
            content_id: content.id()
        },
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn accepted_user_work_wins_lifecycle_settlement_after_compaction_admission() {
    let fixture = CompactionFixture::new("phase72-compaction-lifecycle-user-work", 50);
    let id = fixture.admit(60, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    fixture.publish_request(id, syndic_storage::CompactionRequestDisposition::Accepted);
    fixture.admit_current_draft_as_accepted("later accepted work", 90, 35);

    let content = fixture.prepare_lifecycle_content();
    let operation = fixture.operation(id);
    let request = SettleLifecycleCompaction::new(&operation, content, timestamp(40));
    let unused_continuation = request.turn_id();
    fixture
        .store
        .execute_current(fixture.storage.current_settle_lifecycle_compaction(request))
        .unwrap();

    assert_eq!(
        consumed_settlement(&fixture, id),
        CompactionSettlement::LifecycleUserWorkWon
    );
    assert_eq!(fixture.gate().state(), &InputGateState::Idle);
    assert_eq!(fixture.gate().live_next_turn_count(), 1);
    assert!(
        fixture
            .storage
            .turn(&fixture.store, unused_continuation, point_limit())
            .unwrap()
            .is_none()
    );
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn lifecycle_continuation_accepts_active_and_terminal_descendants_across_reopen() {
    let fixture = CompactionFixture::new("phase72-compaction-continuation-descendants", 70);
    let id = fixture.admit(90, 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    let content = fixture.prepare_lifecycle_content();
    let operation = fixture.operation(id);
    let request = SettleLifecycleCompaction::new(&operation, content, timestamp(40));
    let turn_id = request.turn_id();
    let item_id = request.item_id();
    fixture
        .store
        .execute_current(fixture.storage.current_settle_lifecycle_compaction(request))
        .unwrap();

    let source = exact_cas::establish_turn(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        turn_id,
        timestamp(41),
    );
    exact_cas::admit_event(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        turn_id,
        &source,
        syndic_storage::SourceEventPayload::TurnActivated,
        timestamp(42),
    );
    fixture.store.validate_registered_domains().unwrap();
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.store, turn_id, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Active
    );

    exact_cas::correlate_user_item(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        turn_id,
        item_id,
        &source,
        timestamp(43),
    );
    exact_cas::admit_event(
        &fixture.store,
        fixture.storage,
        fixture.thread,
        turn_id,
        &source,
        syndic_storage::SourceEventPayload::TurnEnded(syndic_storage::TurnEndStatus::complete()),
        timestamp(44),
    );
    converge_and_release_terminal_history(&fixture.store, fixture.storage, fixture.thread, turn_id);
    fixture.store.validate_registered_domains().unwrap();

    let fixture = fixture.reopen();
    fixture.store.validate_registered_domains().unwrap();
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.store, turn_id, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Complete
    );
    assert!(matches!(
        fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit())
            .unwrap()
            .unwrap(),
        syndic_storage::CompactionRecoveryCase::Settled(_)
    ));
}
