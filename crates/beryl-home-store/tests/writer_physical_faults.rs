#![cfg(feature = "test-faults")]

mod support;

use std::{
    error::Error,
    io,
    panic::{AssertUnwindSafe, catch_unwind},
};

use beryl_home_store::{
    CommandError, DomainMutation, DomainReader, DomainValidator, HomeCommand, HomeHealthState,
    HomeOpenOptions, HomeRecoveryError, HomeSchemaVersion, HomeStore, MutationBuilder,
    PointReadLimit, ReadError,
    test_faults::{FaultController, FaultPoint},
};
use tempfile::tempdir;

use support::{AlphaDomain, BetaDomain, BytesRecord, FixtureMutationError, PutBytes};

struct RequireBeta;

impl DomainValidator<BetaDomain> for RequireBeta {
    type Error = FixtureMutationError;

    fn validate(&self, reader: &DomainReader<'_, BetaDomain>) -> Result<(), Self::Error> {
        let value = reader
            .point::<BytesRecord<BetaDomain>>(&7, PointReadLimit::new(1_028).unwrap())
            .map_err(|_| FixtureMutationError::Rejected("validator read failed"))?;
        if value.as_deref() != Some(b"guarded") {
            return Err(FixtureMutationError::Rejected("validator value changed"));
        }
        Ok(())
    }
}

struct PanicValidator;

impl DomainValidator<BetaDomain> for PanicValidator {
    type Error = FixtureMutationError;

    fn validate(&self, _reader: &DomainReader<'_, BetaDomain>) -> Result<(), Self::Error> {
        panic!("synthetic validation-only participant panic")
    }
}

struct PutMany {
    count: u64,
}

impl DomainMutation<AlphaDomain> for PutMany {
    type Error = FixtureMutationError;

    fn validate(&self, _reader: &DomainReader<'_, AlphaDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AlphaDomain>,
        mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        let value = vec![0];
        for key in 0..self.count {
            mutations.put::<BytesRecord<AlphaDomain>>(&key, &value)?;
        }
        Ok(())
    }
}

fn open(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn put_command(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    key: u64,
    value: &[u8],
) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<AlphaDomain>::new(key, value.to_vec()),
        ))
        .unwrap();
    command
}

fn read_value(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    key: u64,
) -> Option<Vec<u8>> {
    store
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &key,
            PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap()
}

