use super::compaction_support::{CompactionFixture, point_limit};
use beryl_home_store::{
    CommandOutcome, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use std::{thread, time::Duration};
use syndic_storage::{
    ClaimCompactionDispatch, CompactionSettlement, SettleCompactionOperation, SyndicPointReadLimit,
    SyndicReadError, SyndicStorage,
    test_faults::{FixtureBatch, FixtureDelete},
};

fn open_candidate(
    path: &std::path::Path,
    faults: FaultController,
) -> (beryl_home_store::HomeOpenPublication, SyndicStorage) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    (
        candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap(),
        storage,
    )
}

fn settle(fixture: &CompactionFixture, id: syndic_storage::CompactionOperationId) {
    assert!(matches!(
        fixture
            .store
            .execute_current(fixture.storage.current_settle_compaction_operation(
                SettleCompactionOperation::new(
                    id,
                    fixture.operation(id).revision(),
                    CompactionSettlement::ManualSuccess
                )
            )),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn candidate_compaction_recovery_preserves_exact_outcomes_and_generation_fences() {
    let foreign = CompactionFixture::new("candidate-compaction-foreign", 209);
    for stage in 0..4 {
        let fixture = CompactionFixture::new("candidate-compaction-recovery", 210 + stage);
        let id = fixture.admit(220 + stage, 10);
        if stage > 0 {
            fixture.claim(id);
        }
        if stage > 1 {
            fixture.publish_success(id, 20);
        }
        if stage > 2 {
            settle(&fixture, id);
        }
        let expected = fixture
            .storage
            .compaction_recovery_read(&fixture.store, id, point_limit())
            .unwrap();
        assert!(expected.is_some());
        fixture.store.close().unwrap();
        let faults = FaultController::new();
        let (mut publication, storage) = open_candidate(fixture.home.path(), faults.clone());
        let access = publication.recovery_access().unwrap();
        assert!(
            foreign
                .storage
                .compaction_recovery_read_candidate(&access, id, point_limit())
                .is_err()
        );
        assert_eq!(
            storage
                .compaction_recovery_read_candidate(&access, id, point_limit())
                .unwrap(),
            expected
        );
        assert!(
            fixture
                .storage
                .compaction_recovery_read_candidate(&access, id, point_limit())
                .is_err()
        );
        assert!(
            storage
                .compaction_recovery_read_candidate(
                    &access,
                    id,
                    SyndicPointReadLimit::new(1).unwrap()
                )
                .is_err()
        );
        beryl_home_store::test_faults::with_initial_publication_store(&publication, |store| {
            assert!(
                storage
                    .compaction_recovery_read(store, id, point_limit())
                    .is_err()
            );
        });
        let store = publication.publish().unwrap();
        assert_eq!(
            storage
                .compaction_recovery_read(&store, id, point_limit())
                .unwrap(),
            expected
        );
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
        let mut recovered = store.recover_same_home().unwrap();
        let fresh = SyndicStorage::reacquire_candidate(&recovered).unwrap();
        let access = recovered.recovery_access().unwrap();
        assert!(
            storage
                .compaction_recovery_read_candidate(&access, id, point_limit())
                .is_err()
        );
        assert_eq!(
            fresh
                .compaction_recovery_read_candidate(&access, id, point_limit())
                .unwrap(),
            expected
        );
        recovered.publish().unwrap().close().unwrap();
    }
    foreign.store.close().unwrap();
}

#[test]
fn candidate_compaction_recovery_rejects_missing_snapshot_or_consumed_receipt() {
    for consumed in [false, true] {
        let fixture =
            CompactionFixture::new("candidate-compaction-missing", 230 + u8::from(consumed));
        let id = fixture.admit(232 + u8::from(consumed), 10);
        fixture.claim(id);
        fixture.publish_success(id, 20);
        if consumed {
            settle(&fixture, id);
        }
        let deletion = if consumed {
            FixtureDelete::CompactionSettlementReceipt(id)
        } else {
            FixtureDelete::ExecutionSnapshot(fixture.operation(id).target().snapshot_id())
        };
        let mut changes = FixtureBatch::new();
        changes.delete(deletion).unwrap();
        crate::support::commit(&fixture.store, fixture.storage.clone(), changes);
        fixture.store.close().unwrap();
        let (mut publication, storage) =
            open_candidate(fixture.home.path(), FaultController::new());
        assert!(matches!(
            storage.compaction_recovery_read_candidate(
                &publication.recovery_access().unwrap(),
                id,
                point_limit()
            ),
            Err(SyndicReadError::Invariant(_))
        ));
        publication.close().unwrap();
    }
}

#[test]
fn candidate_compaction_recovery_rejects_dispatch_change_between_passes() {
    let fixture = CompactionFixture::new("candidate-compaction-drift", 240);
    let id = fixture.admit(241, 10);
    let operation = fixture.operation(id);
    fixture.store.close().unwrap();
    let faults = FaultController::new();
    let (mut publication, storage) = open_candidate(fixture.home.path(), faults.clone());
    let access = publication.recovery_access().unwrap();
    let blocks = (0..11)
        .map(|_| faults.block_next(FaultPoint::BeforeReadConfirmation))
        .collect::<Vec<_>>();
    for block in &blocks[..10] {
        block.release();
    }
    let result = thread::scope(|scope| {
        let reader =
            scope.spawn(|| storage.compaction_recovery_read_candidate(&access, id, point_limit()));
        assert!(blocks[10].wait_until_reached(Duration::from_secs(10)));
        assert!(matches!(
            access.execute_current(storage.current_claim_compaction_dispatch(
                ClaimCompactionDispatch::new(id, operation.revision(), operation.attempt())
            )),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        blocks[10].release();
        reader.join().unwrap()
    });
    assert!(
        matches!(result, Err(SyndicReadError::ConcurrentChange { .. })),
        "{result:?}"
    );
    publication.close().unwrap();
}
