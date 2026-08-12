use std::marker::PhantomData;

use beryl_model::DomainRevision;

use crate::{
    DomainReader, StorageDomain,
    command::{
        DomainMutation, DomainValidator, MutationBuilder, MutationContribution, PendingMutation,
        ReconciliationReservation, ReconciliationReservationOutput, ValidationContribution,
    },
    domain::{DomainOwnerId, RegisteredDomain, StoreInstanceId, callback::ErasedCallbackError},
};

trait ErasedValidation: Send {
    fn validate(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<(), ErasedCallbackError>;
}

trait ErasedMutation: ErasedValidation {
    fn reserve_reconciliation(
        &self,
    ) -> Result<ReconciliationReservationOutput, ErasedCallbackError>;

    fn assemble(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<Vec<PendingMutation>, ErasedCallbackError>;
}

struct TypedMutation<D: StorageDomain, M: DomainMutation<D>> {
    mutation: M,
    _typed: PhantomData<fn(D) -> D>,
}

impl<D: StorageDomain, M: DomainMutation<D>> ErasedValidation for TypedMutation<D, M> {
    fn validate(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<(), ErasedCallbackError> {
        self.mutation
            .validate(&DomainReader::new(snapshot, domain))
            .map_err(ErasedCallbackError::from_typed)
    }
}

impl<D: StorageDomain, M: DomainMutation<D>> ErasedMutation for TypedMutation<D, M> {
    fn reserve_reconciliation(
        &self,
    ) -> Result<ReconciliationReservationOutput, ErasedCallbackError> {
        let mut reservation = ReconciliationReservation::<D>::new();
        self.mutation
            .reserve_reconciliation(&mut reservation)
            .map_err(ErasedCallbackError::from_typed)?;
        Ok(reservation.into_output())
    }

    fn assemble(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<Vec<PendingMutation>, ErasedCallbackError> {
        let reader = DomainReader::new(snapshot, domain);
        let mut builder = MutationBuilder::<D>::new(domain);
        self.mutation
            .contribute(&reader, &mut builder)
            .map_err(ErasedCallbackError::from_typed)?;
        Ok(builder.into_pending())
    }
}

struct TypedValidation<D: StorageDomain, V: DomainValidator<D>> {
    validator: V,
    _typed: PhantomData<fn(D) -> D>,
}

impl<D: StorageDomain, V: DomainValidator<D>> ErasedValidation for TypedValidation<D, V> {
    fn validate(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<(), ErasedCallbackError> {
        self.validator
            .validate(&DomainReader::new(snapshot, domain))
            .map_err(ErasedCallbackError::from_typed)
    }
}

pub(crate) struct DomainMutationPlan {
    pub(crate) store: StoreInstanceId,
    pub(crate) slot: usize,
    pub(crate) owner: DomainOwnerId,
    pub(crate) domain: &'static str,
    mutation: Box<dyn ErasedMutation>,
}

impl DomainMutationPlan {
    pub(crate) fn reserve_reconciliation(
        &self,
    ) -> Result<ReconciliationReservationOutput, ErasedCallbackError> {
        self.mutation.reserve_reconciliation()
    }
}

pub(crate) struct DomainValidationPlan {
    pub(crate) store: StoreInstanceId,
    pub(crate) slot: usize,
    pub(crate) owner: DomainOwnerId,
    pub(crate) domain: &'static str,
    validator: Box<dyn ErasedValidation>,
}

pub(super) fn mutation_plan<D: StorageDomain, M: DomainMutation<D>>(
    store: StoreInstanceId,
    slot: usize,
    owner: DomainOwnerId,
    mutation: M,
) -> DomainMutationPlan {
    DomainMutationPlan {
        store,
        slot,
        owner,
        domain: D::NAME,
        mutation: Box::new(TypedMutation::<D, M> {
            mutation,
            _typed: PhantomData,
        }),
    }
}

pub(super) fn validation_plan<D: StorageDomain, V: DomainValidator<D>>(
    store: StoreInstanceId,
    slot: usize,
    owner: DomainOwnerId,
    validator: V,
) -> DomainValidationPlan {
    DomainValidationPlan {
        store,
        slot,
        owner,
        domain: D::NAME,
        validator: Box::new(TypedValidation::<D, V> {
            validator,
            _typed: PhantomData,
        }),
    }
}

pub(crate) enum DomainParticipant {
    Mutation(MutationContribution),
    Validation(ValidationContribution),
}

impl DomainParticipant {
    pub(crate) const fn store(&self) -> StoreInstanceId {
        match self {
            Self::Mutation(contribution) => contribution.plan.store,
            Self::Validation(contribution) => contribution.plan.store,
        }
    }

    pub(crate) const fn slot(&self) -> usize {
        match self {
            Self::Mutation(contribution) => contribution.plan.slot,
            Self::Validation(contribution) => contribution.plan.slot,
        }
    }

    pub(crate) const fn owner(&self) -> DomainOwnerId {
        match self {
            Self::Mutation(contribution) => contribution.plan.owner,
            Self::Validation(contribution) => contribution.plan.owner,
        }
    }

    pub(crate) const fn domain(&self) -> &'static str {
        match self {
            Self::Mutation(contribution) => contribution.plan.domain,
            Self::Validation(contribution) => contribution.plan.domain,
        }
    }

    pub(crate) const fn expected_revision(&self) -> DomainRevision {
        match self {
            Self::Mutation(contribution) => contribution.expected_revision,
            Self::Validation(contribution) => contribution.expected_revision,
        }
    }

    pub(crate) const fn is_mutation(&self) -> bool {
        matches!(self, Self::Mutation(_))
    }

    pub(crate) fn validate(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Result<(), ErasedCallbackError> {
        match self {
            Self::Mutation(contribution) => contribution.plan.mutation.validate(snapshot, domain),
            Self::Validation(contribution) => {
                contribution.plan.validator.validate(snapshot, domain)
            }
        }
    }

    pub(crate) fn reserve_reconciliation(
        &self,
    ) -> Option<Result<ReconciliationReservationOutput, ErasedCallbackError>> {
        match self {
            Self::Mutation(contribution) => Some(contribution.plan.reserve_reconciliation()),
            Self::Validation(_) => None,
        }
    }

    pub(crate) fn assemble_mutation(
        &self,
        snapshot: &fjall::Snapshot,
        domain: &RegisteredDomain,
    ) -> Option<Result<Vec<PendingMutation>, ErasedCallbackError>> {
        match self {
            Self::Mutation(contribution) => {
                Some(contribution.plan.mutation.assemble(snapshot, domain))
            }
            Self::Validation(_) => None,
        }
    }
}
