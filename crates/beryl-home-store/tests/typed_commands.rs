mod support;

#[cfg(feature = "test-faults")]
use std::thread;
use std::{path::PathBuf, process::Command, sync::Arc};

use beryl_home_store::{
    CommandCancellation, CommandError, CommitReceiptError, DomainHandle, DomainMutation,
    DomainReader, HomeCommand, HomeStore, MutationBuilder, PointReadLimit, RevisionConflict,
};
#[cfg(feature = "test-faults")]
use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::DomainRevision;
use tempfile::tempdir;

use support::{AlphaDomain, BetaDomain, BytesRecord, FixtureMutationError, PutBytes, open_home};

#[test]
fn one_cross_domain_batch_advances_all_revisions_and_reopens_wholly() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"alpha".to_vec()),
        ))
        .unwrap()
        .add(beta.contribution(
            store.domain_revision(beta).unwrap(),
            PutBytes::<BetaDomain>::new(2, b"beta".to_vec()),
        ))
        .unwrap();

    let receipt = store.execute(command).unwrap();
    assert_eq!(receipt.home_revision().get(), 2);
    assert_eq!(receipt.generation(), store.health().generation().unwrap());
    assert_eq!(
        store
            .receipt_domain_revision(&receipt, alpha)
            .unwrap()
            .unwrap()
            .get(),
        2
    );
    assert_eq!(
        store
            .receipt_domain_revision(&receipt, beta)
            .unwrap()
            .unwrap()
            .get(),
        2
    );
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    let beta = reopened.register_domain::<BetaDomain>().unwrap();
    assert_eq!(reopened.home_revision().unwrap().get(), 2);
    assert_eq!(reopened.domain_revision(alpha).unwrap().get(), 2);
    assert_eq!(reopened.domain_revision(beta).unwrap().get(), 2);
    assert_eq!(read(&reopened, alpha, 1), Some(b"alpha".to_vec()));
    assert_eq!(read(&reopened, beta, 2), Some(b"beta".to_vec()));
}

#[test]
fn receipt_reports_only_affected_domains_in_its_exact_generation() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"alpha".to_vec()),
        ))
        .unwrap();

    let receipt = store.execute(command).unwrap();
    assert_eq!(receipt.generation(), store.health().generation().unwrap());
    let debug = format!("{receipt:?}");
    assert!(debug.contains("affected_domain_count: 1"));
    assert!(!debug.contains("store:"));
    assert!(!debug.contains("domains:"));
    assert!(!debug.contains("DomainRevision"));
    assert_eq!(
        store.receipt_domain_revision(&receipt, alpha).unwrap(),
        Some(DomainRevision::new(2).unwrap())
    );
    assert_eq!(store.receipt_domain_revision(&receipt, beta).unwrap(), None);
}

#[test]
fn receipt_rejects_another_home_and_another_registration() {
    let first_directory = tempdir().unwrap();
    let second_directory = tempdir().unwrap();
    let mut first = open_home(first_directory.path());
    let mut second = open_home(second_directory.path());
    let first_alpha = first.register_domain::<AlphaDomain>().unwrap();
    let second_alpha = second.register_domain::<AlphaDomain>().unwrap();
    let mut command = HomeCommand::new(first.home_revision().unwrap());
    command
        .add(first_alpha.contribution(
            first.domain_revision(first_alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"first home".to_vec()),
        ))
        .unwrap();
    let receipt = first.execute(command).unwrap();

    assert!(matches!(
        second.receipt_domain_revision(&receipt, second_alpha),
        Err(CommitReceiptError::StaleOrForeign { .. })
    ));
    assert!(matches!(
        first.receipt_domain_revision(&receipt, second_alpha),
        Err(CommitReceiptError::ForeignDomain { domain: "alpha" })
    ));
}

#[test]
fn later_validation_or_assembly_failure_commits_nothing() {
    for reject_assembly in [false, true] {
        let directory = tempdir().unwrap();
        let mut store = open_home(directory.path());
        let alpha = store.register_domain::<AlphaDomain>().unwrap();
        let beta = store.register_domain::<BetaDomain>().unwrap();
        let mut rejected = PutBytes::<BetaDomain>::new(2, b"beta".to_vec());
        if reject_assembly {
            rejected = rejected.rejecting_assembly();
        } else {
            rejected = rejected.rejecting_validation();
        }
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(alpha.contribution(
                store.domain_revision(alpha).unwrap(),
                PutBytes::<AlphaDomain>::new(1, b"alpha".to_vec()),
            ))
            .unwrap()
            .add(beta.contribution(store.domain_revision(beta).unwrap(), rejected))
            .unwrap();

        let error = store.execute(command).unwrap_err();
        if reject_assembly {
            assert!(matches!(
                error,
                CommandError::ContributorAssembly { domain: "beta", .. }
            ));
        } else {
            assert!(matches!(
                error,
                CommandError::ContributorValidation { domain: "beta", .. }
            ));
        }
        assert_eq!(store.home_revision().unwrap().get(), 1);
        assert_eq!(store.domain_revision(alpha).unwrap().get(), 1);
        assert_eq!(store.domain_revision(beta).unwrap().get(), 1);
        assert_eq!(read(&store, alpha, 1), None);
        assert_eq!(read(&store, beta, 2), None);
    }
}

