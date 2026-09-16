use super::*;
use beryl_model::InputGateRevision;
use syndic_storage::test_faults::{
    RepairTargetReplacementForTest, inject_repair_target_replacement_for_test,
};

#[test]
fn persisted_resolution_survives_finalization_rejected_freeze_and_reopen() {
    for consumed in [false, true] {
        let mut fixture = setup("resolved-repair-preservation");
        let original = super::repair_retained::terminal_target(&fixture, true);
        let request = if consumed {
            RepairRequestDisposition::Consumed(
                ConsumedRepairRequest::new(
                    RepairRequestAttemptNonce::from_bytes([22; 16]),
                    InputGateRevision::new(6).unwrap(),
                    InputGateRevision::new(7).unwrap(),
                )
                .unwrap(),
            )
        } else {
            RepairRequestDisposition::Available
        };
        let target = RepairRequiredTarget::new(
            original.turn_id(),
            original.source().clone(),
            original.gap(),
            request,
        );
        let resolved = ResolvedRepair::new(
            target.clone(),
            RepairResolution::Incomplete(TurnIncompleteReason::CompletionMismatch),
        );
        let current = fixture
            .storage
            .turn_state(&fixture.store, fixture.turn, limit())
            .unwrap()
            .unwrap();
        let state = current.with_resolved_repair(resolved.clone()).unwrap();
        exact_cas::project_item_if_needed(
            &fixture.store,
            fixture.storage.clone(),
            SyndicItemId::from_bytes([4; 16]),
        );
        inject_repair_target_replacement_for_test(
            &fixture.store,
            &fixture.storage,
            fixture.thread,
            &target,
            RepairTargetReplacementForTest::State(state.clone()),
        )
        .unwrap();
        committed_command(execute(
            &fixture.store,
            fixture.storage.finalize_next_turn_item(
                fixture.storage.revision(&fixture.store).unwrap(),
                FinalizeNextTurnItem::new(
                    fixture.thread,
                    fixture.turn,
                    state.revision(),
                    TurnItemOrdinal::FIRST,
                    SyndicItemId::from_bytes([4; 16]),
                    timestamp(10),
                ),
            ),
        ));
        let finalized = fixture
            .storage
            .turn_state(&fixture.store, fixture.turn, limit())
            .unwrap()
            .unwrap();
        assert_eq!(finalized.finalized_item_count(), 1);
        assert_eq!(finalized.resolved_repair(), Some(&resolved));
        let before_freeze = fixture.storage.revision(&fixture.store).unwrap();
        let rejected = not_committed_command(execute(
            &fixture.store,
            fixture.storage.freeze_next_turn_item(
                fixture.storage.revision(&fixture.store).unwrap(),
                FreezeNextTurnItem::new(
                    fixture.thread,
                    fixture.turn,
                    finalized.revision(),
                    TurnItemOrdinal::new(2).unwrap(),
                    fixture.assistant,
                    timestamp(11),
                ),
            ),
        ));
        assert!(matches!(
            typed_error(&rejected),
            SyndicMutationError::CanonicalFinalizationConflict
        ));
        assert_eq!(
            fixture.storage.revision(&fixture.store).unwrap(),
            before_freeze
        );
        fixture.store.close().unwrap();
        let mut reopened = open(fixture.home.path());
        fixture.storage = SyndicStorage::register(&mut reopened).unwrap();
        fixture.store = reopened
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let after = fixture
            .storage
            .turn_state(&fixture.store, fixture.turn, limit())
            .unwrap()
            .unwrap();
        assert_eq!(after.resolved_repair(), Some(&resolved));
        assert_eq!(after, finalized);
        assert_eq!(
            after.source_event_count(),
            target.gap().terminal().sequence().get()
        );
    }
}
