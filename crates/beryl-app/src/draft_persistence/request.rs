use std::sync::Arc;

use beryl_home_store::CommandCancellation;
use beryl_model::{DraftRevision, SyndicDraftId, SyndicThreadId};
use syndic_storage::{ComposerPayload, SyndicTimestamp};

use super::{
    DraftBindingGeneration, DraftEditGeneration, DraftPersistenceBinding, DraftRequestGeneration,
    DraftTimerGeneration,
};

/// Exact identity of one asynchronous draft-save attempt.
#[derive(Clone, Eq, PartialEq)]
pub struct DraftSaveToken {
    binding: DraftPersistenceBinding,
    expected_revision: DraftRevision,
    payload: Arc<ComposerPayload>,
    updated_at: SyndicTimestamp,
    binding_generation: DraftBindingGeneration,
    edit_generation: DraftEditGeneration,
    timer_generation: DraftTimerGeneration,
    request_generation: DraftRequestGeneration,
}

impl DraftSaveToken {
    #[must_use]
    pub const fn binding_generation(&self) -> DraftBindingGeneration {
        self.binding_generation
    }

    #[must_use]
    pub const fn edit_generation(&self) -> DraftEditGeneration {
        self.edit_generation
    }

    #[must_use]
    pub const fn timer_generation(&self) -> DraftTimerGeneration {
        self.timer_generation
    }

    #[must_use]
    pub const fn request_generation(&self) -> DraftRequestGeneration {
        self.request_generation
    }
}

impl std::fmt::Debug for DraftSaveToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DraftSaveToken")
            .field("binding", &self.binding)
            .field("expected_revision", &self.expected_revision)
            .field("updated_at", &self.updated_at)
            .field("binding_generation", &self.binding_generation)
            .field("edit_generation", &self.edit_generation)
            .field("timer_generation", &self.timer_generation)
            .field("request_generation", &self.request_generation)
            .finish_non_exhaustive()
    }
}

/// One bounded save request ready for a non-GPUI writer worker.
#[derive(Clone)]
pub struct DraftSaveRequest {
    token: DraftSaveToken,
    cancellation: CommandCancellation,
}

impl DraftSaveRequest {
    #[must_use]
    pub const fn binding(&self) -> DraftPersistenceBinding {
        self.token.binding
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.token.binding.thread_id()
    }

    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.token.binding.draft_id()
    }

    #[must_use]
    pub const fn expected_revision(&self) -> DraftRevision {
        self.token.expected_revision
    }

    #[must_use]
    pub fn payload(&self) -> &ComposerPayload {
        self.token.payload.as_ref()
    }

    #[must_use]
    pub const fn updated_at(&self) -> SyndicTimestamp {
        self.token.updated_at
    }

    #[must_use]
    pub fn token(&self) -> DraftSaveToken {
        self.token.clone()
    }

    pub(crate) const fn token_ref(&self) -> &DraftSaveToken {
        &self.token
    }

    /// Returns the cooperative signal observed only before writer admission.
    #[must_use]
    pub fn cancellation(&self) -> CommandCancellation {
        self.cancellation.clone()
    }

    pub(crate) fn new(
        binding: DraftPersistenceBinding,
        expected_revision: DraftRevision,
        payload: ComposerPayload,
        updated_at: SyndicTimestamp,
        edit_generation: DraftEditGeneration,
        timer_generation: DraftTimerGeneration,
        request_generation: DraftRequestGeneration,
    ) -> Self {
        Self {
            token: DraftSaveToken {
                binding,
                expected_revision,
                payload: Arc::new(payload),
                updated_at,
                binding_generation: binding.generation(),
                edit_generation,
                timer_generation,
                request_generation,
            },
            cancellation: CommandCancellation::new(),
        }
    }
}

impl std::fmt::Debug for DraftSaveRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DraftSaveRequest")
            .field("binding", &self.binding())
            .field("expected_revision", &self.expected_revision())
            .field("updated_at", &self.updated_at())
            .field("token", &self.token)
            .finish_non_exhaustive()
    }
}
