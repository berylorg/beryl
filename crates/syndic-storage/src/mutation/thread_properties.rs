use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, MutationContribution, ReconciliationReservation,
};
use beryl_model::{DomainRevision, JobId, SyndicThreadId};

use crate::{
    BindingState, CanonicalItemKind, ContentLifecycle, GeneratedThreadTitle, SyndicMutationError,
    SyndicTimestamp, ThreadArchiveState, ThreadAttributesRevision, ThreadUsageObservation,
    ThreadUsageRevision, codec::*, domain::SyndicDomain,
};

use super::required;

mod catalog;

/// One-way acceptance of an exact generated title and its immutable source witness.
pub struct AcceptGeneratedThreadTitle {
    thread_id: SyndicThreadId,
    expected_attributes_revision: ThreadAttributesRevision,
    title: GeneratedThreadTitle,
}

impl AcceptGeneratedThreadTitle {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_attributes_revision: ThreadAttributesRevision,
        title: GeneratedThreadTitle,
    ) -> Self {
        Self {
            thread_id,
            expected_attributes_revision,
            title,
        }
    }
}

/// One-way intrinsic archive publication composed with exact durable handoff success.
pub struct ArchiveBranchDiscussionThread {
    thread_id: SyndicThreadId,
    expected_attributes_revision: ThreadAttributesRevision,
    handoff_job_id: JobId,
    archived_at: SyndicTimestamp,
}

impl ArchiveBranchDiscussionThread {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_attributes_revision: ThreadAttributesRevision,
        handoff_job_id: JobId,
        archived_at: SyndicTimestamp,
    ) -> Self {
        Self {
            thread_id,
            expected_attributes_revision,
            handoff_job_id,
            archived_at,
        }
    }
}

/// Exact latest token-usage publication from one authenticated compact provider control.
pub struct PublishThreadUsage {
    thread_id: SyndicThreadId,
    expected_usage_revision: ThreadUsageRevision,
    observation: ThreadUsageObservation,
}

impl PublishThreadUsage {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_usage_revision: ThreadUsageRevision,
        observation: ThreadUsageObservation,
    ) -> Self {
        Self {
            thread_id,
            expected_usage_revision,
            observation,
        }
    }
}

impl crate::SyndicStorage {
    #[must_use]
    pub fn accept_generated_thread_title(
        &self,
        expected_domain_revision: DomainRevision,
        request: AcceptGeneratedThreadTitle,
    ) -> MutationContribution {
        self.handle.contribution(expected_domain_revision, request)
    }

    #[must_use]
    pub fn archive_branch_discussion(
        &self,
        expected_domain_revision: DomainRevision,
        request: ArchiveBranchDiscussionThread,
    ) -> MutationContribution {
        self.handle.contribution(expected_domain_revision, request)
    }

    #[must_use]
    pub fn publish_thread_usage(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishThreadUsage,
    ) -> MutationContribution {
        self.handle.contribution(expected_domain_revision, request)
    }
}

