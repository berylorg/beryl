mod cas;
mod content;
mod projection;
mod provider;
mod query;
mod transcript;

pub(crate) use cas::*;
pub(crate) use content::*;
pub(crate) use projection::*;
pub(crate) use provider::*;
pub(crate) use query::*;
pub(crate) use transcript::*;

use beryl_model::*;

use crate::{
    AcceptedInputOrdinal, AcceptedRouteGeneration, ContentChunkOrdinal, ItemProjectionGeneration,
    ItemSourceEventOrdinal, ProjectionOrdinal, ResourceOrdinal, SourceEventSequence,
    TranscriptGeneration, TranscriptPosition, TurnDepth, TurnItemOrdinal,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadRouteKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) generation: AcceptedRouteGeneration,
}

impl ScanKey for ThreadRouteKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: AcceptedRouteGeneration::FIRST,
        }
    }
    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: AcceptedRouteGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ThreadRouteKey {
    pub(crate) fn encode(self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        e.u64(self.generation.get());
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = Self {
            thread: dec_thread(&mut d)?,
            generation: AcceptedRouteGeneration::new(d.u64()?)
                .map_err(|source| invalid("accepted-route generation key", source))?,
        };
        d.finish()?;
        Ok(value)
    }
}

use super::{CodecError, invalid, parts::*};

pub(crate) trait ScanKey: Clone {
    fn first() -> Self;
    fn last() -> Self;
}

macro_rules! id_scan_key {
    ($ty:ty) => {
        impl ScanKey for $ty {
            fn first() -> Self {
                Self::from_bytes([0; 16])
            }
            fn last() -> Self {
                Self::from_bytes([u8::MAX; 16])
            }
        }
    };
}
id_scan_key!(SyndicThreadId);
id_scan_key!(SyndicDraftId);
id_scan_key!(SyndicContentId);
id_scan_key!(SyndicTurnId);
id_scan_key!(SyndicAcceptedInputId);
id_scan_key!(SyndicItemId);
id_scan_key!(SyndicProjectionId);
id_scan_key!(SyndicResourceId);
id_scan_key!(SyndicExecutionSnapshotId);
id_scan_key!(ProviderObservationId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextOwnerKey {
    Draft(SyndicDraftId),
    Turn(SyndicTurnId),
}
impl From<DiscussionContextOwnerId> for ContextOwnerKey {
    fn from(value: DiscussionContextOwnerId) -> Self {
        match value {
            DiscussionContextOwnerId::Draft(id) => Self::Draft(id),
            DiscussionContextOwnerId::SubmittedTurn(id) => Self::Turn(id),
        }
    }
}
impl From<ContextOwnerKey> for DiscussionContextOwnerId {
    fn from(value: ContextOwnerKey) -> Self {
        match value {
            ContextOwnerKey::Draft(id) => Self::Draft(id),
            ContextOwnerKey::Turn(id) => Self::SubmittedTurn(id),
        }
    }
}
impl ScanKey for ContextOwnerKey {
    fn first() -> Self {
        Self::Draft(SyndicDraftId::from_bytes([0; 16]))
    }
    fn last() -> Self {
        Self::Turn(SyndicTurnId::from_bytes([u8::MAX; 16]))
    }
}

pub(crate) fn enc_context_key(key: &ContextOwnerKey) -> Vec<u8> {
    let mut e = Encoder::new();
    match key {
        ContextOwnerKey::Draft(id) => {
            e.u8(0);
            enc_draft(&mut e, *id);
        }
        ContextOwnerKey::Turn(id) => {
            e.u8(1);
            enc_turn(&mut e, *id);
        }
    }
    e.finish()
}
pub(crate) fn dec_context_key(encoded: &[u8]) -> Result<ContextOwnerKey, CodecError> {
    let mut d = Decoder::new(encoded);
    let value = match d.u8()? {
        0 => ContextOwnerKey::Draft(dec_draft(&mut d)?),
        1 => ContextOwnerKey::Turn(dec_turn(&mut d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "context-owner key",
                tag,
            });
        }
    };
    d.finish()?;
    Ok(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadPairKey {
    pub(crate) first: SyndicThreadId,
    pub(crate) second: SyndicThreadId,
}
impl ScanKey for ThreadPairKey {
    fn first() -> Self {
        Self {
            first: SyndicThreadId::from_bytes([0; 16]),
            second: SyndicThreadId::from_bytes([0; 16]),
        }
    }
    fn last() -> Self {
        Self {
            first: SyndicThreadId::from_bytes([u8::MAX; 16]),
            second: SyndicThreadId::from_bytes([u8::MAX; 16]),
        }
    }
}
pub(crate) fn enc_thread_pair(key: &ThreadPairKey) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_thread(&mut e, key.first);
    enc_thread(&mut e, key.second);
    e.finish()
}
pub(crate) fn dec_thread_pair(bytes: &[u8]) -> Result<ThreadPairKey, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = ThreadPairKey {
        first: dec_thread(&mut d)?,
        second: dec_thread(&mut d)?,
    };
    d.finish()?;
    Ok(key)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TurnPairKey {
    pub(crate) parent: SyndicTurnId,
    pub(crate) child: SyndicTurnId,
}
impl ScanKey for TurnPairKey {
    fn first() -> Self {
        Self {
            parent: SyndicTurnId::from_bytes([0; 16]),
            child: SyndicTurnId::from_bytes([0; 16]),
        }
    }
    fn last() -> Self {
        Self {
            parent: SyndicTurnId::from_bytes([u8::MAX; 16]),
            child: SyndicTurnId::from_bytes([u8::MAX; 16]),
        }
    }
}
pub(crate) fn enc_turn_pair(key: &TurnPairKey) -> Vec<u8> {
    let mut e = Encoder::new();
    enc_turn(&mut e, key.parent);
    enc_turn(&mut e, key.child);
    e.finish()
}
pub(crate) fn dec_turn_pair(bytes: &[u8]) -> Result<TurnPairKey, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = TurnPairKey {
        parent: dec_turn(&mut d)?,
        child: dec_turn(&mut d)?,
    };
    d.finish()?;
    Ok(key)
}

