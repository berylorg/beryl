mod support;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use beryl_home_store::{
    CommandBuildError, CommandCancellation, CommandError, DomainHandle, DomainMutation,
    DomainReader, DomainValidator, HomeCommand, HomeStore, MutationBuilder, PointReadLimit,
    RevisionConflict,
};
use beryl_model::DomainRevision;
use tempfile::tempdir;

use support::{
    committed, not_committed, open_home, AlphaDomain, BetaDomain, BytesRecord,
    FixtureMutationError, PutBytes,
};

struct RequireBytes<D> {
    key: u64,
    expected: Vec<u8>,
    called: Option<Arc<AtomicBool>>,
    cancellation: Option<CommandCancellation>,
    _typed: std::marker::PhantomData<fn() -> D>,
}

impl<D> RequireBytes<D> {
    fn new(key: u64, expected: impl Into<Vec<u8>>) -> Self {
        Self {
            key,
            expected: expected.into(),
            called: None,
            cancellation: None,
            _typed: std::marker::PhantomData,
        }
    }

    fn tracking(mut self, called: Arc<AtomicBool>) -> Self {
        self.called = Some(called);
        self
    }

    fn cancelling(mut self, cancellation: CommandCancellation) -> Self {
        self.cancellation = Some(cancellation);
        self
    }
}

impl<D: beryl_home_store::StorageDomain> DomainValidator<D> for RequireBytes<D> {
    type Error = FixtureMutationError;

    fn validate(&self, reader: &DomainReader<'_, D>) -> Result<(), Self::Error> {
        if let Some(called) = &self.called {
            called.store(true, Ordering::SeqCst);
        }
        if let Some(cancellation) = &self.cancellation {
            cancellation.cancel();
        }
        let current = reader
            .point::<BytesRecord<D>>(&self.key, PointReadLimit::new(1_028).unwrap())
            .map_err(|_| FixtureMutationError::Rejected("validator read failed"))?;
        if current.as_deref() != Some(self.expected.as_slice()) {
            return Err(FixtureMutationError::Rejected(
                "validator observed an unexpected value",
            ));
        }
        Ok(())
    }
}

struct EmptyMutation;

impl DomainMutation<AlphaDomain> for EmptyMutation {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        _reader: &DomainReader<'_, AlphaDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BytesRecord<AlphaDomain>>(1)?;
        Ok(())
    }

    fn contribute(
        _prepared: Self::Prepared,
        _mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct ReentrantValidator {
    store: Arc<HomeStore>,
    domain: DomainHandle<BetaDomain>,
}

impl DomainValidator<BetaDomain> for ReentrantValidator {
    type Error = FixtureMutationError;

    fn validate(&self, _reader: &DomainReader<'_, BetaDomain>) -> Result<(), Self::Error> {
        let mut nested = HomeCommand::new(self.store.home_revision().unwrap());
        nested
            .add(self.domain.contribution(
                self.store.domain_revision(&self.domain).unwrap(),
                PutBytes::<BetaDomain>::new(99, b"nested".to_vec()),
            ))
            .unwrap();
        if !matches!(
            self.store.execute(nested),
            beryl_home_store::CommandOutcome::NotCommitted {
                evidence: CommandError::ReentrantWriter
            }
        ) {
            return Err(FixtureMutationError::Rejected(
                "validator nested writer did not reject reentry",
            ));
        }
        if !matches!(
            self.store.execute_current(
                self.domain
                    .current_command(PutBytes::<BetaDomain>::new(100, b"nested current".to_vec()))
            ),
            beryl_home_store::CommandOutcome::NotCommitted {
                evidence: CommandError::ReentrantWriter
            }
        ) {
            return Err(FixtureMutationError::Rejected(
                "validator nested current writer did not reject reentry",
            ));
        }
        Ok(())
    }
}

fn commit<D: beryl_home_store::StorageDomain>(
    store: &HomeStore,
    domain: &DomainHandle<D>,
    key: u64,
    value: impl Into<Vec<u8>>,
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
    committed(store.execute(command));
}

fn read<D: beryl_home_store::StorageDomain>(
    store: &HomeStore,
    domain: &DomainHandle<D>,
    key: u64,
) -> Option<Vec<u8>> {
    store
        .read_point::<D, BytesRecord<D>>(domain, &key, PointReadLimit::new(1_028).unwrap())
        .unwrap()
}

#[test]
fn mixed_validation_and_mutation_commit_only_mutating_revisions_and_reopen() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    commit(&store, &beta, 7, b"guarded");
    let home_before = store.home_revision().unwrap();
    let alpha_before = store.domain_revision(&alpha).unwrap();
    let beta_before = store.domain_revision(&beta).unwrap();

    let mut command = HomeCommand::new(home_before);
    command
        .add(alpha.contribution(
            alpha_before,
            PutBytes::<AlphaDomain>::new(1, b"committed".to_vec()),
        ))
        .unwrap()
        .add_validation(
            beta.validation(beta_before, RequireBytes::<BetaDomain>::new(7, b"guarded")),
        )
        .unwrap();

    let receipt = committed(store.execute(command));
    assert_eq!(receipt.home_revision().get(), home_before.get() + 1);
    assert_eq!(
        store.domain_revision(&alpha).unwrap().get(),
        alpha_before.get() + 1
    );
    assert_eq!(store.domain_revision(&beta).unwrap(), beta_before);
    assert_eq!(
        store.receipt_domain_revision(&receipt, &alpha).unwrap(),
        Some(DomainRevision::new(alpha_before.get() + 1).unwrap())
    );
    assert_eq!(
        store.receipt_domain_revision(&receipt, &beta).unwrap(),
        None
    );
    assert_eq!(read(&store, &alpha, 1), Some(b"committed".to_vec()));
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    let beta = reopened.register_domain::<BetaDomain>().unwrap();
    assert_eq!(reopened.home_revision().unwrap(), receipt.home_revision());
    assert_eq!(
        reopened.domain_revision(&alpha).unwrap(),
        DomainRevision::new(alpha_before.get() + 1).unwrap()
    );
    assert_eq!(reopened.domain_revision(&beta).unwrap(), beta_before);
    assert_eq!(read(&reopened, &alpha, 1), Some(b"committed".to_vec()));
}