fn assert_io_kind(source: &(dyn Error + Send + Sync + 'static), expected: io::ErrorKind) {
    let source = source
        .downcast_ref::<io::Error>()
        .expect("deterministic fault source must remain an io::Error");
    assert_eq!(source.kind(), expected);
}

#[derive(Clone, Copy)]
enum ExpectedRecoveredState {
    Old,
    New,
    Either,
}

fn assert_recovered_state(
    store: &HomeStore,
    domain: beryl_home_store::DomainHandle<AlphaDomain>,
    expected: ExpectedRecoveredState,
) {
    let home_revision = store.home_revision().unwrap().get();
    let domain_revision = store.domain_revision(domain).unwrap().get();
    let value = read_value(store, domain, 41);
    let old = home_revision == 1 && domain_revision == 1 && value.is_none();
    let new = home_revision == 2
        && domain_revision == 2
        && value.as_deref() == Some(b"panic boundary".as_slice());
    match expected {
        ExpectedRecoveredState::Old => assert!(old),
        ExpectedRecoveredState::New => assert!(new),
        ExpectedRecoveredState::Either => assert!(old || new),
    }
}

#[test]
fn fixed_batch_limit_counts_application_and_package_owned_revision_records() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    let exact_application_records = 16_384 - 2;
    let mut exact = HomeCommand::new(store.home_revision().unwrap());
    exact
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutMany {
                count: exact_application_records,
            },
        ))
        .unwrap();
    let receipt = store.execute(exact).unwrap();
    assert_eq!(receipt.home_revision().get(), 2);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 2);

    let mut one_over = HomeCommand::new(store.home_revision().unwrap());
    one_over
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutMany {
                count: exact_application_records + 1,
            },
        ))
        .unwrap();
    assert!(matches!(
        store.execute(one_over),
        Err(CommandError::Commit { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert_eq!(store.home_revision().unwrap().get(), 2);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 2);
}

#[test]
fn owned_fjall_journal_write_failure_never_publishes_durable_success() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let generation = store.health().generation().unwrap();

    let fault = fjall::test_faults::fail_next_journal_write();
    let error = store
        .execute(put_command(&store, alpha, 42, b"must not publish"))
        .unwrap_err();
    drop(fault);
    assert!(matches!(error, CommandError::Commit { .. }));
    assert_eq!(store.health().state(), HomeHealthState::Verifying);

    assert!(store.verify_health().is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
    let recovery = store.recover_same_home().unwrap();
    assert_eq!(recovery.generation().get(), generation.get() + 1);
    let alpha = store.domain_handle::<AlphaDomain>().unwrap();
    assert_eq!(store.home_revision().unwrap().get(), 1);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 1);
    assert_eq!(read_value(&store, alpha, 42), None);
}

#[test]
fn validator_panic_fails_health_and_recovers_without_any_command_effect() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let generation_before = store.health().generation().unwrap();
    let home_before = store.home_revision().unwrap();
    let alpha_before = store.domain_revision(alpha).unwrap();
    let beta_before = store.domain_revision(beta).unwrap();
    let mut command = HomeCommand::new(home_before);
    command
        .add(alpha.contribution(
            alpha_before,
            PutBytes::<AlphaDomain>::new(40, b"must not commit".to_vec()),
        ))
        .unwrap()
        .add_validation(beta.validation(beta_before, PanicValidator))
        .unwrap();

    let panicked = catch_unwind(AssertUnwindSafe(|| {
        let _ = store.execute(command);
    }));
    assert!(panicked.is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    let recovery = store.recover_same_home().unwrap();
    assert_eq!(recovery.generation().get(), generation_before.get() + 1);
    let alpha = store.domain_handle::<AlphaDomain>().unwrap();
    let beta = store.domain_handle::<BetaDomain>().unwrap();
    assert_eq!(store.home_revision().unwrap(), home_before);
    assert_eq!(store.domain_revision(alpha).unwrap(), alpha_before);
    assert_eq!(store.domain_revision(beta).unwrap(), beta_before);
    assert_eq!(read_value(&store, alpha, 40), None);
}

#[test]
fn controlled_commit_boundary_panics_fail_closed_and_recover_old_or_new() {
    for (point, expected) in [
        (FaultPoint::BeforeCommit, ExpectedRecoveredState::Old),
        (
            FaultPoint::AfterCommitBeforePersist,
            ExpectedRecoveredState::Either,
        ),
        (FaultPoint::AfterPersist, ExpectedRecoveredState::New),
    ] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let mut store = open(directory.path(), faults.clone());
        let alpha = store.register_domain::<AlphaDomain>().unwrap();
        let original_generation = store.health().generation().unwrap();

        faults.panic_next(point);
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let _ = store.execute(put_command(&store, alpha, 41, b"panic boundary"));
        }));
        assert!(panicked.is_err());
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        assert!(matches!(
            store.home_revision(),
            Err(ReadError::HealthGate(error)) if error.state() == HomeHealthState::Failed
        ));

        let receipt = store.recover_same_home().unwrap();
        assert_eq!(receipt.generation().get(), original_generation.get() + 1);
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        let alpha = store.domain_handle::<AlphaDomain>().unwrap();
        assert_recovered_state(&store, alpha, expected);
    }
}

#[test]
fn exact_io_error_kinds_surface_at_the_commit_boundary() {
    for kind in [
        io::ErrorKind::StorageFull,
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::NotFound,
    ] {
        let directory = tempdir().unwrap();
        let faults = FaultController::new();
        let mut store = open(directory.path(), faults.clone());
        let alpha = store.register_domain::<AlphaDomain>().unwrap();

        faults.fail_next_with_kind(FaultPoint::BeforeCommit, kind);
        let error = store
            .execute(put_command(&store, alpha, 9, b"must not commit"))
            .unwrap_err();
        match error {
            CommandError::Commit { source } => assert_io_kind(source.as_ref(), kind),
            other => panic!("unexpected command error: {other:?}"),
        }
        assert_eq!(store.health().state(), HomeHealthState::Verifying);

        store.verify_health().unwrap();
        assert_eq!(store.health().state(), HomeHealthState::Healthy);
        assert_eq!(store.home_revision().unwrap().get(), 1);
        assert_eq!(read_value(&store, alpha, 9), None);
    }
}

#[test]
fn surfaced_post_sync_all_failure_preserves_the_durable_new_state() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let generation = store.health().generation().unwrap();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let error = store
        .execute(put_command(&store, alpha, 22, b"already durable"))
        .unwrap_err();
    match error {
        CommandError::Persistence { source } => {
            assert_io_kind(source.as_ref(), io::ErrorKind::StorageFull);
        }
        other => panic!("unexpected command error: {other:?}"),
    }
    assert_eq!(store.health().state(), HomeHealthState::Verifying);

    let health = store.verify_health().unwrap();
    assert_eq!(health.state(), HomeHealthState::Healthy);
    assert_eq!(health.generation(), Some(generation));
    assert_eq!(store.home_revision().unwrap().get(), 2);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 2);
    assert_eq!(
        read_value(&store, alpha, 22).as_deref(),
        Some(b"already durable".as_slice())
    );
}

