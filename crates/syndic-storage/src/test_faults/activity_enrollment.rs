use crate::{
    SyndicMutationError, SyndicStorage, TurnRecord, TurnStateRecord, codec::*, domain::SyndicDomain,
};
use beryl_home_store::{
    DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::SyndicTurnId;

pub fn turn_alias_contribution_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    alias: SyndicTurnId,
    turn: TurnRecord,
    state: TurnStateRecord,
) -> MutationContribution {
    storage.handle.contribution(
        storage.revision(store).unwrap(),
        TurnAlias { alias, turn, state },
    )
}
struct TurnAlias {
    alias: SyndicTurnId,
    turn: TurnRecord,
    state: TurnStateRecord,
}
impl DomainMutation<SyndicDomain> for TurnAlias {
    type Error = SyndicMutationError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<TurnsCodec>(1)?;
        reservation.reserve_records::<TurnStatesCodec>(1)?;
        Ok(())
    }
    fn contribute(
        value: Self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<TurnsCodec>(&value.alias, &value.turn)?;
        mutations.put::<TurnStatesCodec>(&value.alias, &value.state)?;
        Ok(())
    }
}
