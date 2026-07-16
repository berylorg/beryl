use beryl_home_store::HomeGeneration;
use beryl_model::{
    BerylHomeId, DiscussionContextOwnerId, DraftRevision, SyndicDraftId, SyndicThreadId,
    ThreadRevision,
};
use syndic_storage::{
    ComposerPayload, ConversationParent, DraftRecord, ReplacementEditIntent, SyndicCurrentDraft,
    SyndicTimestamp,
};

use super::{DraftBindingGeneration, DraftPersistenceTime};

/// Exact durable home/thread/draft authority held by one persistence service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPersistenceBinding {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    thread_revision: ThreadRevision,
    generation: DraftBindingGeneration,
}

impl DraftPersistenceBinding {
    #[must_use]
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }

    #[must_use]
    pub const fn thread_revision(self) -> ThreadRevision {
        self.thread_revision
    }

    #[must_use]
    pub const fn generation(self) -> DraftBindingGeneration {
        self.generation
    }

    pub(crate) const fn initial(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        thread_id: SyndicThreadId,
        draft_id: SyndicDraftId,
        thread_revision: ThreadRevision,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            thread_id,
            draft_id,
            thread_revision,
            generation: DraftBindingGeneration::FIRST,
        }
    }

    pub(crate) const fn recovered(
        self,
        home_generation: HomeGeneration,
        generation: DraftBindingGeneration,
    ) -> Self {
        Self {
            home_generation,
            generation,
            ..self
        }
    }
}

/// Exact durable current-draft seed used for startup or same-home reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPersistenceSeed {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    current: SyndicCurrentDraft,
    payload: ComposerPayload,
    published_at: DraftPersistenceTime,
}

impl DraftPersistenceSeed {
    #[must_use]
    pub const fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        current: SyndicCurrentDraft,
        payload: ComposerPayload,
        published_at: DraftPersistenceTime,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            current,
            payload,
            published_at,
        }
    }

    #[must_use]
    pub const fn home_id(&self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(&self) -> HomeGeneration {
        self.home_generation
    }

    #[must_use]
    pub const fn current(&self) -> &SyndicCurrentDraft {
        &self.current
    }

    #[must_use]
    pub const fn payload(&self) -> &ComposerPayload {
        &self.payload
    }

    #[must_use]
    pub const fn published_at(&self) -> DraftPersistenceTime {
        self.published_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DurableDraftBase {
    pub(crate) revision: DraftRevision,
    pub(crate) payload: ComposerPayload,
    pub(crate) updated_at: SyndicTimestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableDraftShape {
    pub(crate) parent: ConversationParent,
    pub(crate) context_owner_id: Option<DiscussionContextOwnerId>,
    pub(crate) replacement_edit_intent: Option<ReplacementEditIntent>,
    pub(crate) created_at: SyndicTimestamp,
}

impl ImmutableDraftShape {
    pub(crate) const fn from_record(draft: &DraftRecord) -> Self {
        Self {
            parent: draft.parent(),
            context_owner_id: draft.context_owner_id(),
            replacement_edit_intent: draft.replacement_edit_intent(),
            created_at: draft.created_at(),
        }
    }

    pub(crate) fn matches(self, draft: &DraftRecord) -> bool {
        self == Self::from_record(draft)
    }
}
