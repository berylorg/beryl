use std::{
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use beryl_home_store::{
    CommandCancellation, CommandPairRequirement, CommandParticipantKind, DomainHandle,
    DomainMutation, DomainReader, DomainValidator, HomeCommand, HomeDomainRequirements,
    HomeOpenCandidate, HomeStore, MutationBuilder, PointReadLimit, ReconciliationReservation,
    StorageDomain,
};

use super::{
    domains::{AlphaDomain, BetaDomain},
    support::{self, BytesRecord, FixtureMutationError, PutBytes, open_home},
};

#[derive(Default)]
pub(super) struct Calls {
    pub sealed: AtomicUsize,
    pub reserved: AtomicUsize,
    pub prepared: AtomicUsize,
    pub assembled: AtomicUsize,
}

pub(super) struct PairMutation<D> {
    pub put: PutBytes<D>,
    pub requirement: Option<CommandPairRequirement>,
    pub calls: Arc<Calls>,
    pub cancellation: Option<CommandCancellation>,
}

impl<D: StorageDomain> DomainMutation<D> for PairMutation<D> {
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn command_pair(&self) -> Option<CommandPairRequirement> {
        self.calls.sealed.fetch_add(1, Ordering::SeqCst);
        self.requirement
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, D>,
    ) -> Result<(), Self::Error> {
        self.calls.reserved.fetch_add(1, Ordering::SeqCst);
        self.put.reserve_reconciliation(reservation)
    }

    fn prepare(mut self, reader: &DomainReader<'_, D>) -> Result<Self, Self::Error> {
        self.calls.prepared.fetch_add(1, Ordering::SeqCst);
        if let Some(cancellation) = &self.cancellation {
            cancellation.cancel();
        }
        self.put = self.put.prepare(reader)?;
        Ok(self)
    }

    fn contribute(
        prepared: Self,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        prepared.calls.assembled.fetch_add(1, Ordering::SeqCst);
        PutBytes::<D>::contribute(prepared.put, mutations)
    }
}

pub(super) struct PairValidator<D> {
    pub requirement: Option<CommandPairRequirement>,
    pub calls: Arc<Calls>,
    pub reject: bool,
    pub typed: PhantomData<fn() -> D>,
}

impl<D: StorageDomain> DomainValidator<D> for PairValidator<D> {
    type Error = FixtureMutationError;

    fn command_pair(&self) -> Option<CommandPairRequirement> {
        self.calls.sealed.fetch_add(1, Ordering::SeqCst);
        self.requirement
    }

    fn validate(&self, reader: &DomainReader<'_, D>) -> Result<(), Self::Error> {
        self.calls.prepared.fetch_add(1, Ordering::SeqCst);
        let value = reader
            .point::<BytesRecord<D>>(&1, PointReadLimit::new(1_028).unwrap())
            .map_err(|_| FixtureMutationError::Rejected("peer read failed"))?;
        if self.reject || value.is_some() {
            return Err(FixtureMutationError::Rejected("peer state refused"));
        }
        Ok(())
    }
}

pub(super) struct Fixture {
    pub store: HomeStore,
    pub alpha: DomainHandle<AlphaDomain>,
    pub beta: DomainHandle<BetaDomain>,
    pub third: DomainHandle<support::AlphaDomain>,
    pub calls: Arc<Calls>,
}

impl Fixture {
    pub fn new(path: &std::path::Path) -> Self {
        Self::publish(open_home(path))
    }

    pub fn publish(mut candidate: HomeOpenCandidate) -> Self {
        let alpha = candidate.register_domain::<AlphaDomain>().unwrap();
        let beta = candidate.register_domain::<BetaDomain>().unwrap();
        let third = candidate.register_domain::<support::AlphaDomain>().unwrap();
        let store = candidate
            .prepare_publication(
                HomeDomainRequirements::new()
                    .with_domain::<AlphaDomain>()
                    .unwrap()
                    .with_domain::<BetaDomain>()
                    .unwrap()
                    .with_domain::<support::AlphaDomain>()
                    .unwrap(),
            )
            .unwrap()
            .publish()
            .unwrap();
        Self {
            store,
            alpha,
            beta,
            third,
            calls: Arc::default(),
        }
    }

    pub fn mutation<D: StorageDomain>(
        &self,
        requirement: Option<CommandPairRequirement>,
    ) -> PairMutation<D> {
        PairMutation {
            put: PutBytes::new(1, D::NAME.as_bytes().to_vec()),
            requirement,
            calls: self.calls.clone(),
            cancellation: None,
        }
    }

    pub fn add<D: StorageDomain>(
        &self,
        command: &mut HomeCommand,
        handle: &DomainHandle<D>,
        kind: CommandParticipantKind,
        requirement: Option<CommandPairRequirement>,
    ) {
        let revision = self.store.domain_revision(handle).unwrap();
        match kind {
            CommandParticipantKind::Mutation => {
                command
                    .add(handle.contribution(revision, self.mutation::<D>(requirement)))
                    .unwrap();
            }
            CommandParticipantKind::Validation => {
                command
                    .add_validation(handle.validation(
                        revision,
                        PairValidator::<D> {
                            requirement,
                            calls: self.calls.clone(),
                            reject: false,
                            typed: PhantomData,
                        },
                    ))
                    .unwrap();
            }
        }
    }

    pub fn command(
        &self,
        kinds: [CommandParticipantKind; 2],
        requirements: [Option<CommandPairRequirement>; 2],
        reverse: bool,
    ) -> HomeCommand {
        let mut command = HomeCommand::new(self.store.home_revision().unwrap());
        if reverse {
            self.add(&mut command, &self.beta, kinds[1], requirements[1]);
            self.add(&mut command, &self.alpha, kinds[0], requirements[0]);
        } else {
            self.add(&mut command, &self.alpha, kinds[0], requirements[0]);
            self.add(&mut command, &self.beta, kinds[1], requirements[1]);
        }
        command
    }

    pub fn value<D: StorageDomain>(&self, handle: &DomainHandle<D>) -> Option<Vec<u8>> {
        self.store
            .read_point::<D, BytesRecord<D>>(handle, &1, PointReadLimit::new(1_028).unwrap())
            .unwrap()
    }
}

pub(super) fn pair(
    peer_domain: &'static str,
    peer_kind: CommandParticipantKind,
) -> Option<CommandPairRequirement> {
    Some(CommandPairRequirement::new::<1, 1>(
        peer_domain,
        peer_kind,
        [7; 32],
    ))
}

pub(super) fn requirements(
    kinds: [CommandParticipantKind; 2],
) -> [Option<CommandPairRequirement>; 2] {
    [
        pair(BetaDomain::NAME, kinds[1]),
        pair(AlphaDomain::NAME, kinds[0]),
    ]
}