#[test]
fn mixed_validator_commit_fault_advances_only_the_mutating_domain() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();

    let mut seed = HomeCommand::new(store.home_revision().unwrap());
    seed.add(beta.contribution(
        store.domain_revision(beta).unwrap(),
        PutBytes::<BetaDomain>::new(7, b"guarded".to_vec()),
    ))
    .unwrap();
    store.execute(seed).unwrap();
    let home_before = store.home_revision().unwrap();
    let alpha_before = store.domain_revision(alpha).unwrap();
    let beta_before = store.domain_revision(beta).unwrap();

    let mut command = HomeCommand::new(home_before);
    command
        .add(alpha.contribution(
            alpha_before,
            PutBytes::<AlphaDomain>::new(24, b"mixed durable".to_vec()),
        ))
        .unwrap()
        .add_validation(beta.validation(beta_before, RequireBeta))
        .unwrap();
    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    assert!(matches!(
        store.execute(command),
        Err(CommandError::Persistence { .. })
    ));
    assert_eq!(store.health().state(), HomeHealthState::Verifying);

    store.verify_health().unwrap();
    assert_eq!(store.home_revision().unwrap().get(), home_before.get() + 1);
    assert_eq!(
        store.domain_revision(alpha).unwrap().get(),
        alpha_before.get() + 1
    );
    assert_eq!(store.domain_revision(beta).unwrap(), beta_before);
    assert_eq!(
        read_value(&store, alpha, 24).as_deref(),
        Some(b"mixed durable".as_slice())
    );
}

#[test]
fn current_domain_command_shares_post_sync_durability_and_health_semantics() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let generation = store.health().generation().unwrap();

    faults.fail_next_with_kind(FaultPoint::AfterPersist, io::ErrorKind::StorageFull);
    let error = store
        .execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(
            23,
            b"current already durable".to_vec(),
        )))
        .unwrap_err();
    match error {
        CommandError::Persistence { source } => {
            assert_io_kind(source.as_ref(), io::ErrorKind::StorageFull);
        }
        other => panic!("unexpected current-domain command error: {other:?}"),
    }
    assert_eq!(store.health().state(), HomeHealthState::Verifying);

    let health = store.verify_health().unwrap();
    assert_eq!(health.state(), HomeHealthState::Healthy);
    assert_eq!(health.generation(), Some(generation));
    assert_eq!(store.home_revision().unwrap().get(), 2);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 2);
    assert_eq!(
        read_value(&store, alpha, 23).as_deref(),
        Some(b"current already durable".as_slice())
    );
}

#[test]
fn writer_panic_survives_persistent_recovery_faults_until_replacement_succeeds() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = open(directory.path(), faults.clone());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let original_generation = store.health().generation().unwrap();
    let mut poison_probe = Some(put_command(&store, alpha, 99, b"poison probe"));

    faults.panic_next(FaultPoint::BeforeCommit);
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        let _ = store.execute(put_command(&store, alpha, 1, b"panic"));
    }));
    assert!(panicked.is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);

    faults.fail_times_with_kind(FaultPoint::BeforeReopen, io::ErrorKind::PermissionDenied, 3);
    for _ in 0..3 {
        let error = store.recover_same_home().unwrap_err();
        match error {
            HomeRecoveryError::Layout { source } => {
                assert_io_kind(source.as_ref(), io::ErrorKind::PermissionDenied);
            }
            other => panic!("unexpected recovery error: {other:?}"),
        }
        assert_eq!(store.health().state(), HomeHealthState::Failed);
        if let Some(probe) = poison_probe.take() {
            assert!(matches!(
                store.execute(probe),
                Err(CommandError::WriterPoisoned)
            ));
        }
    }

    let receipt = store.recover_same_home().unwrap();
    assert_eq!(receipt.generation().get(), original_generation.get() + 1);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);

    let alpha = store.domain_handle::<AlphaDomain>().unwrap();
    store
        .execute(put_command(&store, alpha, 2, b"writer usable"))
        .unwrap();
    assert_eq!(
        read_value(&store, alpha, 2).as_deref(),
        Some(b"writer usable".as_slice())
    );
}
