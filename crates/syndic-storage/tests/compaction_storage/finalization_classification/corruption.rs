use super::*;
use syndic_storage::{
    CompactionOperationRecord, CompactionTerminalObservation, DeliveryRecoveryClassificationError,
    SyndicReadError, TurnEndStatus, TurnRecord, TurnTerminalOutcome,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

fn fixture(seed: u8) -> (CompactionFixture, syndic_storage::CompactionOperationId) {
    let fixture = CompactionFixture::new("finalization-classification-corruption", seed);
    let id = fixture.admit(seed.wrapping_add(20), 10);
    fixture.claim(id);
    fixture.publish_success(id, 20);
    (fixture, id)
}

fn assert_corruption(fixture: &CompactionFixture) {
    assert!(matches!(
        fixture
            .storage
            .classify_delivery_recovery(&fixture.store, &source(fixture), point_limit()),
        Err(DeliveryRecoveryClassificationError::Corruption(_))
    ));
    assert!(matches!(
        fixture
            .storage
            .stop_admission_read(&fixture.store, fixture.thread, point_limit()),
        Err(SyndicReadError::Invariant(_))
    ));
}

#[test]
fn finalization_requires_every_authority_record() {
    for index in 0..6 {
        let (fixture, id) = fixture(181 + index);
        let operation = fixture.operation(id);
        let deleted = match index {
            0 => FixtureDelete::CompactionOperation(id),
            1 => FixtureDelete::ExecutionSnapshot(operation.target().snapshot_id()),
            2 => FixtureDelete::ActiveCasTurn(operation.target().snapshot_id()),
            3 => FixtureDelete::Turn(id.provider_turn_id()),
            4 => FixtureDelete::TurnState(id.provider_turn_id()),
            _ => FixtureDelete::Binding {
                thread: fixture.thread,
                revision: operation.target().binding_revision(),
            },
        };
        let mut batch = FixtureBatch::new();
        batch.delete(deleted).unwrap();
        crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
        if index == 5 {
            assert!(
                fixture
                    .storage
                    .classify_delivery_recovery(&fixture.store, &source(&fixture), point_limit())
                    .is_err()
            );
            assert!(
                fixture
                    .storage
                    .stop_admission_read(&fixture.store, fixture.thread, point_limit())
                    .is_err()
            );
        } else {
            assert_corruption(&fixture);
        }
    }
}

#[test]
fn finalization_rejects_mismatched_home_and_terminal_evidence() {
    for index in 0..3 {
        let (fixture, id) = fixture(188 + index);
        let operation = fixture.operation(id);
        let terminal = operation.terminal().unwrap();
        let terminal = match index {
            1 => CompactionTerminalObservation::new(
                terminal.sequence(),
                TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
                terminal.turn_state_revision(),
            ),
            2 => CompactionTerminalObservation::new(
                terminal.sequence(),
                terminal.status(),
                syndic_storage::TurnStateRevision::FIRST,
            ),
            _ => terminal,
        };
        let forged = CompactionOperationRecord::new(
            id,
            if index == 0 {
                beryl_model::BerylHomeId::from_bytes([0xF1; 16])
            } else {
                operation.home_id()
            },
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
        .unwrap();
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::CompactionOperation(forged))
            .unwrap();
        crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
        assert_corruption(&fixture);
    }
}

#[test]
fn finalization_rejects_an_ordinary_or_foreign_provider_turn() {
    for foreign in [false, true] {
        let (fixture, id) = fixture(192 + u8::from(foreign));
        let turn = fixture
            .storage
            .turn(&fixture.store, id.provider_turn_id(), point_limit())
            .unwrap()
            .unwrap();
        let forged = TurnRecord::new(
            turn.id(),
            if foreign {
                beryl_model::SyndicThreadId::from_bytes([0xF2; 16])
            } else {
                turn.origin_thread_id()
            },
            if foreign {
                turn.kind()
            } else {
                syndic_storage::TurnKind::OrdinaryUser
            },
            turn.parent(),
            turn.ancestor_skip(),
            turn.depth(),
            turn.chain_digest(),
            turn.submitted_at(),
        );
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::Turn(forged)).unwrap();
        crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
        assert_corruption(&fixture);
    }
}

#[test]
fn finalization_rejects_live_steering_custody() {
    let (fixture, _) = fixture(194);
    let gate = fixture.gate();
    let forged = syndic_storage::InputGateRecord::new(
        gate.thread_id(),
        gate.revision(),
        gate.state().clone(),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        1,
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::InputGate(forged)).unwrap();
    crate::support::commit(&fixture.store, fixture.storage.clone(), batch);
    assert_corruption(&fixture);
}