#[test]
fn stale_conflicts_are_home_first_then_domain_name_order() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    commit_one(&store, alpha, 1, b"first".to_vec());

    let mut stale = HomeCommand::new(beryl_model::HomeRevision::new(1).unwrap());
    stale
        .add(beta.contribution(
            DomainRevision::new(9).unwrap(),
            PutBytes::<BetaDomain>::new(2, b"beta".to_vec()),
        ))
        .unwrap()
        .add(alpha.contribution(
            DomainRevision::new(1).unwrap(),
            PutBytes::<AlphaDomain>::new(3, b"alpha".to_vec()),
        ))
        .unwrap();

    let error = store.execute(stale).unwrap_err();
    assert_eq!(
        error.conflicts().unwrap(),
        &[
            RevisionConflict::Home {
                expected: beryl_model::HomeRevision::new(1).unwrap(),
                current: beryl_model::HomeRevision::new(2).unwrap(),
            },
            RevisionConflict::Domain {
                domain: "alpha",
                expected: DomainRevision::new(1).unwrap(),
                current: DomainRevision::new(2).unwrap(),
            },
            RevisionConflict::Domain {
                domain: "beta",
                expected: DomainRevision::new(9).unwrap(),
                current: DomainRevision::new(1).unwrap(),
            },
        ]
    );
}

#[test]
fn cancellation_before_admission_aborts_but_cancellation_after_admission_does_not() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    let mut command =
        HomeCommand::new(store.home_revision().unwrap()).with_cancellation(cancelled.clone());
    command
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"never".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        Err(CommandError::CancelledBeforeAdmission)
    ));
    assert_eq!(read(&store, alpha, 1), None);

    let after_admission = CommandCancellation::new();
    let mut admitted =
        HomeCommand::new(store.home_revision().unwrap()).with_cancellation(after_admission.clone());
    admitted
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            CancelDuringValidation {
                cancellation: after_admission.clone(),
                key: 2,
            },
        ))
        .unwrap();
    store.execute(admitted).unwrap();
    assert!(after_admission.is_cancelled());
    assert_eq!(read(&store, alpha, 2), Some(b"admitted".to_vec()));

    let cancelled_current = CommandCancellation::new();
    cancelled_current.cancel();
    let current = alpha
        .current_command(PutBytes::<AlphaDomain>::new(3, b"never current".to_vec()))
        .with_cancellation(cancelled_current);
    assert!(matches!(
        store.execute_current(current),
        Err(CommandError::CancelledBeforeAdmission)
    ));
    assert_eq!(read(&store, alpha, 3), None);

    let current_after_admission = CommandCancellation::new();
    let current = alpha
        .current_command(CancelDuringValidation {
            cancellation: current_after_admission.clone(),
            key: 4,
        })
        .with_cancellation(current_after_admission.clone());
    store.execute_current(current).unwrap();
    assert!(current_after_admission.is_cancelled());
    assert_eq!(read(&store, alpha, 4), Some(b"admitted".to_vec()));
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
        second.join().unwrap().unwrap();
    });

    assert_eq!(store.home_revision().unwrap().get(), 3);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 3);
    assert_eq!(read(&store, alpha, 1), Some(b"first".to_vec()));
    assert_eq!(read(&store, alpha, 2), Some(b"second".to_vec()));
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

    let error = store.execute_current(command).unwrap_err();
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

#[test]
fn same_thread_writer_reentrancy_is_rejected_without_deadlock() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let store = Arc::new(store);
    let mut outer = HomeCommand::new(store.home_revision().unwrap());
    outer
        .add(alpha.contribution(
            store.domain_revision(alpha).unwrap(),
            ReentrantProbe {
                store: Arc::clone(&store),
                domain: alpha,
                home_revision: store.home_revision().unwrap(),
                domain_revision: store.domain_revision(alpha).unwrap(),
            },
        ))
        .unwrap();

    store.execute(outer).unwrap();
    assert_eq!(read(&store, alpha, 7), Some(b"outer".to_vec()));
    assert_eq!(read(&store, alpha, 8), None);
    assert_eq!(read(&store, alpha, 9), None);
}

