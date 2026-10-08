mod support;

#[path = "current_home_commands/domains.rs"]
mod domains;
use domains::{AlphaDomain, BetaDomain};

#[cfg(feature = "test-faults")]
#[path = "current_home_commands/faults.rs"]
mod faults;

use beryl_home_store::{
    CommandBuildError, CommandCancellation, CommandError, CurrentHomeCommand, DomainHandle,
    DomainMutation, DomainReader, HomeDomainRequirements, HomeOpenCandidate, HomeStore,
    MutationBuilder, PointReadLimit, ReconciliationReservation,
};
use tempfile::tempdir;

use support::{BytesRecord, FixtureMutationError, PutBytes, committed, not_committed, open_home};

struct Fixture {
    store: HomeStore,
    alpha: DomainHandle<AlphaDomain>,
    beta: DomainHandle<BetaDomain>,
}

impl Fixture {
    fn publish(mut candidate: HomeOpenCandidate) -> Self {
        let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
        let beta = candidate.register_domain::<BetaDomain>().unwrap();
        let store = candidate
            .prepare_publication(
                HomeDomainRequirements::new()
                    .with_domain::<AlphaDomain>()
                    .unwrap()
                    .with_domain::<BetaDomain>()
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        Self { store, alpha, beta }
    }

    fn command(&self) -> CurrentHomeCommand {
        let mut command =
            CurrentHomeCommand::new(
                self.alpha
                    .current_command(PutBytes::<AlphaDomain>::new(1, b"alpha".to_vec())),
            );
        command
            .add(
                self.beta
                    .current_command(PutBytes::<BetaDomain>::new(2, b"beta".to_vec())),
            )
            .unwrap();
        command
    }
}

fn read<D: beryl_home_store::StorageDomain>(
    store: &HomeStore,
    handle: &DomainHandle<D>,
    key: u64,
) -> Option<Vec<u8>> {
    store
        .read_point::<D, BytesRecord<D>>(handle, &key, PointReadLimit::new(1_028).unwrap())
        .unwrap()
}

struct PutIfMissing {
    key: u64,
}

struct EmptySecondary;

impl DomainMutation<BetaDomain> for EmptySecondary {
    type Error = FixtureMutationError;
    type Prepared = ();

    fn prepare(self, _: &DomainReader<'_, BetaDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        _: &mut ReconciliationReservation<'_, BetaDomain>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn contribute(_: (), _: &mut MutationBuilder<'_, BetaDomain>) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl DomainMutation<AlphaDomain> for PutIfMissing {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, reader: &DomainReader<'_, AlphaDomain>) -> Result<Self, Self::Error> {
        if reader
            .point::<BytesRecord<AlphaDomain>>(&self.key, PointReadLimit::new(1_028).unwrap())
            .map_err(|_| FixtureMutationError::Rejected("logical source read failed"))?
            .is_some()
        {
            return Err(FixtureMutationError::Rejected(
                "logical source no longer absent",
            ));
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<BytesRecord<AlphaDomain>>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, AlphaDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<BytesRecord<AlphaDomain>>(&prepared.key, &b"new".to_vec())?;
        Ok(())
    }
}

#[test]
fn joined_current_command_captures_later_revisions_and_reopens_both_effects() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::publish(open_home(directory.path()));
    let command = fixture.command();
    committed(
        fixture.store.execute_current(
            fixture
                .alpha
                .current_command(PutBytes::<AlphaDomain>::new(3, b"intervening".to_vec())),
        ),
    );
    let receipt = committed(fixture.store.execute_current_home(command));
    assert_eq!(receipt.home_revision().get(), 3);
    assert_eq!(
        fixture
            .store
            .receipt_domain_revision(&receipt, &fixture.alpha)
            .unwrap()
            .unwrap()
            .get(),
        3
    );
    assert_eq!(
        fixture
            .store
            .receipt_domain_revision(&receipt, &fixture.beta)
            .unwrap()
            .unwrap()
            .get(),
        2
    );
    fixture.store.close().unwrap();
    let reopened = Fixture::publish(open_home(directory.path()));
    assert_eq!(
        read(&reopened.store, &reopened.alpha, 1),
        Some(b"alpha".to_vec())
    );
    assert_eq!(
        read(&reopened.store, &reopened.beta, 2),
        Some(b"beta".to_vec())
    );
    assert_eq!(
        read(&reopened.store, &reopened.alpha, 3),
        Some(b"intervening".to_vec())
    );
}

#[test]
fn exact_logical_source_rejection_preserves_both_domains() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::publish(open_home(directory.path()));
    let mut command =
        CurrentHomeCommand::new(fixture.alpha.current_command(PutIfMissing { key: 1 }));
    command
        .add(
            fixture
                .beta
                .current_command(PutBytes::<BetaDomain>::new(2, b"beta".to_vec())),
        )
        .unwrap();
    committed(
        fixture.store.execute_current(
            fixture
                .alpha
                .current_command(PutBytes::<AlphaDomain>::new(1, b"existing".to_vec())),
        ),
    );
    let revision = fixture.store.home_revision().unwrap();
    assert!(matches!(
        not_committed(fixture.store.execute_current_home(command)),
        CommandError::ContributorValidation { .. } | CommandError::ContributorAssembly { .. }
    ));
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(
        read(&fixture.store, &fixture.alpha, 1),
        Some(b"existing".to_vec())
    );
    assert_eq!(read(&fixture.store, &fixture.beta, 2), None);
}

#[test]
fn secondary_preparation_and_contribution_rejections_are_atomic() {
    for reject_assembly in [false, true] {
        let directory = tempdir().unwrap();
        let fixture = Fixture::publish(open_home(directory.path()));
        let mut command = CurrentHomeCommand::new(fixture.alpha.current_command(PutBytes::<
            AlphaDomain,
        >::new(
            1,
            b"alpha".to_vec(),
        )));
        let beta = PutBytes::<BetaDomain>::new(2, b"beta".to_vec());
        command
            .add(fixture.beta.current_command(if reject_assembly {
                beta.rejecting_assembly()
            } else {
                beta.rejecting_validation()
            }))
            .unwrap();
        let before = fixture.store.home_revision().unwrap();
        assert!(matches!(
            not_committed(fixture.store.execute_current_home(command)),
            CommandError::ContributorValidation { .. } | CommandError::ContributorAssembly { .. }
        ));
        assert_eq!(fixture.store.home_revision().unwrap(), before);
        assert_eq!(read(&fixture.store, &fixture.alpha, 1), None);
        assert_eq!(read(&fixture.store, &fixture.beta, 2), None);
    }
}

#[test]
fn empty_secondary_rejects_without_publishing_primary() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::publish(open_home(directory.path()));
    let mut command = CurrentHomeCommand::new(
        fixture
            .alpha
            .current_command(PutBytes::<AlphaDomain>::new(1, b"primary".to_vec())),
    );
    command
        .add(fixture.beta.current_command(EmptySecondary))
        .unwrap();
    assert!(matches!(
        not_committed(fixture.store.execute_current_home(command)),
        CommandError::EmptyContribution { .. }
    ));
    assert_eq!(read(&fixture.store, &fixture.alpha, 1), None);
    assert_eq!(read(&fixture.store, &fixture.beta, 2), None);
}

