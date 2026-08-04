use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder};

use crate::{
    BindingHeadRecord, BindingRecord, BindingState, CasThreadBindingIndexRecord,
    CasThreadIndexRecord, SyndicMutationError, UsableCasBinding, codec::*, domain::SyndicDomain,
};

use super::{
    PublishStaleBinding, PublishUnboundBinding, PublishValidBinding,
    validation::{
        ensure_not_active, membership, reservation, retirement, transition_base,
        validate_canonical_execution, validate_stale, validate_usable_current,
    },
};

pub(crate) enum PublishBindingMutation {
    Valid(PublishValidBinding),
    Stale(PublishStaleBinding),
    Unbound(PublishUnboundBinding),
}

impl PublishBindingMutation {
    pub(super) const fn valid(request: PublishValidBinding) -> Self {
        Self::Valid(request)
    }

    pub(super) const fn stale(request: PublishStaleBinding) -> Self {
        Self::Stale(request)
    }

    pub(super) const fn unbound(request: PublishUnboundBinding) -> Self {
        Self::Unbound(request)
    }

    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<PublishBindingRecords, SyndicMutationError> {
        match self {
            Self::Valid(request) => {
                validate_canonical_execution(reader, request.thread_id, &request.execution)?;
                let base = transition_base(
                    reader,
                    request.thread_id,
                    request.expected_binding_revision,
                    request.selected_path,
                )?;
                ensure_not_active(&base.current)?;
                let usable = UsableCasBinding::new(
                    request.execution.clone(),
                    request.cas_thread_id.clone(),
                    request.represented_prefix,
                    request.native_turn_count,
                    request.tool_profile,
                    request.lineage,
                );
                validate_usable_current(reader, request.selected_path, &usable)?;
                let reservation =
                    reservation(reader, &usable, request.thread_id, base.next_revision)?;
                let membership = membership(
                    reader,
                    usable.cas_thread_id(),
                    request.thread_id,
                    base.next_revision,
                )?;
                Ok(PublishBindingRecords::new(
                    request.thread_id,
                    base.next_revision,
                    request.selected_path,
                    BindingState::valid(usable),
                    Some(reservation),
                    Some(membership),
                ))
            }
            Self::Stale(request) => {
                validate_canonical_execution(reader, request.thread_id, request.stale.execution())?;
                let base = transition_base(
                    reader,
                    request.thread_id,
                    request.expected_binding_revision,
                    request.selected_path,
                )?;
                ensure_not_active(&base.current)?;
                validate_stale(reader, request.selected_path, &request.stale)?;
                let reservation = retirement(
                    reader,
                    &request.stale,
                    request.thread_id,
                    base.next_revision,
                )?;
                let membership = membership(
                    reader,
                    request.stale.cas_thread_id(),
                    request.thread_id,
                    base.next_revision,
                )?;
                Ok(PublishBindingRecords::new(
                    request.thread_id,
                    base.next_revision,
                    request.selected_path,
                    BindingState::stale(request.stale.clone()),
                    reservation,
                    Some(membership),
                ))
            }
            Self::Unbound(request) => {
                let base = transition_base(
                    reader,
                    request.thread_id,
                    request.expected_binding_revision,
                    request.selected_path,
                )?;
                ensure_not_active(&base.current)?;
                Ok(PublishBindingRecords::new(
                    request.thread_id,
                    base.next_revision,
                    request.selected_path,
                    request.state.clone(),
                    None,
                    None,
                ))
            }
        }
    }
}

impl DomainMutation<SyndicDomain> for PublishBindingMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.records(reader)?.contribute(mutations)
    }
}

struct PublishBindingRecords {
    binding: BindingRecord,
    head: BindingHeadRecord,
    reservation: Option<CasThreadIndexRecord>,
    membership: Option<CasThreadBindingIndexRecord>,
}

impl PublishBindingRecords {
    fn new(
        thread_id: beryl_model::SyndicThreadId,
        revision: beryl_model::BindingRevision,
        selected_path: crate::SelectedPathProof,
        state: BindingState,
        reservation: Option<CasThreadIndexRecord>,
        membership: Option<CasThreadBindingIndexRecord>,
    ) -> Self {
        let head = BindingHeadRecord::new(
            thread_id,
            revision,
            state.lifecycle(),
            selected_path.digest(),
        );
        Self {
            binding: BindingRecord::new(thread_id, revision, selected_path, state),
            head,
            reservation,
            membership,
        }
    }

    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<BindingsCodec>(
            &BindingKey {
                thread: self.binding.thread_id(),
                revision: self.binding.revision(),
            },
            &self.binding,
        )?;
        mutations.put::<BindingHeadsCodec>(&self.binding.thread_id(), &self.head)?;
        if let Some(reservation) = &self.reservation {
            mutations.put::<CasThreadIndexCodec>(
                &CasThreadKey::Record(reservation.cas_thread_id().clone()),
                reservation,
            )?;
        }
        if let Some(membership) = &self.membership {
            mutations.put::<CasThreadBindingIndexCodec>(
                &CasThreadBindingKey::Record(
                    membership.cas_thread_id().clone(),
                    membership.binding_revision(),
                ),
                membership,
            )?;
        }
        Ok(())
    }
}