#[test]
fn validation_only_command_is_rejected_without_running_its_callback() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let beta = store.register_domain::<BetaDomain>().unwrap();
    commit(&store, &beta, 7, b"guarded");
    let home_before = store.home_revision().unwrap();
    let beta_before = store.domain_revision(&beta).unwrap();
    let called = Arc::new(AtomicBool::new(false));
    let mut command = HomeCommand::new(home_before);
    command
        .add_validation(beta.validation(
            beta_before,
            RequireBytes::<BetaDomain>::new(7, b"guarded").tracking(Arc::clone(&called)),
        ))
        .unwrap();

    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::ValidationOnlyCommand
        }
    ));
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(store.home_revision().unwrap(), home_before);
    assert_eq!(store.domain_revision(&beta).unwrap(), beta_before);
}

#[test]
fn mutation_and_validation_roles_cannot_duplicate_one_domain_in_either_order() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let revision = store.domain_revision(&alpha).unwrap();

    let mut mutation_first = HomeCommand::new(store.home_revision().unwrap());
    mutation_first
        .add(alpha.contribution(
            revision,
            PutBytes::<AlphaDomain>::new(1, b"mutation".to_vec()),
        ))
        .unwrap();
    let duplicate = mutation_first.add_validation(
        alpha.validation(revision, RequireBytes::<AlphaDomain>::new(1, b"missing")),
    );
    assert!(matches!(
        duplicate,
        Err(CommandBuildError::DuplicateDomain { domain: "alpha" })
    ));

    let mut validation_first = HomeCommand::new(store.home_revision().unwrap());
    validation_first
        .add_validation(alpha.validation(revision, RequireBytes::<AlphaDomain>::new(1, b"missing")))
        .unwrap();
    let duplicate = validation_first.add(alpha.contribution(
        revision,
        PutBytes::<AlphaDomain>::new(1, b"mutation".to_vec()),
    ));
    assert!(matches!(
        duplicate,
        Err(CommandBuildError::DuplicateDomain { domain: "alpha" })
    ));
}

