use super::*;

/// Immutable lineage authority retained by one named Syndic thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadLineageProof {
    parent_thread_id: Option<SyndicThreadId>,
    ancestor_skip: Option<SyndicThreadId>,
    depth: ThreadLineageDepth,
    digest: SyndicPathDigest,
}

impl ThreadLineageProof {
    #[must_use]
    pub const fn new(
        parent_thread_id: Option<SyndicThreadId>,
        ancestor_skip: Option<SyndicThreadId>,
        depth: ThreadLineageDepth,
        digest: SyndicPathDigest,
    ) -> Self {
        Self {
            parent_thread_id,
            ancestor_skip,
            depth,
            digest,
        }
    }

    #[must_use]
    pub const fn parent_thread_id(self) -> Option<SyndicThreadId> {
        self.parent_thread_id
    }

    #[must_use]
    pub const fn ancestor_skip(self) -> Option<SyndicThreadId> {
        self.ancestor_skip
    }

    #[must_use]
    pub const fn depth(self) -> ThreadLineageDepth {
        self.depth
    }

    #[must_use]
    pub const fn digest(self) -> SyndicPathDigest {
        self.digest
    }
}

/// Authoritative mutable bindings for one named Syndic thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadRecord {
    id: SyndicThreadId,
    selected_path: SelectedPathProof,
    current_draft_id: SyndicDraftId,
    lineage: ThreadLineageProof,
    image_label_frontiers: ThreadImageLabelFrontiers,
    context_owner_id: Option<DiscussionContextOwnerId>,
}

impl ThreadRecord {
    #[must_use]
    pub const fn new(
        id: SyndicThreadId,
        selected_path: SelectedPathProof,
        current_draft_id: SyndicDraftId,
        lineage: ThreadLineageProof,
        image_label_frontiers: ThreadImageLabelFrontiers,
        context_owner_id: Option<DiscussionContextOwnerId>,
    ) -> Self {
        Self {
            id,
            selected_path,
            current_draft_id,
            lineage,
            image_label_frontiers,
            context_owner_id,
        }
    }
    #[must_use]
    pub const fn id(&self) -> SyndicThreadId {
        self.id
    }
    #[must_use]
    pub const fn revision(&self) -> ThreadRevision {
        self.selected_path.thread_revision()
    }
    #[must_use]
    pub const fn committed_tail(&self) -> Option<SyndicTurnId> {
        self.selected_path.tail()
    }
    #[must_use]
    pub const fn current_draft_id(&self) -> SyndicDraftId {
        self.current_draft_id
    }
    #[must_use]
    pub const fn parent_thread_id(&self) -> Option<SyndicThreadId> {
        self.lineage.parent_thread_id()
    }
    #[must_use]
    pub const fn lineage_ancestor_skip(&self) -> Option<SyndicThreadId> {
        self.lineage.ancestor_skip()
    }
    #[must_use]
    pub const fn lineage_depth(&self) -> ThreadLineageDepth {
        self.lineage.depth()
    }
    #[must_use]
    pub const fn lineage_digest(&self) -> SyndicPathDigest {
        self.lineage.digest()
    }
    #[must_use]
    pub const fn image_label_frontiers(&self) -> ThreadImageLabelFrontiers {
        self.image_label_frontiers
    }
    #[must_use]
    pub const fn context_owner_id(&self) -> Option<DiscussionContextOwnerId> {
        self.context_owner_id
    }
    #[must_use]
    pub const fn selected_path_digest(&self) -> SyndicPathDigest {
        self.selected_path.digest()
    }
    #[must_use]
    pub const fn selected_path(&self) -> SelectedPathProof {
        self.selected_path
    }
    #[must_use]
    pub const fn lineage(&self) -> ThreadLineageProof {
        self.lineage
    }
}