impl DomainMutation<SyndicDomain> for AcceptGeneratedThreadTitle {
    type Error = SyndicMutationError;
    type Prepared = (SyndicThreadId, crate::ThreadAttributesRecord);

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let attributes = required::<ThreadAttributesFamily>(reader, &self.thread_id)?;
        if attributes.revision() != self.expected_attributes_revision {
            return Err(SyndicMutationError::ThreadAttributesRevisionConflict {
                expected: self.expected_attributes_revision,
                current: attributes.revision(),
            });
        }
        if attributes.generated_title().is_some() {
            return Err(SyndicMutationError::GeneratedTitleAlreadyAccepted);
        }
        validate_title_eligibility(reader, self.thread_id, &self.title)?;
        let next = attributes.accept_generated_title(self.title)?;
        Ok((self.thread_id, next))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ThreadAttributesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ThreadAttributesCodec>(&prepared.0, &prepared.1)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for ArchiveBranchDiscussionThread {
    type Error = SyndicMutationError;
    type Prepared = (SyndicThreadId, crate::ThreadAttributesRecord);

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let thread = required::<ThreadsFamily>(reader, &self.thread_id)?;
        let attributes = required::<ThreadAttributesFamily>(reader, &self.thread_id)?;
        if attributes.revision() != self.expected_attributes_revision {
            return Err(SyndicMutationError::ThreadAttributesRevisionConflict {
                expected: self.expected_attributes_revision,
                current: attributes.revision(),
            });
        }
        if thread.parent_thread_id().is_none()
            || attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
        {
            return Err(SyndicMutationError::ThreadArchiveStateConflict);
        }
        let next = attributes.archive_branch_discussion(self.handoff_job_id, self.archived_at)?;
        Ok((self.thread_id, next))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ThreadAttributesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ThreadAttributesCodec>(&prepared.0, &prepared.1)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for PublishThreadUsage {
    type Error = SyndicMutationError;
    type Prepared = (SyndicThreadId, crate::ThreadUsageRecord);

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let usage = required::<ThreadUsageFamily>(reader, &self.thread_id)?;
        if usage.revision() != self.expected_usage_revision {
            return Err(SyndicMutationError::ThreadUsageRevisionConflict {
                expected: self.expected_usage_revision,
                current: usage.revision(),
            });
        }
        if usage.observation().is_some_and(|prior| {
            prior.provider_control_ordinal() >= self.observation.provider_control_ordinal()
        }) {
            return Err(SyndicMutationError::UsageProviderOrdinalConflict);
        }
        validate_current_usage_route(reader, self.thread_id, &self.observation)?;
        let next = usage.publish(self.observation)?;
        Ok((self.thread_id, next))
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ThreadUsageCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ThreadUsageCodec>(&prepared.0, &prepared.1)?;
        Ok(())
    }
}

fn validate_title_eligibility(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    title: &GeneratedThreadTitle,
) -> Result<(), SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &thread_id)?;
    if title.source_thread_revision() != thread.revision()
        || title.source_selected_path_digest() != thread.selected_path_digest()
    {
        return Err(SyndicMutationError::SourceTailConflict);
    }
    let turn = required::<TurnsFamily>(reader, &title.source_turn_id())?;
    let tail = required::<TurnsFamily>(
        reader,
        &thread
            .committed_tail()
            .ok_or(SyndicMutationError::SourceTailConflict)?,
    )?;
    if turn.kind() != crate::TurnKind::OrdinaryUser
        || turn.origin_thread_id() != thread_id
        || title.generated_at() < turn.submitted_at()
        || !crate::selected_path::includes_turn(
            tail,
            &turn,
            |id| required::<TurnsFamily>(reader, &id),
            |_| SyndicMutationError::SourceTailConflict,
        )?
    {
        return Err(SyndicMutationError::SourceTailConflict);
    }
    let index = required::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: turn.id(),
            ordinal: crate::TurnItemOrdinal::FIRST,
        },
    )?;
    let item = required::<CanonicalItemsFamily>(reader, &index.item_id())?;
    let content = required::<ContentManifestsFamily>(reader, &title.source_content().id())?;
    if item.turn_id() != turn.id()
        || item.kind() != CanonicalItemKind::UserInput
        || item.presentation_content() != Some(title.source_content())
        || content.lifecycle() != ContentLifecycle::Sealed
        || content.sealed_reference() != Some(title.source_content())
    {
        return Err(SyndicMutationError::CanonicalItemConflict);
    }
    Ok(())
}

fn validate_current_usage_route(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    observation: &ThreadUsageObservation,
) -> Result<(), SyndicMutationError> {
    let canonical = required::<ThreadExecutionsFamily>(reader, &thread_id)?;
    let head = required::<BindingHeadsFamily>(reader, &thread_id)?;
    if canonical.execution() != observation.execution()
        || head.revision() != observation.binding_revision()
    {
        return Err(SyndicMutationError::UsageRouteConflict);
    }
    let binding = required::<BindingsFamily>(
        reader,
        &BindingKey {
            thread: thread_id,
            revision: head.revision(),
        },
    )?;
    let usable = match binding.state() {
        BindingState::Valid(usable) => usable,
        BindingState::Active(active) => {
            let snapshot = required::<ExecutionSnapshotsFamily>(reader, &active.snapshot_id())?;
            if snapshot.execution() != observation.execution()
                || snapshot.cas_thread_id() != observation.cas_thread_id()
                || snapshot.binding_revision() != observation.binding_revision()
                || snapshot.loaded_generation() != observation.loaded_generation()
            {
                return Err(SyndicMutationError::UsageRouteConflict);
            }
            active.usable()
        }
        BindingState::Unbound { .. } | BindingState::Stale(_) => {
            return Err(SyndicMutationError::UsageRouteConflict);
        }
    };
    if usable.execution() != observation.execution()
        || usable.cas_thread_id() != observation.cas_thread_id()
    {
        return Err(SyndicMutationError::UsageRouteConflict);
    }
    Ok(())
}