#[test]
fn stale_validator_revision_conflicts_before_any_mutation_callback() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    let stale_beta = store.domain_revision(&beta).unwrap();
    commit(&store, &beta, 7, b"current");
    let home_before = store.home_revision().unwrap();
    let alpha_before = store.domain_revision(&alpha).unwrap();

    let mut command = HomeCommand::new(home_before);
    command
        .add(alpha.contribution(
            alpha_before,
            PutBytes::<AlphaDomain>::new(1, b"must not commit".to_vec()),
        ))
        .unwrap()
        .add_validation(beta.validation(stale_beta, RequireBytes::<BetaDomain>::new(7, b"current")))
        .unwrap();

    let error = not_committed(store.execute(command));
    assert_eq!(
        error.conflicts().unwrap(),
        &[RevisionConflict::Domain {
            domain: "beta",
            expected: stale_beta,
            current: store.domain_revision(&beta).unwrap(),
        }]
    );
    assert_eq!(store.home_revision().unwrap(), home_before);
    assert_eq!(store.domain_revision(&alpha).unwrap(), alpha_before);
    assert_eq!(read(&store, &alpha, 1), None);
}

#[test]
fn validator_rejection_and_empty_mutation_each_abort_the_complete_command() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    commit(&store, &beta, 7, b"guarded");
    let home_before = store.home_revision().unwrap();
    let alpha_before = store.domain_revision(&alpha).unwrap();
    let beta_before = store.domain_revision(&beta).unwrap();

    let mut rejected = HomeCommand::new(home_before);
    rejected
        .add(alpha.contribution(
            alpha_before,
            PutBytes::<AlphaDomain>::new(1, b"must not commit".to_vec()),
        ))
        .unwrap()
        .add_validation(beta.validation(beta_before, RequireBytes::<BetaDomain>::new(7, b"wrong")))
        .unwrap();
    assert!(matches!(
        store.execute(rejected),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorValidation { domain: "beta", .. }
        }
    ));

    let mut empty = HomeCommand::new(home_before);
    empty
        .add(alpha.contribution(alpha_before, EmptyMutation))
        .unwrap()
        .add_validation(
            beta.validation(beta_before, RequireBytes::<BetaDomain>::new(7, b"guarded")),
        )
        .unwrap();
    assert!(matches!(
        store.execute(empty),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::EmptyContribution { domain: "alpha" }
        }
    ));

    assert_eq!(store.home_revision().unwrap(), home_before);
    assert_eq!(store.domain_revision(&alpha).unwrap(), alpha_before);
    assert_eq!(store.domain_revision(&beta).unwrap(), beta_before);
    assert_eq!(read(&store, &alpha, 1), None);
}

#[test]
fn validator_obeys_command_cancellation_and_reentry_boundaries() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();
    let beta = store.register_domain::<BetaDomain>().unwrap();
    commit(&store, &beta, 7, b"guarded");

    let called = Arc::new(AtomicBool::new(false));
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    let mut command =
        HomeCommand::new(store.home_revision().unwrap()).with_cancellation(cancelled.clone());
    command
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(1, b"cancelled".to_vec()),
        ))
        .unwrap()
        .add_validation(beta.validation(
            store.domain_revision(&beta).unwrap(),
            RequireBytes::<BetaDomain>::new(7, b"guarded").tracking(Arc::clone(&called)),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission
        }
    ));
    assert!(!called.load(Ordering::SeqCst));

    let admitted_cancellation = CommandCancellation::new();
    let mut admitted = HomeCommand::new(store.home_revision().unwrap())
        .with_cancellation(admitted_cancellation.clone());
    admitted
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(2, b"admitted".to_vec()),
        ))
        .unwrap()
        .add_validation(
            beta.validation(
                store.domain_revision(&beta).unwrap(),
                RequireBytes::<BetaDomain>::new(7, b"guarded")
                    .cancelling(admitted_cancellation.clone()),
            ),
        )
        .unwrap();
    committed(store.execute(admitted));
    assert!(admitted_cancellation.is_cancelled());
    assert_eq!(read(&store, &alpha, 2), Some(b"admitted".to_vec()));

    let store = Arc::new(store);
    let mut reentrant = HomeCommand::new(store.home_revision().unwrap());
    reentrant
        .add(alpha.contribution(
            store.domain_revision(&alpha).unwrap(),
            PutBytes::<AlphaDomain>::new(3, b"outer".to_vec()),
        ))
        .unwrap()
        .add_validation(beta.validation(
            store.domain_revision(&beta).unwrap(),
            ReentrantValidator {
                store: Arc::clone(&store),
                domain: beta.clone(),
            },
        ))
        .unwrap();
    committed(store.execute(reentrant));
    assert_eq!(read(&store, &alpha, 3), Some(b"outer".to_vec()));
    assert_eq!(read(&store, &beta, 99), None);
    assert_eq!(read(&store, &beta, 100), None);
}
