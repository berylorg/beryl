use super::{InitialThreadRecords, point, required};
use crate::{codec::*, domain::SyndicDomain, *};
use beryl_home_store::{
    DomainMutation, DomainReader, DomainValidator, HomeStore, MutationBuilder, MutationContribution,
};
use beryl_model::{DiscussionContextOwnerId, SyndicDraftId, SyndicThreadId, ThreadRevision};
use std::sync::Arc;

mod outcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreateDiscussion {
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    created_at: SyndicTimestamp,
    history_policy: DraftEditHistoryPolicyV1,
}

impl CreateDiscussion {
    pub const fn new(
        thread_id: SyndicThreadId,
        draft_id: SyndicDraftId,
        created_at: SyndicTimestamp,
        history_policy: DraftEditHistoryPolicyV1,
    ) -> Self {
        Self {
            thread_id,
            draft_id,
            created_at,
            history_policy,
        }
    }
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }
}

#[derive(Clone)]
pub struct DiscussionCreationIntent(Arc<DiscussionRecords>);

struct DiscussionRecords {
    home_id: beryl_model::BerylHomeId,
    initial: InitialThreadRecords,
    context: ContextEnvelopeRecord,
    parent: ThreadParentIndexRecord,
    gate: DiscussionHandoffGateRecord,
}

impl DiscussionCreationIntent {
    pub fn thread_id(&self) -> SyndicThreadId {
        self.0.initial.thread.id()
    }
    pub fn draft_id(&self) -> SyndicDraftId {
        self.0.initial.draft.id()
    }
    pub fn initial_catalog_summary(&self) -> &ThreadCatalogSummaryRecord {
        &self.0.initial.catalog_summary
    }
}

pub struct PreparedDiscussionCreation {
    source: PreparedDiscussionSource,
    parent_execution: ThreadExecutionRecord,
    parent_labels: ImageLabelAuthorityHeadV1,
    intent: DiscussionCreationIntent,
}

impl PreparedDiscussionCreation {
    pub fn intent(&self) -> &DiscussionCreationIntent {
        &self.intent
    }

    pub fn contribution(self) -> MutationContribution {
        self.source
            .handle
            .clone()
            .contribution(self.source.domain_revision, self)
    }
}

impl SyndicStorage {
    pub fn prepare_discussion_creation(
        &self,
        store: &HomeStore,
        source: PreparedDiscussionSource,
        request: CreateDiscussion,
    ) -> Result<PreparedDiscussionCreation, SyndicReadError> {
        let revision = store.domain_revision(&source.handle)?;
        if revision != source.domain_revision || self.revision(store)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion creation preparation",
            });
        }
        let parent = source.source.thread_id();
        if request.thread_id == parent {
            return Err(SyndicReadError::Invariant(
                "discussion cannot be its own parent",
            ));
        }
        let read = |message| SyndicReadError::Invariant(message);
        let limit = SyndicPointReadLimit::new(65_536).expect("record bound is nonzero");
        let parent_thread = self
            .thread(store, parent, limit)?
            .ok_or_else(|| read("discussion parent is missing"))?;
        let turn = self
            .turn(store, source.source.turn_id(), limit)?
            .ok_or_else(|| read("discussion source turn is missing"))?;
        let parent_execution = self
            .point::<ThreadExecutionsFamily>(store, parent, limit)?
            .ok_or_else(|| read("discussion parent execution is missing"))?;
        let parent_labels = self
            .point::<ImageLabelAuthorityHeadsFamily>(store, parent, limit)?
            .ok_or_else(|| read("discussion parent labels are missing"))?;
        if parent_execution.thread_id() != parent
            || parent_labels.thread_id() != parent
            || !parent_labels.is_exact()
        {
            return Err(read("discussion parent execution or labels disagree"));
        }
        let state = self
            .turn_state(store, turn.id(), limit)?
            .ok_or_else(|| read("discussion source state is missing"))?;
        if request.created_at < state.updated_at() {
            return Err(read("discussion creation precedes its source activity"));
        }
        let (depth, digest, skip) = crate::thread_lineage::child_shape(
            request.thread_id,
            parent_thread,
            |id| {
                self.thread(store, id, limit)?
                    .ok_or_else(|| read("discussion ancestor is missing"))
            },
            read,
        )?;
        let owner = DiscussionContextOwnerId::Draft(request.draft_id);
        let context = ContextEnvelopeRecord::new(
            owner,
            ContextEnvelopeRevision::FIRST,
            DiscussionContextEnvelope::new(source.source, source.text.clone(), request.created_at)
                .map_err(|_| read("discussion envelope shape disagrees"))?,
        );
        let thread = ThreadRecord::new(
            request.thread_id,
            SelectedPathProof::new(
                Some(turn.id()),
                ThreadRevision::new(1).expect("initial revision"),
                turn.chain_digest(),
            ),
            request.draft_id,
            ThreadLineageProof::new(Some(parent), Some(skip), depth, digest),
            Some(owner),
        );
        let initial = InitialThreadRecords::new(
            thread,
            parent_execution.execution().clone(),
            ThreadAttributesRecord::branch_discussion_open(request.thread_id),
            DraftSubmissionIntent::DiscussionContext(owner),
            parent_labels.permanent(),
            None,
            request.created_at,
            request.history_policy,
        );
        let records = DiscussionRecords {
            home_id: store.home_id(),
            parent: ThreadParentIndexRecord::new(
                parent,
                request.thread_id,
                initial.thread.revision(),
                owner,
            ),
            gate: DiscussionHandoffGateRecord::open(request.thread_id),
            initial,
            context,
        };
        if store.domain_revision(&source.handle)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion creation preparation",
            });
        }
        Ok(PreparedDiscussionCreation {
            source,
            parent_execution,
            parent_labels,
            intent: DiscussionCreationIntent(Arc::new(records)),
        })
    }
}

