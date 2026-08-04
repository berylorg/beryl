use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingHeadRecord {
    pub(crate) thread_id: SyndicThreadId,
    pub(crate) revision: BindingRevision,
    pub(crate) lifecycle: BindingLifecycle,
    pub(crate) selected_path_digest: SyndicPathDigest,
}

impl BindingHeadRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        revision: BindingRevision,
        lifecycle: BindingLifecycle,
        selected_path_digest: SyndicPathDigest,
    ) -> Self {
        Self {
            thread_id,
            revision,
            lifecycle,
            selected_path_digest,
        }
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn revision(&self) -> BindingRevision {
        self.revision
    }

    #[must_use]
    pub const fn lifecycle(&self) -> BindingLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn selected_path_digest(&self) -> SyndicPathDigest {
        self.selected_path_digest
    }
}
