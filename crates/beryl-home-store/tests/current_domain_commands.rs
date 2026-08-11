mod support;

#[cfg(feature = "test-faults")]
use std::{sync::Arc, thread};

#[cfg(feature = "test-faults")]
use beryl_home_store::{
    test_faults::{FaultController, FaultPoint, FaultScope},
    HomeOpenOptions, HomeSchemaVersion,
};
use beryl_home_store::{
    CommandError, DomainHandle, DomainMutation, DomainReader, HomeCommand, HomeStore,
    MutationBuilder, PointReadLimit,
};
use tempfile::tempdir;

use support::{
    committed, not_committed, open_home, AlphaDomain, BytesRecord, FixtureMutationError, PutBytes,
};

struct PutIfMissing {
    key: u64,
    value: Vec<u8>,
}

impl DomainMutation<AlphaDomain> for PutIfMissing {
    type Error = FixtureMutationError;

    fn validate(&self, reader: &DomainReader<'_, AlphaDomain>) -> Result<(), Self::Error> {
        let current = reader
            .point::<BytesRecord<AlphaDomain>>(&self.key, PointReadLimit::new(1_028).unwrap())
            .map_err(|_| FixtureMutationError::Rejected("logical validation read failed"))?;
        if current.is_some() {
            return Err(FixtureMutationError::Rejected(
                "logical record is no longer absent",
            ));
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BytesRecord<AlphaDomain>>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AlphaDomain>,
        mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BytesRecord<AlphaDomain>>(&self.key, &self.value)?;
        Ok(())
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn current_domain_command_captures_physical_revisions_after_writer_admission() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let store = Arc::new(store);
    let first_cut = faults.block_next(FaultPoint::BeforeCommit);

    thread::scope(|scope| {
        let first_store = Arc::clone(&store);
        let first = scope.spawn(move || {
            commit_one(&first_store, alpha, 1, b"first".to_vec());
        });
        assert!(first_cut.wait_until_reached(std::time::Duration::from_secs(10)));

        let second_store = Arc::clone(&store);
        let second = scope.spawn(move || {
            second_store.execute_current(
                alpha.current_command(PutBytes::<AlphaDomain>::new(2, b"second".to_vec())),
            )
        });
        thread::sleep(std::time::Duration::from_millis(50));
        assert!(!second.is_finished());
        first_cut.release();
        first.join().unwrap();
        committed(second.join().unwrap());
    });

    assert_eq!(store.home_revision().unwrap().get(), 3);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 3);
    assert_eq!(read(&store, alpha, 1), Some(b"first".to_vec()));
    assert_eq!(read(&store, alpha, 2), Some(b"second".to_vec()));
}

#[cfg(feature = "test-faults")]
#[test]
fn scoped_writer_fault_ignores_other_typed_current_commands() {
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    faults.fail_next_in_scope(FaultPoint::BeforeCommit, FaultScope::of::<PutIfMissing>());

    committed(
        store.execute_current(alpha.current_command(PutBytes::<AlphaDomain>::new(
            1,
            b"different mutation".to_vec(),
        ))),
    );
    let error = not_committed(store.execute_current(alpha.current_command(PutIfMissing {
        key: 2,
        value: b"target mutation".to_vec(),
    })));
    assert!(matches!(error, CommandError::Commit { .. }));

    store.verify_health().unwrap();
    assert_eq!(read(&store, alpha, 1), Some(b"different mutation".to_vec()));
    assert_eq!(read(&store, alpha, 2), None);
}

#[test]
fn current_domain_command_preserves_exact_logical_validation() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let command = alpha.current_command(PutIfMissing {
        key: 4,
        value: b"stale".to_vec(),
    });
    commit_one(&store, alpha, 4, b"current".to_vec());

    let error = not_committed(store.execute_current(command));
    assert!(matches!(
        error,
        CommandError::ContributorValidation {
            domain: "alpha",
            ..
        }
    ));
    assert_eq!(store.home_revision().unwrap().get(), 2);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 2);
    assert_eq!(read(&store, alpha, 4), Some(b"current".to_vec()));
}

fn commit_one(store: &HomeStore, domain: DomainHandle<AlphaDomain>, key: u64, value: Vec<u8>) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<AlphaDomain>::new(key, value),
        ))
        .unwrap();
    committed(store.execute(command));
}

fn read(store: &HomeStore, domain: DomainHandle<AlphaDomain>, key: u64) -> Option<Vec<u8>> {
    store
        .read_point::<AlphaDomain, BytesRecord<AlphaDomain>>(
            domain,
            &key,
            PointReadLimit::new(1_028).unwrap(),
        )
        .unwrap()
}