#[test]
fn empty_duplicate_and_foreign_commands_are_rejected_before_mutation() {
    let first_directory = tempdir().unwrap();
    let second_directory = tempdir().unwrap();
    let mut first = open_home(first_directory.path());
    let mut second = open_home(second_directory.path());
    let alpha = first.register_domain::<AlphaDomain>().unwrap();
    second.register_domain::<AlphaDomain>().unwrap();

    assert!(matches!(
        first.execute(HomeCommand::new(first.home_revision().unwrap())),
        Err(CommandError::EmptyCommand)
    ));

    let mut duplicate = HomeCommand::new(first.home_revision().unwrap());
    duplicate
        .add(alpha.contribution(
            first.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"one".to_vec()),
        ))
        .unwrap();
    assert!(
        duplicate
            .add(alpha.contribution(
                first.domain_revision(alpha).unwrap(),
                PutBytes::<AlphaDomain>::new(2, b"two".to_vec()),
            ))
            .is_err()
    );

    let mut foreign = HomeCommand::new(second.home_revision().unwrap());
    foreign
        .add(alpha.contribution(
            first.domain_revision(alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(3, b"foreign".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        second.execute(foreign),
        Err(CommandError::ForeignDomain { domain: "alpha" })
    ));
}

#[test]
fn durable_success_survives_immediate_process_abort() {
    let directory = tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "phase4_abort_after_durable_success_helper",
            "--nocapture",
        ])
        .env("BERYL_PHASE4_ABORT_HOME", directory.path())
        .status()
        .unwrap();
    assert!(!status.success());

    let mut reopened = open_home(directory.path());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(reopened.home_revision().unwrap().get(), 2);
    assert_eq!(read(&reopened, alpha, 44), Some(b"durable".to_vec()));
}

#[test]
fn phase4_abort_after_durable_success_helper() {
    let Some(path) = std::env::var_os("BERYL_PHASE4_ABORT_HOME").map(PathBuf::from) else {
        return;
    };
    let mut store = open_home(&path);
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    commit_one(&store, alpha, 44, b"durable".to_vec());
    std::process::abort();
}

struct CancelDuringValidation {
    cancellation: CommandCancellation,
    key: u64,
}

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

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AlphaDomain>,
        mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BytesRecord<AlphaDomain>>(&self.key, &self.value)?;
        Ok(())
    }
}

impl DomainMutation<AlphaDomain> for CancelDuringValidation {
    type Error = FixtureMutationError;

    fn validate(&self, _reader: &DomainReader<'_, AlphaDomain>) -> Result<(), Self::Error> {
        self.cancellation.cancel();
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AlphaDomain>,
        mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BytesRecord<AlphaDomain>>(&self.key, &b"admitted".to_vec())?;
        Ok(())
    }
}

struct ReentrantProbe {
    store: Arc<HomeStore>,
    domain: DomainHandle<AlphaDomain>,
    home_revision: beryl_model::HomeRevision,
    domain_revision: DomainRevision,
}

impl DomainMutation<AlphaDomain> for ReentrantProbe {
    type Error = FixtureMutationError;

    fn validate(&self, _reader: &DomainReader<'_, AlphaDomain>) -> Result<(), Self::Error> {
        let mut nested = HomeCommand::new(self.home_revision);
        nested
            .add(self.domain.contribution(
                self.domain_revision,
                PutBytes::<AlphaDomain>::new(8, b"inner".to_vec()),
            ))
            .unwrap();
        if !matches!(
            self.store.execute(nested),
            Err(CommandError::ReentrantWriter)
        ) {
            return Err(FixtureMutationError::Rejected(
                "nested writer did not reject reentrancy",
            ));
        }
        if !matches!(
            self.store.execute_current(
                self.domain
                    .current_command(PutBytes::<AlphaDomain>::new(9, b"current inner".to_vec()))
            ),
            Err(CommandError::ReentrantWriter)
        ) {
            return Err(FixtureMutationError::Rejected(
                "nested current writer did not reject reentrancy",
            ));
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, AlphaDomain>,
        mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BytesRecord<AlphaDomain>>(&7, &b"outer".to_vec())?;
        Ok(())
    }
}

fn commit_one<D: beryl_home_store::StorageDomain>(
    store: &HomeStore,
    domain: DomainHandle<D>,
    key: u64,
    value: Vec<u8>,
) where
    PutBytes<D>: DomainMutation<D>,
{
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<D>::new(key, value),
        ))
        .unwrap();
    store.execute(command).unwrap();
}

fn read<D: beryl_home_store::StorageDomain>(
    store: &HomeStore,
    domain: DomainHandle<D>,
    key: u64,
) -> Option<Vec<u8>> {
    store
        .read_point::<D, BytesRecord<D>>(domain, &key, PointReadLimit::new(1_028).unwrap())
        .unwrap()
}
