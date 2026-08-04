use beryl_home_store::{
    CurrentDomainCommand, DomainMutation, DomainReader, MutationBuilder, MutationContribution,
};
use beryl_model::{DomainRevision, SyndicItemId, SyndicThreadId, SyndicTurnId};

use crate::mutation::live::{activity_order, entry_stored_bytes, prune_completed};
use crate::mutation::{point, required};
use crate::{
    ActivityChildHandoffFact, ActivityChildHandoffMembership, ActivityCompactFact,
    ActivityItemSource, ActivityQueryEntryRecord, ActivityQueryHeadRecord, ActivityQueryRevision,
    ActivityQuerySource, ActivityQuerySourceRecord, ProjectionLifecycle, ProjectionSourceRange,
    ProviderItemKind, ProviderItemLifecycle, SyndicMutationError, SyndicStorage, codec::*,
    domain::SyndicDomain,
};

/// Exact bounded publication of one observed child final-answer handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishActivityChildHandoff {
    thread_id: SyndicThreadId,
    expected_activity_revision: ActivityQueryRevision,
    child_thread_id: SyndicThreadId,
    child_turn_id: SyndicTurnId,
    final_answer_item_id: SyndicItemId,
    final_answer_range: ProjectionSourceRange,
}

impl PublishActivityChildHandoff {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_activity_revision: ActivityQueryRevision,
        child_thread_id: SyndicThreadId,
        child_turn_id: SyndicTurnId,
        final_answer_item_id: SyndicItemId,
        final_answer_range: ProjectionSourceRange,
    ) -> Self {
        Self {
            thread_id,
            expected_activity_revision,
            child_thread_id,
            child_turn_id,
            final_answer_item_id,
            final_answer_range,
        }
    }
}

impl SyndicStorage {
    /// Publishes one exact child handoff into the current activity work period.
    #[must_use]
    pub fn publish_activity_child_handoff(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishActivityChildHandoff,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            PublishActivityChildHandoffMutation { request },
        )
    }

    /// Publishes one exact child handoff against writer-captured physical revisions.
    #[must_use]
    pub fn current_publish_activity_child_handoff(
        &self,
        request: PublishActivityChildHandoff,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(PublishActivityChildHandoffMutation { request })
    }
}

struct PublishActivityChildHandoffMutation {
    request: PublishActivityChildHandoff,
}

struct HandoffRecords {
    head: ActivityQueryHeadRecord,
    source: ActivityQuerySourceRecord,
    delete: Vec<ActivityQueryEntryKey>,
    entry: Option<ActivityQueryEntryRecord>,
}

impl PublishActivityChildHandoffMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<HandoffRecords, SyndicMutationError> {
        let request = self.request;
        let head = required::<ActivityQueryHeadsFamily>(reader, &request.thread_id)?;
        if head.revision() != request.expected_activity_revision
            || !head.source_active()
            || head.lifecycle() != ProjectionLifecycle::Current
        {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        let owner = required::<ThreadsFamily>(reader, &request.thread_id)?;
        let child = required::<ThreadsFamily>(reader, &request.child_thread_id)?;
        let turn = required::<TurnsFamily>(reader, &request.child_turn_id)?;
        let state = required::<TurnStatesFamily>(reader, &request.child_turn_id)?;
        let item = required::<CanonicalItemsFamily>(reader, &request.final_answer_item_id)?;
        if child.parent_thread_id() != Some(owner.id())
            || turn.origin_thread_id() != child.id()
            || !state.lifecycle().is_proven_terminal()
            || item.turn_id() != turn.id()
            || item
                .source_event()
                .and_then(|event| event.get().checked_add(1))
                != Some(state.source_event_count())
            || item.provider_kind() != ProviderItemKind::AgentMessage
            || item.provider_lifecycle() != ProviderItemLifecycle::Completed
            || item.assistant_phase() != Some(crate::AssistantMessagePhase::FinalAnswer)
            || !matches!(
                item.presentation(),
                crate::CanonicalItemPresentation::Narrative
            )
            || item
                .provider()
                .and_then(|provider| provider.narrative())
                .is_none_or(|narrative| {
                    request.final_answer_range.end() > narrative.logical_utf8_bytes()
                })
        {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        let source = ActivityQuerySource::new(child.id(), turn.id());
        let source_key = ActivityQuerySourceKey {
            thread: owner.id(),
            work_period: head.work_period(),
            source_thread: child.id(),
            source_turn: turn.id(),
        };
        if point::<ActivityQuerySourcesFamily>(reader, &source_key)?.is_some() {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        let source_count = head
            .source_count()
            .checked_add(1)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        let source_frontier = head
            .source_frontier()
            .checked_add(state.source_event_count())
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        let order = activity_order(&item)?;
        let entry_key = ActivityQueryEntryKey {
            thread: owner.id(),
            work_period: head.work_period(),
            order,
        };
        if point::<ActivityQueryEntriesFamily>(reader, &entry_key)?.is_some() {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        let entry = ActivityQueryEntryRecord::new(
            owner.id(),
            head.work_period(),
            order,
            ActivityItemSource::new(
                child.id(),
                turn.id(),
                item.id(),
                item.cas_source()
                    .cloned()
                    .ok_or(SyndicMutationError::ActivityQueryConflict)?,
            ),
            item.source_event()
                .ok_or(SyndicMutationError::ActivityQueryConflict)?,
            item.provider_kind(),
            item.provider_lifecycle(),
            Some(ActivityCompactFact::ChildHandoff(
                ActivityChildHandoffFact::new(child.id(), request.final_answer_range),
            )),
        )?;
        let mut logical_count = head
            .logical_row_count()
            .checked_add(1)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        let mut completed_count = head
            .completed_row_count()
            .checked_add(1)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        let mut completed_bytes = head
            .completed_stored_bytes()
            .checked_add(entry_stored_bytes(&entry)?)
            .ok_or(SyndicMutationError::ActivityQueryConflict)?;
        let mut delete = Vec::new();
        let mut entry = Some(entry);
        let cutoff = prune_completed(
            reader,
            &head,
            &mut logical_count,
            &mut completed_count,
            &mut completed_bytes,
            &mut delete,
            &mut entry,
        )?;
        let next_head = ActivityQueryHeadRecord::new(
            owner.id(),
            head.work_period(),
            head.source(),
            head.source_active(),
            source_frontier,
            head.revision().checked_next()?,
            source_count,
            logical_count,
            head.running_row_count(),
            completed_count,
            completed_bytes,
            cutoff,
            ProjectionLifecycle::Current,
        )?;
        let source = ActivityQuerySourceRecord::new(
            owner.id(),
            head.work_period(),
            source,
            item.source_event(),
            state.source_event_count(),
            false,
            Some(ActivityChildHandoffMembership::new(
                item.id(),
                request.final_answer_range,
            )),
        );
        Ok(HandoffRecords {
            head: next_head,
            source,
            delete,
            entry,
        })
    }
}

impl HandoffRecords {
    fn contribute(
        self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        mutations.put::<ActivityQueryHeadsCodec>(&self.head.thread_id(), &self.head)?;
        mutations.put::<ActivityQuerySourcesCodec>(
            &ActivityQuerySourceKey {
                thread: self.source.thread_id(),
                work_period: self.source.work_period(),
                source_thread: self.source.source().thread_id(),
                source_turn: self.source.source().turn_id(),
            },
            &self.source,
        )?;
        for key in self.delete {
            mutations.delete::<ActivityQueryEntriesCodec>(&key)?;
        }
        if let Some(entry) = self.entry {
            mutations.put::<ActivityQueryEntriesCodec>(
                &ActivityQueryEntryKey {
                    thread: entry.thread_id(),
                    work_period: entry.work_period(),
                    order: entry.order(),
                },
                &entry,
            )?;
        }
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for PublishActivityChildHandoffMutation {
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
