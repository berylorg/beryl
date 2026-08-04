use std::time::{SystemTime, UNIX_EPOCH};

use beryl_model::SyndicExecutionSnapshotId;
use sha2::{Digest, Sha256};
use syndic_storage::SyndicTimestamp;

use crate::cas_projection::ordinary::{
    OrdinaryTurnExecutionError, preflight::PendingOrdinaryExecution,
};
use crate::cas_projection::{CasProjectionCoordinator, LoadedCasProjection};

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

pub(super) fn system_timestamp_at_least(
    minimum: SyndicTimestamp,
) -> Result<SyndicTimestamp, OrdinaryTurnExecutionError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(OrdinaryTurnExecutionError::SystemClockBeforeUnixEpoch)?;
    let millis = u64::try_from(elapsed.as_millis())
        .map_err(|_| OrdinaryTurnExecutionError::SystemClockOutOfRange)?;
    Ok(SyndicTimestamp::from_unix_millis(millis).max(minimum))
}
