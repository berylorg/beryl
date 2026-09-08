use crate::test_faults::FixtureMutationError;
use crate::{
    SyndicStorage,
    codec::{ExactCodec, Family},
    domain::SyndicDomain,
};
use beryl_home_store::{
    CommandOutcome, DomainMutation, DomainReader, HomeCommand, HomeStore, MutationBuilder,
    ReconciliationReservation,
};

struct RecordFixture<F: Family> {
    key: F::Key,
    value: F::Value,
}

impl<F: Family> DomainMutation<SyndicDomain> for RecordFixture<F> {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ExactCodec<F>>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        builder.put::<ExactCodec<F>>(&prepared.key, &prepared.value)?;
        Ok(())
    }
}

pub(crate) fn put_marker_bounds_fixture_record<F: Family>(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: &F::Key,
    value: &F::Value,
) {
    let contribution = storage.handle.contribution(
        storage.revision(store).unwrap(),
        RecordFixture::<F> {
            key: key.clone(),
            value: value.clone(),
        },
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
}
