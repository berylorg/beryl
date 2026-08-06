use beryl_model::{conversation::ConversationThreadId, workspace::BerylWorkspaceId};

use super::phase_thread_preparation_core::PhaseThreadPreparationRequest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedPhaseThreadRegistration {
    child_thread_id: ConversationThreadId,
    created_at_millis: i64,
    updated_at_millis: i64,
}

impl PreparedPhaseThreadRegistration {
    pub(crate) fn new(
        child_thread_id: ConversationThreadId,
        created_at_millis: i64,
        updated_at_millis: i64,
    ) -> Self {
        Self {
            child_thread_id,
            created_at_millis,
            updated_at_millis,
        }
    }

    pub(crate) fn child_thread_id(&self) -> &ConversationThreadId {
        &self.child_thread_id
    }

    pub(crate) fn created_at_millis(&self) -> i64 {
        self.created_at_millis
    }

    pub(crate) fn updated_at_millis(&self) -> i64 {
        self.updated_at_millis
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeferredPhaseThreadOutcome {
    request: PhaseThreadPreparationRequest,
    title: String,
    detail: String,
    refresh_inventory: bool,
    prepared_registration: Option<PreparedPhaseThreadRegistration>,
}

impl DeferredPhaseThreadOutcome {
    pub(crate) fn new(
        request: PhaseThreadPreparationRequest,
        title: String,
        detail: String,
        refresh_inventory: bool,
        prepared_registration: Option<PreparedPhaseThreadRegistration>,
    ) -> Self {
        Self {
            request,
            title,
            detail,
            refresh_inventory,
            prepared_registration,
        }
    }

    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        self.request.workspace_id()
    }

    pub(crate) fn request(&self) -> &PhaseThreadPreparationRequest {
        &self.request
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn detail(&self) -> &str {
        &self.detail
    }

    pub(crate) fn refresh_inventory(&self) -> bool {
        self.refresh_inventory
    }

    pub(crate) fn prepared_registration(&self) -> Option<&PreparedPhaseThreadRegistration> {
        self.prepared_registration.as_ref()
    }
}