#[test]
fn duplicate_domain_addition_does_not_change_original_command() {
    let directory = tempdir().unwrap();
    let fixture = Fixture::publish(open_home(directory.path()));
    let mut command = fixture.command();
    assert_eq!(
        command
            .add(
                fixture
                    .alpha
                    .current_command(PutBytes::<AlphaDomain>::new(9, b"duplicate".to_vec()))
            )
            .unwrap_err(),
        CommandBuildError::DuplicateDomain {
            domain: "current_alpha"
        }
    );
    committed(fixture.store.execute_current_home(command));
    assert_eq!(read(&fixture.store, &fixture.alpha, 9), None);
    assert_eq!(
        read(&fixture.store, &fixture.beta, 2),
        Some(b"beta".to_vec())
    );
}

#[test]
fn foreign_participant_cannot_publish_local_effect() {
    let directory = tempdir().unwrap();
    let other_directory = tempdir().unwrap();
    let fixture = Fixture::publish(open_home(directory.path()));
    let foreign = Fixture::publish(open_home(other_directory.path()));
    let mut command = CurrentHomeCommand::new(
        fixture
            .alpha
            .current_command(PutBytes::<AlphaDomain>::new(1, b"alpha".to_vec())),
    );
    command
        .add(
            foreign
                .beta
                .current_command(PutBytes::<BetaDomain>::new(2, b"beta".to_vec())),
        )
        .unwrap();
    assert!(matches!(
        not_committed(fixture.store.execute_current_home(command)),
        CommandError::ForeignDomain { .. }
    ));
    assert_eq!(read(&fixture.store, &fixture.alpha, 1), None);
    assert_eq!(read(&foreign.store, &foreign.beta, 2), None);
}

#[test]
fn every_retained_cancellation_signal_remains_effective() {
    for cancelled_lane in 0..3 {
        let directory = tempdir().unwrap();
        let fixture = Fixture::publish(open_home(directory.path()));
        let signals = [
            CommandCancellation::new(),
            CommandCancellation::new(),
            CommandCancellation::new(),
        ];
        let mut command = CurrentHomeCommand::new(
            fixture
                .alpha
                .current_command(PutBytes::<AlphaDomain>::new(1, b"alpha".to_vec()))
                .with_cancellation(signals[0].clone()),
        )
        .with_cancellation(signals[2].clone());
        command
            .add(
                fixture
                    .beta
                    .current_command(PutBytes::<BetaDomain>::new(2, b"beta".to_vec()))
                    .with_cancellation(signals[1].clone()),
            )
            .unwrap();
        signals[cancelled_lane].cancel();
        assert!(matches!(
            not_committed(fixture.store.execute_current_home(command)),
            CommandError::CancelledBeforeAdmission
        ));
        assert_eq!(read(&fixture.store, &fixture.alpha, 1), None);
        assert_eq!(read(&fixture.store, &fixture.beta, 2), None);
    }
}
