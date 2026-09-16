use beryl_home_store::HomeCandidateRecoveryAccess;
use beryl_model::BindingRevision;
use syndic_storage::{AbandonActiveBinding, AbandonStopOperation, SyndicStorage};

use crate::cas_projection::{ProjectionPublicationFailure, publication};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupRecoverySnapshot {
    pub page_reads: u64,
    pub cases: u64,
    pub pending_turns: u64,
    pub active_convergences: u64,
    pub terminal_convergences: u64,
    pub deferred_compactions: u64,
}

impl From<crate::cas_projection::accepted_input_scheduler::StartupRecoveryDiagnostics>
    for StartupRecoverySnapshot
{
    fn from(
        value: crate::cas_projection::accepted_input_scheduler::StartupRecoveryDiagnostics,
    ) -> Self {
        Self {
            page_reads: value.page_reads,
            cases: value.cases,
            pending_turns: value.pending_turns,
            active_convergences: value.active_convergences,
            terminal_convergences: value.terminal_convergences,
            deferred_compactions: value.deferred_compactions,
        }
    }
}

pub fn recover_startup_candidate(
    store: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
) -> Result<StartupRecoverySnapshot, crate::cas_projection::ProjectionCoordinatorError> {
    crate::cas_projection::accepted_delivery_recovery::recover_startup_candidate(store, storage)
        .map(Into::into)
}

pub fn recover_startup(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
) -> Result<StartupRecoverySnapshot, crate::cas_projection::ProjectionCoordinatorError> {
    let generation = store
        .health()
        .generation()
        .ok_or(crate::cas_projection::ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead)?;
    crate::cas_projection::accepted_delivery_recovery::recover_startup(
        store,
        store.home_id(),
        generation,
        storage,
    )
    .map(Into::into)
}

pub fn converge_compaction_restart_candidate(
    store: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
) -> Result<(), crate::cas_projection::ProjectionCoordinatorError> {
    crate::cas_projection::accepted_delivery_recovery::converge_compaction_restart_candidate(
        store, storage, thread, turn,
    )
}

pub fn converge_compaction_restart(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    turn: beryl_model::SyndicTurnId,
) -> Result<(), crate::cas_projection::ProjectionCoordinatorError> {
    crate::cas_projection::accepted_delivery_recovery::converge_compaction_restart(
        store, storage, thread, turn,
    )
}

pub fn abandon_active_candidate(
    store: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
    request: &AbandonActiveBinding,
) -> Result<BindingRevision, ProjectionPublicationFailure> {
    publication::abandon_active_candidate(store, storage, request)
}

pub fn abandon_stop_candidate(
    store: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
    request: &AbandonStopOperation,
) -> Result<(), ProjectionPublicationFailure> {
    publication::abandon_stop_candidate(store, storage, request)
}