macro_rules! owner_ordinal_key {
    ($name:ident, $owner:ty, $ordinal:ty, $enc_owner:ident, $dec_owner:ident, $enc_ord:ident, $dec_ord:ident, $first_owner:expr, $last_owner:expr) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) struct $name {
            pub(crate) owner: $owner,
            pub(crate) ordinal: $ordinal,
        }
        impl ScanKey for $name {
            fn first() -> Self {
                Self {
                    owner: $first_owner,
                    ordinal: <$ordinal>::FIRST,
                }
            }
            fn last() -> Self {
                Self {
                    owner: $last_owner,
                    ordinal: <$ordinal>::new(u64::MAX).expect("maximum is nonzero"),
                }
            }
        }
        impl $name {
            pub(crate) fn encode(&self) -> Vec<u8> {
                let mut e = Encoder::new();
                $enc_owner(&mut e, self.owner);
                $enc_ord(&mut e, self.ordinal);
                e.finish()
            }
            pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
                let mut d = Decoder::new(bytes);
                let key = Self {
                    owner: $dec_owner(&mut d)?,
                    ordinal: $dec_ord(&mut d)?,
                };
                d.finish()?;
                Ok(key)
            }
        }
    };
}
owner_ordinal_key!(
    ContentChunkKey,
    SyndicContentId,
    ContentChunkOrdinal,
    enc_content,
    dec_content,
    enc_content_chunk_ord,
    dec_content_chunk_ord,
    SyndicContentId::from_bytes([0; 16]),
    SyndicContentId::from_bytes([u8::MAX; 16])
);
owner_ordinal_key!(
    ThreadAcceptedKey,
    SyndicThreadId,
    AcceptedInputOrdinal,
    enc_thread,
    dec_thread,
    enc_accepted_ord,
    dec_accepted_ord,
    SyndicThreadId::from_bytes([0; 16]),
    SyndicThreadId::from_bytes([u8::MAX; 16])
);
owner_ordinal_key!(
    TurnEventKey,
    SyndicTurnId,
    SourceEventSequence,
    enc_turn,
    dec_turn,
    enc_source_seq,
    dec_source_seq,
    SyndicTurnId::from_bytes([0; 16]),
    SyndicTurnId::from_bytes([u8::MAX; 16])
);
owner_ordinal_key!(
    TurnItemKey,
    SyndicTurnId,
    TurnItemOrdinal,
    enc_turn,
    dec_turn,
    enc_item_ord,
    dec_item_ord,
    SyndicTurnId::from_bytes([0; 16]),
    SyndicTurnId::from_bytes([u8::MAX; 16])
);
owner_ordinal_key!(
    ItemEventKey,
    SyndicItemId,
    ItemSourceEventOrdinal,
    enc_item,
    dec_item,
    enc_item_event_ord,
    dec_item_event_ord,
    SyndicItemId::from_bytes([0; 16]),
    SyndicItemId::from_bytes([u8::MAX; 16])
);
owner_ordinal_key!(
    ProjectionResourceKey,
    SyndicProjectionId,
    ResourceOrdinal,
    enc_projection,
    dec_projection,
    enc_resource_ord,
    dec_resource_ord,
    SyndicProjectionId::from_bytes([0; 16]),
    SyndicProjectionId::from_bytes([u8::MAX; 16])
);