impl DomainMutation<SyndicDomain> for PreparedDiscussionCreation {
    type Error = SyndicMutationError;
    type Prepared = DiscussionCreationIntent;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.source.validate(reader).map_err(|error| match error {
            DiscussionSourceError::Read(source)
            | DiscussionSourceError::SourceRead(SyndicReadError::Read(source)) => {
                SyndicMutationError::Read(source)
            }
            _ => SyndicMutationError::SourceTailConflict,
        })?;
        let parent = self.source.source.thread_id();
        super::repair::exclude_repair(reader, parent, None)?;
        let source_turn = required::<TurnsFamily>(reader, &self.source.source.turn_id())?;
        super::repair::exclude_repair(
            reader,
            source_turn.origin_thread_id(),
            Some(source_turn.id()),
        )?;
        if required::<ThreadExecutionsFamily>(reader, &parent)? != self.parent_execution
            || required::<ImageLabelAuthorityHeadsFamily>(reader, &parent)? != self.parent_labels
        {
            return Err(SyndicMutationError::SourceTailConflict);
        }
        let records = &self.intent.0;
        records.initial.ensure_absent(reader)?;
        if point::<ContextEnvelopesFamily>(reader, &ContextOwnerKey::from(records.context.owner()))?
            .is_some()
            || point::<ThreadParentFamily>(
                reader,
                &ThreadPairKey {
                    first: parent,
                    second: records.initial.thread.id(),
                },
            )?
            .is_some()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        Ok(self.intent)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        self.intent.0.initial.reserve(reservation)?;
        reservation.reserve_records::<ContextEnvelopesCodec>(1)?;
        reservation.reserve_records::<ThreadParentCodec>(1)?;
        reservation.reserve_records::<DiscussionHandoffGatesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = &prepared.0;
        records.initial.put(mutations)?;
        mutations.put::<ContextEnvelopesCodec>(
            &ContextOwnerKey::from(records.context.owner()),
            &records.context,
        )?;
        mutations.put::<ThreadParentCodec>(
            &ThreadPairKey {
                first: records.parent.parent_thread_id(),
                second: records.initial.thread.id(),
            },
            &records.parent,
        )?;
        mutations.put::<DiscussionHandoffGatesCodec>(&records.gate.thread_id(), &records.gate)?;
        Ok(())
    }
}
