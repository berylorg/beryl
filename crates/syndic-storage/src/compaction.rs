use beryl_model::{
    BerylHomeId, BindingRevision, CasLoadedSessionGeneration, CasThreadId, InputGateRevision,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicTurnId,
};
use sha2::{Digest, Sha256};

use crate::{
    CasRepresentedPrefixProof, CompactionOperationId, ComposerAtom, ComposerPayload,
    PreparedContent, SyndicRecordError,
};

const COMPACTION_SNAPSHOT_V1: &[u8] = b"beryl.syndic.compaction-snapshot.v1";
const LIFECYCLE_CONTINUATION_TURN_V1: &[u8] = b"beryl.syndic.lifecycle-continuation.turn.v1";
const LIFECYCLE_CONTINUATION_ITEM_V1: &[u8] = b"beryl.syndic.lifecycle-continuation.item.v1";

/// Exact Beryl-owned lifecycle continuation text.
pub const LIFECYCLE_CONTINUATION_TEXT: &str = "Continue from the root doc/plan.md.";

/// Prepares the one-atom ownerless Composer V1 content required by lifecycle settlement.
pub fn prepare_lifecycle_continuation_content() -> Result<PreparedContent, SyndicRecordError> {
    PreparedContent::composer(&ComposerPayload::new(vec![ComposerAtom::text(
        LIFECYCLE_CONTINUATION_TEXT,
    )?])?)
}

/// Derives the exact V1 provider-operation execution-snapshot identity.
///
/// Variable-width CAS identity bytes are length-prefixed; all numeric fields use big endian.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn derive_compaction_snapshot_id(
    home_id: BerylHomeId,
    operation_id: CompactionOperationId,
    turn_id: SyndicTurnId,
    source_gate_revision: InputGateRevision,
    binding_revision: BindingRevision,
    represented_prefix: CasRepresentedPrefixProof,
    cas_thread_id: &CasThreadId,
    loaded_generation: CasLoadedSessionGeneration,
) -> SyndicExecutionSnapshotId {
    let mut hash = Sha256::new();
    hash.update(COMPACTION_SNAPSHOT_V1);
    hash.update(home_id.as_bytes());
    hash.update(operation_id.thread_id().as_bytes());
    hash.update(operation_id.nonce().as_bytes());
    hash.update(turn_id.as_bytes());
    hash.update(source_gate_revision.get().to_be_bytes());
    hash.update(binding_revision.get().to_be_bytes());
    match represented_prefix.tail() {
        Some(tail) => {
            hash.update([1]);
            hash.update(tail.as_bytes());
        }
        None => hash.update([0]),
    }
    hash.update(
        represented_prefix
            .source_thread_revision()
            .get()
            .to_be_bytes(),
    );
    hash.update(represented_prefix.digest().as_bytes());
    let cas_thread = cas_thread_id.as_str().as_bytes();
    hash.update((cas_thread.len() as u64).to_be_bytes());
    hash.update(cas_thread);
    hash.update(loaded_generation.process().get().to_be_bytes());
    hash.update(loaded_generation.thread().get().to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0; 16];
    identity.copy_from_slice(&digest[..16]);
    SyndicExecutionSnapshotId::from_bytes(identity)
}

/// Derives the exact lifecycle-continuation conversation turn identity.
#[must_use]
pub fn derive_lifecycle_continuation_turn_id(
    home_id: BerylHomeId,
    operation_id: CompactionOperationId,
    content_digest: beryl_model::SyndicContentDigest,
) -> SyndicTurnId {
    SyndicTurnId::from_bytes(derive_continuation_identity(
        LIFECYCLE_CONTINUATION_TURN_V1,
        home_id,
        operation_id,
        content_digest,
    ))
}

/// Derives the exact lifecycle-continuation canonical user-item identity.
#[must_use]
pub fn derive_lifecycle_continuation_item_id(
    home_id: BerylHomeId,
    operation_id: CompactionOperationId,
    content_digest: beryl_model::SyndicContentDigest,
) -> SyndicItemId {
    SyndicItemId::from_bytes(derive_continuation_identity(
        LIFECYCLE_CONTINUATION_ITEM_V1,
        home_id,
        operation_id,
        content_digest,
    ))
}

fn derive_continuation_identity(
    domain: &[u8],
    home_id: BerylHomeId,
    operation_id: CompactionOperationId,
    content_digest: beryl_model::SyndicContentDigest,
) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(home_id.as_bytes());
    hash.update(operation_id.thread_id().as_bytes());
    hash.update(operation_id.nonce().as_bytes());
    hash.update(content_digest.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0; 16];
    identity.copy_from_slice(&digest[..16]);
    identity
}
