use super::*;

fn continuation_successor_fixture(
    name: &str,
    seed: u8,
) -> (
    CompactionFixture,
    CompactionOperationRecord,
    PublishCompactionRequestDisposition,
    syndic_storage::TurnRecord,
) {
    let fixture = CompactionFixture::new(name, seed);
    let id = fixture.admit(seed.wrapping_add(20), 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
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
    let CompactionOperationState::Consumed(witness) = consumed.state() else {
        panic!("lifecycle settlement must consume the operation")
    };
    let CompactionSettlement::LifecycleContinuation { turn_id, .. } = witness.settlement() else {
        panic!("lifecycle settlement must admit its continuation")
    };
    let turn = fixture
        .storage
        .turn(&fixture.store, *turn_id, point_limit())
        .unwrap()
        .unwrap();
    (fixture, consumed, late_ack, turn)
}

#[test]
fn late_terminal_reconciliation_rejects_corrupted_provider_lifecycle_successor() {
    let (fixture, id) = settled_fixture("phase72-late-lifecycle-corruption", 146);
    let operation = fixture.operation(id);
    let late_ack = request(
        &fixture,
        id,
        operation.attempt(),
        CompactionRequestDisposition::Accepted,
    );
    let state = fixture
        .storage
        .turn_state(&fixture.store, operation.target().turn_id(), point_limit())
        .unwrap()
        .unwrap();
    let forged = TurnStateRecord::with_capture_frontiers(
        state.turn_id(),
        state.revision(),
        TurnLifecycle::Active,
        state.source_event_count(),
        state.item_count(),
        state.finalized_item_count(),
        state.open_item_count(),
        state.history_blocking_item_count(),
        None,
        state.updated_at(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::TurnState(forged)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage, batch);

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
fn late_terminal_reconciliation_rejects_missing_preserved_binding_successor() {
    let (fixture, id) = settled_fixture("phase72-late-binding-corruption", 147);
    let operation = fixture.operation(id);
    let late_ack = request(
        &fixture,
        id,
        operation.attempt(),
        CompactionRequestDisposition::Accepted,
    );
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::Binding {
            thread: operation.target().thread_id(),
            revision: operation.target().binding_revision(),
        })
        .unwrap();
    crate::support::commit(&fixture.store, fixture.storage, batch);

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
fn late_terminal_reconciliation_rejects_continuation_outside_admission_path() {
    let (fixture, _, late_ack, turn) =
        continuation_successor_fixture("phase72-late-continuation-topology", 148);
    let forged = syndic_storage::TurnRecord::new(
        turn.id(),
        turn.origin_thread_id(),
        turn.kind(),
        ConversationParent::Root,
        turn.ancestor_skip(),
        turn.depth(),
        turn.chain_digest(),
        turn.submitted_at(),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Turn(forged)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage, batch);

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
fn late_terminal_reconciliation_rejects_wrong_continuation_depth() {
    let (fixture, _, late_ack, turn) =
        continuation_successor_fixture("phase72-late-continuation-depth", 149);
    let forged = syndic_storage::TurnRecord::new(
        turn.id(),
        turn.origin_thread_id(),
        turn.kind(),
        turn.parent(),
        turn.ancestor_skip(),
        turn.depth().checked_next().unwrap(),
        turn.chain_digest(),
        turn.submitted_at(),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Turn(forged)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage, batch);

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
fn late_terminal_reconciliation_rejects_wrong_continuation_ancestor_skip() {
    let (fixture, consumed, late_ack, turn) =
        continuation_successor_fixture("phase72-late-continuation-skip", 150);
    let forged = syndic_storage::TurnRecord::new(
        turn.id(),
        turn.origin_thread_id(),
        turn.kind(),
        turn.parent(),
        Some(consumed.target().turn_id()),
        turn.depth(),
        turn.chain_digest(),
        turn.submitted_at(),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Turn(forged)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage, batch);

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
