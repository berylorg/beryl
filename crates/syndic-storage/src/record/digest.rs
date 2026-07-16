use beryl_model::{SyndicPathDigest, SyndicTurnId};
use sha2::{Digest, Sha256};

const EMPTY_PATH_V1: &[u8] = b"syndic:selected-path:empty:v1";
const ROOT_TURN_V1: &[u8] = b"syndic:turn-chain:root:v1";
const CHILD_TURN_V1: &[u8] = b"syndic:turn-chain:child:v1";

/// Canonical selected-path digest for a thread with no committed turn.
#[must_use]
pub fn empty_selected_path_digest() -> SyndicPathDigest {
    SyndicPathDigest::from_bytes(Sha256::digest(EMPTY_PATH_V1).into())
}

/// Canonical V1 chain digest for one root turn.
#[must_use]
pub fn root_turn_chain_digest(turn_id: SyndicTurnId) -> SyndicPathDigest {
    let mut digest = Sha256::new();
    digest.update(ROOT_TURN_V1);
    digest.update(turn_id.as_bytes());
    SyndicPathDigest::from_bytes(digest.finalize().into())
}

/// Canonical V1 chain digest for one child and its exact parent proof.
#[must_use]
pub fn child_turn_chain_digest(
    child_id: SyndicTurnId,
    parent_id: SyndicTurnId,
    parent_digest: SyndicPathDigest,
) -> SyndicPathDigest {
    let mut digest = Sha256::new();
    digest.update(CHILD_TURN_V1);
    digest.update(child_id.as_bytes());
    digest.update(parent_id.as_bytes());
    digest.update(parent_digest.as_bytes());
    SyndicPathDigest::from_bytes(digest.finalize().into())
}
