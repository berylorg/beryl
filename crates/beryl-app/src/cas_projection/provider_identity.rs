use beryl_model::{CasItemId, SyndicContentId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use sha2::{Digest, Sha256};
use syndic_storage::CasTurnSource;

pub(super) fn syndic_item_id(
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    source: &CasTurnSource,
    cas_item_id: &CasItemId,
) -> SyndicItemId {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.live-item.v1");
    hash.update(thread_id.as_bytes());
    hash.update(turn_id.as_bytes());
    hash.update(source.thread_id().as_str().as_bytes());
    hash.update([0]);
    hash.update(source.turn_id().as_str().as_bytes());
    hash.update([0]);
    hash.update(cas_item_id.as_str().as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    SyndicItemId::from_bytes(identity)
}

pub(super) fn provider_content_id(
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    source: &CasTurnSource,
    item_id: SyndicItemId,
) -> SyndicContentId {
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.provider-content.v1");
    hash.update(thread_id.as_bytes());
    hash.update(turn_id.as_bytes());
    hash.update(source.thread_id().as_str().as_bytes());
    hash.update([0]);
    hash.update(source.turn_id().as_str().as_bytes());
    hash.update([0]);
    hash.update(item_id.as_bytes());
    SyndicContentId::from_digest(hash.finalize().into())
}
