#![cfg(feature = "test-faults")]

#[path = "support/mod.rs"]
mod support;

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    ReconciliationHandle, ReconciliationResolution,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{DraftRevision, ProviderObservationId, ThreadRevision};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::{
    DraftByThreadRecord, ProviderField, ProviderObservationBegin, ProviderObservationControl,
    ProviderObservationItemKind, ProviderObservationItemLifecycle, ProviderObservationStageBatch,
    ProviderObservationStageOutcome, ProviderObservationStager, ProviderScalar,
    ProviderValueContext, SyndicStorage,
};

use support::{TestHome, draft_id, id};

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn index(thread: u8, draft: u8, revision: u64) -> DraftByThreadRecord {
    DraftByThreadRecord::new(
        id(thread),
        draft_id(draft),
        DraftRevision::new(revision).unwrap(),
        ThreadRevision::new(1).unwrap(),
    )
}

fn batch(records: impl IntoIterator<Item = FixtureRecord>) -> FixtureBatch {
    let mut batch = FixtureBatch::new();
    for record in records {
        batch.put(record).unwrap();
    }
    batch
}

fn commit_fixture(store: &HomeStore, storage: SyndicStorage, batch: FixtureBatch) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn indeterminate_fixture(
    store: &HomeStore,
    storage: SyndicStorage,
    faults: &FaultController,
    batch: FixtureBatch,
) -> ReconciliationHandle {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(store).unwrap(), batch))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    match store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        outcome => panic!("expected indeterminate fixture outcome, got {outcome:?}"),
    }
}

#[test]
fn exact_old_and_exact_new_classify_from_only_descriptor_records() {
    let home = TestHome::new("phase102-exact-sides");
    let faults = FaultController::new();
    let mut store = open(home.path(), faults.clone());
    let storage = SyndicStorage::register(&mut store).unwrap();

    let old = index(1, 2, 1);
    let new = index(1, 3, 2);
    commit_fixture(
        &store,
        storage,
        batch([FixtureRecord::DraftByThread(old.clone())]),
    );
    let exact_new = indeterminate_fixture(
        &store,
        storage,
        &faults,
        batch([FixtureRecord::DraftByThread(new.clone())]),
    );
    assert!(matches!(
        store.reconcile(&exact_new).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));

    let exact_old = indeterminate_fixture(
        &store,
        storage,
        &faults,
        batch([FixtureRecord::DraftByThread(index(1, 4, 3))]),
    );
    commit_fixture(&store, storage, batch([FixtureRecord::DraftByThread(new)]));
    assert_eq!(
        store.reconcile(&exact_old).unwrap(),
        ReconciliationResolution::ExactOld
    );
    store.close().unwrap();
}

#[test]
fn provider_observation_build_and_chunk_records_classify_exact_new() {
    let home = TestHome::new("phase102-provider-observation");
    let faults = FaultController::new();
    let mut store = open(home.path(), faults.clone());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut callback = |batch: &ProviderObservationStageBatch| {
        store.execute_current(storage.current_stage_provider_observation_batch(batch.clone()))
    };

    let mut stager = match ProviderObservationStager::begin(
        ProviderObservationId::from_bytes([42; 16]),
        ProviderObservationBegin::Item {
            lifecycle: ProviderObservationItemLifecycle::Completed,
            kind: ProviderObservationItemKind::ContextCompaction,
        },
        &mut callback,
    )
    .unwrap()
    {
        ProviderObservationStageOutcome::Committed {
            value,
            later_failure: None,
            ..
        } => value,
        ProviderObservationStageOutcome::Committed {
            later_failure: Some(failure),
            ..
        } => panic!("expected clean provider begin, got {failure:?}"),
        _ => panic!("expected clean provider begin"),
    };
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match stager
        .control(
            ProviderObservationControl::Scalar {
                context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
                value: ProviderScalar::Unsigned(42),
            },
            &mut callback,
        )
        .unwrap()
    {
        ProviderObservationStageOutcome::Indeterminate { reconciliation, .. } => {
            reconciliation.install_and_handle()
        }
        _ => panic!("expected indeterminate provider control"),
    };
    assert!(matches!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    store.close().unwrap();
}

#[test]
fn mixed_and_neither_descriptor_records_seal_collision() {
    let home = TestHome::new("phase102-collision-sides");
    let faults = FaultController::new();
    let mut store = open(home.path(), faults.clone());
    let storage = SyndicStorage::register(&mut store).unwrap();

    let first_old = index(10, 11, 1);
    let second_old = index(12, 13, 1);
    let first_new = index(10, 14, 2);
    let second_new = index(12, 15, 2);
    commit_fixture(
        &store,
        storage,
        batch([
            FixtureRecord::DraftByThread(first_old.clone()),
            FixtureRecord::DraftByThread(second_old),
        ]),
    );
    let mixed = indeterminate_fixture(
        &store,
        storage,
        &faults,
        batch([
            FixtureRecord::DraftByThread(first_new),
            FixtureRecord::DraftByThread(second_new),
        ]),
    );
    commit_fixture(
        &store,
        storage,
        batch([FixtureRecord::DraftByThread(first_old)]),
    );
    assert_eq!(
        store.reconcile(&mixed).unwrap(),
        ReconciliationResolution::Collision
    );

    let neither = indeterminate_fixture(
        &store,
        storage,
        &faults,
        batch([FixtureRecord::DraftByThread(index(20, 21, 1))]),
    );
    commit_fixture(
        &store,
        storage,
        batch([FixtureRecord::DraftByThread(index(20, 22, 2))]),
    );
    commit_fixture(
        &store,
        storage,
        batch([FixtureRecord::DraftByThread(index(20, 23, 3))]),
    );
    assert_eq!(
        store.reconcile(&neither).unwrap(),
        ReconciliationResolution::Collision
    );
    store.close().unwrap();
}
