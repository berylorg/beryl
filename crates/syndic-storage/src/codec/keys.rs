mod cas;
mod content;

pub(crate) use cas::*;
pub(crate) use content::*;

use beryl_model::*;

use crate::{
    AcceptedInputOrdinal, ContentChunkOrdinal, InputMarkerOrdinal, InputMarkerOwner,
    ItemProjectionGeneration, ItemSourceEventOrdinal, ProjectionOrdinal, ResourceOrdinal,
    SourceEventSequence, TranscriptGeneration, TranscriptPosition, TurnDepth, TurnItemOrdinal,
};

use super::{CodecError, parts::*};

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputMarkerKey {
    pub(crate) owner: InputMarkerOwner,
    pub(crate) ordinal: InputMarkerOrdinal,
}

impl ScanKey for InputMarkerKey {
    fn first() -> Self {
        Self {
            owner: InputMarkerOwner::AcceptedInput(SyndicAcceptedInputId::from_bytes([0; 16])),
            ordinal: InputMarkerOrdinal::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            owner: InputMarkerOwner::CanonicalItem(SyndicItemId::from_bytes([u8::MAX; 16])),
            ordinal: InputMarkerOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl InputMarkerKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_input_marker_owner(&mut e, self.owner);
        enc_input_marker_ord(&mut e, self.ordinal);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            owner: dec_input_marker_owner(&mut d)?,
            ordinal: dec_input_marker_ord(&mut d)?,
        };
        d.finish()?;
        Ok(key)
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
impl ThreadAcceptedKey {
    pub(crate) fn first_for_thread(owner: SyndicThreadId) -> Self {
        Self {
            owner,
            ordinal: AcceptedInputOrdinal::FIRST,
        }
    }
    pub(crate) fn last_for_thread(owner: SyndicThreadId) -> Self {
        Self {
            owner,
            ordinal: AcceptedInputOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ItemProjectionSetKey {
    pub(crate) item: SyndicItemId,
    pub(crate) generation: ItemProjectionGeneration,
}

impl ScanKey for ItemProjectionSetKey {
    fn first() -> Self {
        Self {
            item: SyndicItemId::from_bytes([0; 16]),
            generation: ItemProjectionGeneration::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            item: SyndicItemId::from_bytes([u8::MAX; 16]),
            generation: ItemProjectionGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ItemProjectionSetKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_item(&mut e, self.item);
        enc_item_projection_generation(&mut e, self.generation);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            item: dec_item(&mut d)?,
            generation: dec_item_projection_generation(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }

    pub(crate) fn first_for_item(item: SyndicItemId) -> Self {
        Self {
            item,
            generation: ItemProjectionGeneration::FIRST,
        }
    }

    pub(crate) fn last_for_item(item: SyndicItemId) -> Self {
        Self {
            item,
            generation: ItemProjectionGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StableItemProjectionKey {
    pub(crate) item: SyndicItemId,
    pub(crate) ordinal: ProjectionOrdinal,
}

impl ScanKey for StableItemProjectionKey {
    fn first() -> Self {
        Self {
            item: SyndicItemId::from_bytes([0; 16]),
            ordinal: ProjectionOrdinal::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            item: SyndicItemId::from_bytes([u8::MAX; 16]),
            ordinal: ProjectionOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl StableItemProjectionKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_item(&mut e, self.item);
        enc_projection_ord(&mut e, self.ordinal);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            item: dec_item(&mut d)?,
            ordinal: dec_projection_ord(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ItemProjectionKey {
    pub(crate) item: SyndicItemId,
    pub(crate) generation: ItemProjectionGeneration,
    pub(crate) ordinal: ProjectionOrdinal,
}

impl ScanKey for ItemProjectionKey {
    fn first() -> Self {
        Self {
            item: SyndicItemId::from_bytes([0; 16]),
            generation: ItemProjectionGeneration::FIRST,
            ordinal: ProjectionOrdinal::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            item: SyndicItemId::from_bytes([u8::MAX; 16]),
            generation: ItemProjectionGeneration::new(u64::MAX).expect("maximum is nonzero"),
            ordinal: ProjectionOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ItemProjectionKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_item(&mut e, self.item);
        enc_item_projection_generation(&mut e, self.generation);
        enc_projection_ord(&mut e, self.ordinal);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            item: dec_item(&mut d)?,
            generation: dec_item_projection_generation(&mut d)?,
            ordinal: dec_projection_ord(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadTranscriptBuildKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) generation: TranscriptGeneration,
}

impl ScanKey for ThreadTranscriptBuildKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: TranscriptGeneration::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ThreadTranscriptBuildKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_transcript_generation(&mut e, self.generation);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            generation: dec_transcript_generation(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }

    pub(crate) fn first_for_thread(thread: SyndicThreadId) -> Self {
        Self {
            thread,
            generation: TranscriptGeneration::FIRST,
        }
    }

    pub(crate) fn last_for_thread(thread: SyndicThreadId) -> Self {
        Self {
            thread,
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadTranscriptPathKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) generation: TranscriptGeneration,
    pub(crate) depth: TurnDepth,
}

impl ScanKey for ThreadTranscriptPathKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: TranscriptGeneration::FIRST,
            depth: TurnDepth::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
            depth: TurnDepth::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ThreadTranscriptPathKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_transcript_generation(&mut e, self.generation);
        enc_turn_depth(&mut e, self.depth);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            generation: dec_transcript_generation(&mut d)?,
            depth: dec_turn_depth(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThreadTranscriptKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) generation: TranscriptGeneration,
    pub(crate) position: TranscriptPosition,
}

impl ScanKey for ThreadTranscriptKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            generation: TranscriptGeneration::FIRST,
            position: TranscriptPosition::FIRST,
        }
    }

    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            generation: TranscriptGeneration::new(u64::MAX).expect("maximum is nonzero"),
            position: TranscriptPosition::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

impl ThreadTranscriptKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_transcript_generation(&mut e, self.generation);
        enc_transcript_pos(&mut e, self.position);
        e.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            generation: dec_transcript_generation(&mut d)?,
            position: dec_transcript_pos(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SteeringKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) turn: SyndicTurnId,
    pub(crate) ordinal: AcceptedInputOrdinal,
}
impl ScanKey for SteeringKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            turn: SyndicTurnId::from_bytes([0; 16]),
            ordinal: AcceptedInputOrdinal::FIRST,
        }
    }
    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            turn: SyndicTurnId::from_bytes([u8::MAX; 16]),
            ordinal: AcceptedInputOrdinal::new(u64::MAX).expect("nonzero"),
        }
    }
}
impl SteeringKey {
    pub(crate) fn first_for_thread(thread: SyndicThreadId) -> Self {
        Self {
            thread,
            turn: SyndicTurnId::from_bytes([0; 16]),
            ordinal: AcceptedInputOrdinal::FIRST,
        }
    }
    pub(crate) fn last_for_thread(thread: SyndicThreadId) -> Self {
        Self {
            thread,
            turn: SyndicTurnId::from_bytes([u8::MAX; 16]),
            ordinal: AcceptedInputOrdinal::new(u64::MAX).expect("nonzero"),
        }
    }
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_turn(&mut e, self.turn);
        enc_accepted_ord(&mut e, self.ordinal);
        e.finish()
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            turn: dec_turn(&mut d)?,
            ordinal: dec_accepted_ord(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BindingKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) revision: BindingRevision,
}
impl ScanKey for BindingKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            revision: BindingRevision::new(1).expect("nonzero"),
        }
    }
    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            revision: BindingRevision::new(u64::MAX).expect("nonzero"),
        }
    }
}
impl BindingKey {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut e = Encoder::new();
        enc_thread(&mut e, self.thread);
        enc_binding_rev(&mut e, self.revision);
        e.finish()
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut d = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut d)?,
            revision: dec_binding_rev(&mut d)?,
        };
        d.finish()?;
        Ok(key)
    }
}
