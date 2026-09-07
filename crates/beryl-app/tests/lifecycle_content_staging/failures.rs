use beryl_home_store::{
    HomeCommand, HomeHealthState, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::ContentRevision;
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

use super::*;

#[test]
fn partial_fixed_content_preserves_bytes_and_accepted_input_with_bounded_failure_feedback() {
    for sealed in [false, true] {
        let fixture =
            LifecycleFixture::with_accepted_next(205 + u8::from(sealed), 213 + u8::from(sealed));
        let prepared = prepare_lifecycle_continuation_content().unwrap();
        let manifest = if sealed {
            prepared.sealed_manifest(ContentRevision::new(9).unwrap())
        } else {
            prepared.building_manifest()
        };
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::ContentManifest(manifest)).unwrap();
        {
            let command = fixture.service.live_home_command().unwrap();
            let home = command.home();
            let mut mutation = HomeCommand::new(home.home_revision().unwrap());
            mutation
                .add(
                    fixture
                        .storage
                        .fixture_contribution(fixture.storage.revision(home).unwrap(), batch),
                )
                .unwrap();
            assert_committed(home.execute(mutation));
        }
        let before = lifecycle_content_canonical_records(
            fixture.service.live_home_command().unwrap().home(),
            &fixture.storage,
        );
        let gate_before = fixture.input_gate();
        let tail_before = fixture.committed_tail();
        fixture.publish_success_prefix();
        fixture.publish_success_terminal();
        let operation = fixture.operation();
        let CompactionOperationState::Consumed(witness) = operation.state() else {
            panic!("content rejection did not finish compaction: {operation:?}");
        };
        assert_eq!(witness.settlement(), &CompactionSettlement::ManualSuccess);
        assert_eq!(fixture.committed_tail(), tail_before);
        let gate_after = fixture.input_gate();
        assert_eq!(gate_after.state(), &InputGateState::Idle);
        assert_eq!(gate_after.live_count(), 1);
        assert_eq!(
            gate_after.accepted_high_water(),
            gate_before.accepted_high_water()
        );
        assert_eq!(
            gate_after.live_logical_utf8_bytes(),
            gate_before.live_logical_utf8_bytes()
        );
        let diagnostics = fixture.service.context_compaction_diagnostics();
        assert_eq!(diagnostics.lifecycle_continuation_failures(), 1);
        assert_eq!(diagnostics.retained_operations(), 0);
        assert_eq!(
            fixture
                .service
                .take_terminal_lifecycle_yield_outcome(fixture.thread_id, fixture.yielding_turn_id)
                .unwrap(),
            None
        );
        {
            let command = fixture.service.live_home_command().unwrap();
            assert_eq!(
                lifecycle_content_canonical_records(command.home(), &fixture.storage),
                before
            );
            assert_eq!(command.home().health().state(), HomeHealthState::Healthy);
            assert!(command.home().pending_reconciliations().is_empty());
            if !sealed {
                assert!(
                    fixture
                        .storage
                        .content_manifest(command.home(), prepared.id(), point_limit())
                        .is_err()
                );
            }
        }
        fixture.close();
    }
}

#[test]
fn definitive_preparation_failure_finishes_compaction_without_publishing_content() {
    let fixture = LifecycleFixture::new(207, 215);
    let tail_before = fixture.committed_tail();
    fixture.harness.fail_next_lifecycle_staging().unwrap();
    fixture.publish_success_prefix();
    fixture.publish_success_terminal();
    let operation = fixture.operation();
    let CompactionOperationState::Consumed(witness) = operation.state() else {
        panic!("preparation failure left compaction active: {operation:?}");
    };
    assert_eq!(witness.settlement(), &CompactionSettlement::ManualSuccess);
    assert_eq!(fixture.committed_tail(), tail_before);
    assert_eq!(fixture.input_gate().state(), &InputGateState::Idle);
    assert!(
        lifecycle_content_canonical_records(
            fixture.service.live_home_command().unwrap().home(),
            &fixture.storage
        )
        .is_empty()
    );
    let diagnostics = fixture.service.context_compaction_diagnostics();
    assert_eq!(diagnostics.lifecycle_continuation_failures(), 1);
    assert_eq!(diagnostics.retained_operations(), 0);
    fixture.close();
}

#[test]
fn ambiguous_fresh_publication_keeps_exact_custody_through_coordinator_retirement() {
    let faults = FaultController::new();
    let source = syndic::Fixture::with_faults(208, faults.clone());
    let harness = source
        .store
        .context_compaction_lifecycle_test_harness()
        .unwrap();
    let before = source.home().home_revision().unwrap();
    let gate_before = source
        .storage
        .input_gate(&source.home(), source.thread, point_limit())
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(
        source
            .store
            .stage_context_compaction_continuation_for_test()
            .is_err()
    );
    let scopes = source.home().pending_reconciliations();
    assert_eq!(scopes.len(), 1);
    assert_eq!(source.home().health().state(), HomeHealthState::Healthy);
    harness.request_shutdown().unwrap();
    assert_eq!(source.home().pending_reconciliations().len(), 1);
    assert_eq!(
        source
            .storage
            .input_gate(&source.home(), source.thread, point_limit())
            .unwrap(),
        gate_before
    );
    assert_eq!(
        source
            .store
            .context_compaction_diagnostics()
            .retained_operations(),
        0
    );
    match source.home().reconcile(&scopes[0]).unwrap() {
        ReconciliationResolution::ExactNew { receipt } => {
            assert_eq!(receipt.home_revision(), before.checked_next().unwrap())
        }
        other => panic!("publication did not reconcile to the exact sealed object: {other:?}"),
    }
    assert!(source.home().pending_reconciliations().is_empty());
    assert_eq!(
        lifecycle_content_canonical_records(&source.home(), &source.storage).len(),
        5
    );
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let manifest = source
        .storage
        .content_manifest(&source.home(), prepared.id(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        manifest.sealed_reference(),
        Some(prepared.reference(ContentRevision::new(1).unwrap()))
    );
    let (directory, service) = source.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}
