use beryl_home_store::HomeCandidateRecoveryAccess;
use beryl_model::BindingRevision;
use syndic_storage::{AbandonActiveBinding, AbandonStopOperation, SyndicStorage};

use crate::cas_projection::{ProjectionPublicationFailure, publication};

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
