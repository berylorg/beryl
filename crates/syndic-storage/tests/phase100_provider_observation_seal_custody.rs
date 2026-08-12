#![cfg(feature = "test-faults")]

use beryl_home_store::{
    CommandError, CommandOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::ProviderObservationId;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use syndic_storage::test_faults::{
    ProviderObservationStagerLifetimeProbe, provider_observation_stage_fault_scope,
    provider_observation_stager_lifetime_probe,
};
use syndic_storage::{
    ProviderField, ProviderObservationBegin, ProviderObservationControl,
    ProviderObservationItemKind, ProviderObservationItemLifecycle,
    ProviderObservationSealCustodyGuard, ProviderObservationSealOutcome,
    ProviderObservationStageBatch, ProviderObservationStageOutcome, ProviderObservationStager,
    ProviderObservationStagingBytes, ProviderScalar, ProviderValueContext, SyndicStorage,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome {
    path: PathBuf,
}

impl TestHome {
    fn new(name: &str) -> Self {
        loop {
            let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "beryl-syndic-{name}-{}-{sequence}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create isolated test home {path:?}: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn committed<T>(outcome: ProviderObservationStageOutcome<T>) -> T {
    match outcome {
        ProviderObservationStageOutcome::Committed {
            value,
            later_failure: None,
            ..
        } => value,
        ProviderObservationStageOutcome::Committed {
            later_failure: Some(failure),
            ..
        } => panic!("expected clean committed staging outcome, got {failure:?}"),
        ProviderObservationStageOutcome::NotCommitted { evidence } => {
            panic!("expected clean committed staging outcome, got {evidence:?}")
        }
        ProviderObservationStageOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            panic!("expected clean committed staging outcome, got {failure:?}")
        }
    }
}

fn indeterminate_seal_custody(
    store: &HomeStore,
    storage: &SyndicStorage,
    faults: &FaultController,
    identity: u8,
) -> (
    ProviderObservationSealCustodyGuard,
    ProviderObservationStagerLifetimeProbe,
) {
    let mut callback = |batch: &ProviderObservationStageBatch| -> CommandOutcome {
        store.execute_current(storage.current_stage_provider_observation_batch(batch.clone()))
    };

    let mut stager = committed(
        ProviderObservationStager::begin(
            ProviderObservationId::from_bytes([identity; 16]),
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Completed,
                kind: ProviderObservationItemKind::ContextCompaction,
            },
            &mut callback,
        )
        .unwrap(),
    );
    committed(
        stager
            .control(
                ProviderObservationControl::Scalar {
                    context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
                    value: ProviderScalar::Unsigned(42),
                },
                &mut callback,
            )
            .unwrap(),
    );
    let item = ProviderValueContext::Field(ProviderField::ItemId);
    committed(
        stager
            .control(ProviderObservationControl::BeginField(item), &mut callback)
            .unwrap(),
    );
    committed(
        stager
            .fragment(
                ProviderObservationStagingBytes::new(item, b"provider-item").unwrap(),
                &mut callback,
            )
            .unwrap(),
    );
    committed(
        stager
            .control(ProviderObservationControl::EndField(item), &mut callback)
            .unwrap(),
    );

    let lifetime = provider_observation_stager_lifetime_probe(&stager);
    faults.fail_next_in_scope(
        FaultPoint::AfterCommitBeforePersist,
        provider_observation_stage_fault_scope(),
    );
    let custody = match stager.seal(&mut callback).unwrap() {
        ProviderObservationSealOutcome::Indeterminate {
            failure: CommandError::Persistence { .. },
            custody,
        } => custody,
        other => panic!("expected exact indeterminate seal outcome, got {other:?}"),
    };

    (custody, lifetime)
}

#[test]
fn indeterminate_seal_retains_stager_until_custody_installation() {
    let home = TestHome::new("provider-observation-seal-custody");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (custody, lifetime) = indeterminate_seal_custody(&store, &storage, &faults, 100);

    assert!(lifetime.is_retained());
    custody.install();
    assert!(!lifetime.is_retained());
}

#[test]
fn dropping_indeterminate_seal_guard_installs_before_releasing_stager() {
    let home = TestHome::new("provider-observation-seal-drop-custody");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (custody, lifetime) = indeterminate_seal_custody(&store, &storage, &faults, 101);

    assert!(lifetime.is_retained());
    drop(custody);
    assert!(!lifetime.is_retained());

    let close_error = store.close().unwrap_err();
    assert_eq!(close_error.pending_reconciliation_scopes(), Some(1));
    drop(close_error);
    assert!(
        HomeStore::open(HomeOpenOptions::new(
            home.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .is_err()
    );
}
