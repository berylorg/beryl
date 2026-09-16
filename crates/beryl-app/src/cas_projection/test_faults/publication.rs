use beryl_home_store::HomeCandidateRecoveryAccess;
use beryl_model::BindingRevision;
use syndic_storage::{AbandonActiveBinding, AbandonStopOperation, SyndicStorage};

use crate::cas_projection::{ProjectionPublicationFailure, publication};

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
