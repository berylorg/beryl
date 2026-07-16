use beryl_model::SyndicExecutionSnapshotId;
use sha2::{Digest, Sha256};
use syndic_storage::{StaleCasBinding, SyndicPointReadLimit, SyndicTimestamp};

use crate::cas_projection::ordinary::{
    OrdinaryTurnExecutionError, preflight::PendingOrdinaryExecution,
};
use crate::cas_projection::{CasProjectionCoordinator, LoadedCasProjection};

const POINT_READ_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_BYTES)
        .expect("ordinary execution point-read bound is nonzero")
}

pub(super) fn execution_snapshot_id(
    coordinator: &CasProjectionCoordinator,
    projection: &LoadedCasProjection,
    pending: &PendingOrdinaryExecution,
) -> SyndicExecutionSnapshotId {
    let loaded = projection.loaded_session_generation();
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.execution-snapshot.v1");
    hash.update(coordinator.home_id().as_bytes());
    hash.update(coordinator.home_generation().get().to_be_bytes());
    hash.update(pending.thread_id.as_bytes());
    hash.update(pending.turn_id.as_bytes());
    hash.update(pending.item_id.as_bytes());
    hash.update(pending.binding_revision.get().to_be_bytes());
    hash.update(pending.gate_revision.get().to_be_bytes());
    hash.update(pending.selected_path.thread_revision().get().to_be_bytes());
    hash.update(pending.selected_path.digest().as_bytes());
    hash.update(projection.cas_thread_id().as_str().as_bytes());
    hash.update(loaded.process().get().to_be_bytes());
    hash.update(loaded.thread().get().to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    SyndicExecutionSnapshotId::from_bytes(identity)
}

pub(super) fn stale_binding(
    projection: &LoadedCasProjection,
    pending: &PendingOrdinaryExecution,
    observed_at: SyndicTimestamp,
) -> Result<StaleCasBinding, OrdinaryTurnExecutionError> {
    Ok(StaleCasBinding::new(
        projection.execution_binding().clone(),
        projection.cas_thread_id().clone(),
        Some(pending.tool_profile),
        Some(pending.represented_prefix),
        Some(pending.lineage),
        Some(pending.native_turn_count),
        Some(projection.loaded_session_generation()),
        "ordinary turn lost live CAS projection authority",
        observed_at,
    )?)
}
