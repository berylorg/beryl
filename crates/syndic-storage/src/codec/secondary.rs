mod cas;
mod cas_family;
mod content;
mod provider;
mod query;
mod route;

use beryl_model::*;
use cas::*;
pub(crate) use cas_family::*;
pub(crate) use content::*;
pub(crate) use provider::*;
pub(crate) use query::*;
pub(crate) use route::*;

use crate::*;

use super::primary::projection_build::{decode_transcript_path_turn, encode_transcript_path_turn};
use super::{keys::*, parts::*, *};

macro_rules! fixed_family {
    ($marker:ident,$alias:ident,$name:literal,$version:expr,$key:ty,$value:ty,$max_key:expr,$enc_key:expr,$dec_key:expr,$enc_value:ident,$dec_value:ident) => {
        pub(crate) struct $marker;
        pub(crate) type $alias = ExactCodec<$marker>;
        impl Family for $marker {
            type Key = $key;
            type Value = $value;
            const NAME: &'static str = $name;
            const RECORD_VERSION: beryl_home_store::RecordVersion = $version;
            const MAX_KEY_BYTES: usize = $max_key;
            const MAX_VALUE_BYTES: usize = SMALL_MAX;
            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
                Ok($enc_key(key))
            }
            fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
                $dec_key(bytes)
            }
            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
                $enc_value(value)
            }
            fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
                $dec_value(bytes)
            }
        }
    };
}
fn thread_key(bytes: &[u8]) -> Result<SyndicThreadId, CodecError> {
    let value: [u8; 16] = bytes
        .try_into()
        .map_err(|_| CodecError::InvalidLength("thread index key"))?;
    Ok(SyndicThreadId::from_bytes(value))
}
fixed_family!(
    DraftByThreadFamily,
    DraftByThreadCodec,
    "draft-by-thread",
    beryl_home_store::RecordVersion::new(1),
    SyndicThreadId,
    DraftByThreadRecord,
    16,
    |k: &SyndicThreadId| k.as_bytes().to_vec(),
    thread_key,
    encode_draft_index,
    decode_draft_index
);
fixed_family!(
    ThreadParentFamily,
    ThreadParentCodec,
    "thread-parent-index",
    beryl_home_store::RecordVersion::new(1),
    ThreadPairKey,
    ThreadParentIndexRecord,
    32,
    |k: &ThreadPairKey| enc_thread_pair(k),
    dec_thread_pair,
    encode_thread_parent,
    decode_thread_parent
);
fixed_family!(
    TurnChildrenFamily,
    TurnChildrenCodec,
    "turn-children",
    beryl_home_store::RecordVersion::new(1),
    TurnPairKey,
    TurnChildIndexRecord,
    32,
    |k: &TurnPairKey| enc_turn_pair(k),
    dec_turn_pair,
    encode_turn_child,
    decode_turn_child
);
fixed_family!(
    AcceptedOrderFamily,
    AcceptedOrderCodec,
    "accepted-order",
    beryl_home_store::RecordVersion::new(2),
    ThreadAcceptedKey,
    AcceptedOrderIndexRecord,
    24,
    |k: &ThreadAcceptedKey| k.encode(),
    ThreadAcceptedKey::decode,
    encode_accepted_order,
    decode_accepted_order
);
fixed_family!(
    TurnItemsFamily,
    TurnItemsCodec,
    "turn-items",
    beryl_home_store::RecordVersion::new(1),
    TurnItemKey,
    TurnItemIndexRecord,
    24,
    |k: &TurnItemKey| k.encode(),
    TurnItemKey::decode,
    encode_turn_item,
    decode_turn_item
);
fixed_family!(
    ItemSourceEventsFamily,
    ItemSourceEventsCodec,
    "item-source-events",
    beryl_home_store::RecordVersion::new(1),
    ItemEventKey,
    ItemSourceEventIndexRecord,
    24,
    |k: &ItemEventKey| k.encode(),
    ItemEventKey::decode,
    encode_item_source_event,
    decode_item_source_event
);
fixed_family!(
    TranscriptPathTurnsFamily,
    TranscriptPathTurnsCodec,
    "transcript-path-turns",
    beryl_home_store::RecordVersion::new(1),
    ThreadTranscriptPathKey,
    TranscriptPathTurnRecord,
    32,
    |k: &ThreadTranscriptPathKey| k.encode(),
    ThreadTranscriptPathKey::decode,
    encode_transcript_path_turn,
    decode_transcript_path_turn
);
fixed_family!(
    TranscriptEntriesFamily,
    TranscriptEntriesCodec,
    "transcript-view-entries",
    beryl_home_store::RecordVersion::new(1),
    ThreadTranscriptKey,
    TranscriptViewEntryRecord,
    32,
    |k: &ThreadTranscriptKey| k.encode(),
    ThreadTranscriptKey::decode,
    encode_transcript_entry,
    decode_transcript_entry
);
fixed_family!(
    StableItemProjectionsFamily,
    StableItemProjectionsCodec,
    "stable-item-projections",
    beryl_home_store::RecordVersion::new(1),
    StableItemProjectionKey,
    StableItemProjectionIndexRecord,
    24,
    |k: &StableItemProjectionKey| k.encode(),
    StableItemProjectionKey::decode,
    encode_stable_item_projection,
    decode_stable_item_projection
);
fixed_family!(
    ItemProjectionsFamily,
    ItemProjectionsCodec,
    "item-projections",
    beryl_home_store::RecordVersion::new(1),
    ItemProjectionKey,
    ItemProjectionIndexRecord,
    32,
    |k: &ItemProjectionKey| k.encode(),
    ItemProjectionKey::decode,
    encode_item_projection,
    decode_item_projection
);
fixed_family!(
    ProjectionResourcesFamily,
    ProjectionResourcesCodec,
    "projection-resources",
    beryl_home_store::RecordVersion::new(1),
    ProjectionResourceKey,
    ProjectionResourceIndexRecord,
    24,
    |k: &ProjectionResourceKey| k.encode(),
    ProjectionResourceKey::decode,
    encode_projection_resource,
    decode_projection_resource
);
fixed_family!(
    BindingHeadsFamily,
    BindingHeadsCodec,
    "binding-heads",
    beryl_home_store::RecordVersion::new(1),
    SyndicThreadId,
    BindingHeadRecord,
    16,
    |k: &SyndicThreadId| k.as_bytes().to_vec(),
    thread_key,
    encode_binding_head,
    decode_binding_head
);

fn encode_draft_index(v: &DraftByThreadRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, v.thread_id);
    enc_draft(&mut e, v.draft_id);
    enc_draft_rev(&mut e, v.draft_revision);
    enc_thread_rev(&mut e, v.thread_revision);
    Ok(e.finish())
}
fn decode_draft_index(b: &[u8]) -> Result<DraftByThreadRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = DraftByThreadRecord::new(
        dec_thread(&mut d)?,
        dec_draft(&mut d)?,
        dec_draft_rev(&mut d)?,
        dec_thread_rev(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_thread_parent(v: &ThreadParentIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, v.parent_thread_id);
    enc_thread(&mut e, v.child_thread_id);
    enc_thread_rev(&mut e, v.child_revision);
    enc_context_owner(&mut e, v.context_owner_id);
    Ok(e.finish())
}
fn decode_thread_parent(b: &[u8]) -> Result<ThreadParentIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = ThreadParentIndexRecord::new(
        dec_thread(&mut d)?,
        dec_thread(&mut d)?,
        dec_thread_rev(&mut d)?,
        dec_context_owner(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_turn_child(v: &TurnChildIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_turn(&mut e, v.parent_id);
    enc_turn(&mut e, v.child_id);
    enc_turn_depth(&mut e, v.child_depth);
    enc_path_digest(&mut e, v.child_digest);
    Ok(e.finish())
}
fn decode_turn_child(b: &[u8]) -> Result<TurnChildIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = TurnChildIndexRecord::new(
        dec_turn(&mut d)?,
        dec_turn(&mut d)?,
        dec_turn_depth(&mut d)?,
        dec_path_digest(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_accepted_order(v: &AcceptedOrderIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, v.thread_id);
    enc_accepted_ord(&mut e, v.ordinal);
    enc_accepted(&mut e, v.input_id);
    enc_route_generation(&mut e, v.route_generation);
    Ok(e.finish())
}
fn decode_accepted_order(b: &[u8]) -> Result<AcceptedOrderIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = AcceptedOrderIndexRecord::new(
        dec_thread(&mut d)?,
        dec_accepted_ord(&mut d)?,
        dec_accepted(&mut d)?,
        dec_route_generation(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_turn_item(v: &TurnItemIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_turn(&mut e, v.turn_id);
    enc_item_ord(&mut e, v.ordinal);
    enc_item(&mut e, v.item_id);
    enc_projection_rev(&mut e, v.item_revision);
    Ok(e.finish())
}
fn decode_turn_item(b: &[u8]) -> Result<TurnItemIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = TurnItemIndexRecord::new(
        dec_turn(&mut d)?,
        dec_item_ord(&mut d)?,
        dec_item(&mut d)?,
        dec_projection_rev(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_item_source_event(v: &ItemSourceEventIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, v.item_id);
    enc_item_event_ord(&mut e, v.ordinal);
    enc_turn(&mut e, v.turn_id);
    enc_source_seq(&mut e, v.source_event);
    Ok(e.finish())
}
fn decode_item_source_event(b: &[u8]) -> Result<ItemSourceEventIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = ItemSourceEventIndexRecord::new(
        dec_item(&mut d)?,
        dec_item_event_ord(&mut d)?,
        dec_turn(&mut d)?,
        dec_source_seq(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_transcript_entry(v: &TranscriptViewEntryRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, v.thread_id);
    enc_transcript_generation(&mut e, v.generation);
    enc_transcript_pos(&mut e, v.position);
    enc_item(&mut e, v.item_id);
    enc_projection_rev(&mut e, v.item_revision);
    enc_item_projection_generation(&mut e, v.item_projection_generation);
    enc_projection(&mut e, v.projection_id);
    enc_projection_rev(&mut e, v.projection_revision);
    Ok(e.finish())
}
fn decode_transcript_entry(b: &[u8]) -> Result<TranscriptViewEntryRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = TranscriptViewEntryRecord::new(
        dec_thread(&mut d)?,
        dec_transcript_generation(&mut d)?,
        dec_transcript_pos(&mut d)?,
        dec_item(&mut d)?,
        dec_projection_rev(&mut d)?,
        dec_item_projection_generation(&mut d)?,
        dec_projection(&mut d)?,
        dec_projection_rev(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_item_projection(v: &ItemProjectionIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, v.item_id);
    enc_item_projection_generation(&mut e, v.generation);
    enc_projection_ord(&mut e, v.ordinal);
    enc_projection(&mut e, v.projection_id);
    enc_projection_rev(&mut e, v.projection_revision);
    Ok(e.finish())
}
fn encode_stable_item_projection(
    v: &StableItemProjectionIndexRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, v.item_id);
    enc_projection_ord(&mut e, v.ordinal);
    enc_projection(&mut e, v.projection_id);
    enc_projection_rev(&mut e, v.projection_revision);
    Ok(e.finish())
}
fn decode_stable_item_projection(b: &[u8]) -> Result<StableItemProjectionIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = StableItemProjectionIndexRecord::new(
        dec_item(&mut d)?,
        dec_projection_ord(&mut d)?,
        dec_projection(&mut d)?,
        dec_projection_rev(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn decode_item_projection(b: &[u8]) -> Result<ItemProjectionIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = ItemProjectionIndexRecord::new(
        dec_item(&mut d)?,
        dec_item_projection_generation(&mut d)?,
        dec_projection_ord(&mut d)?,
        dec_projection(&mut d)?,
        dec_projection_rev(&mut d)?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_projection_resource(v: &ProjectionResourceIndexRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_projection(&mut e, v.projection_id);
    enc_resource_ord(&mut e, v.ordinal);
    enc_resource(&mut e, v.resource_id);
    enc_projection_rev(&mut e, v.resource_revision);
    e.fixed32(&v.resource_digest);
    Ok(e.finish())
}
fn decode_projection_resource(b: &[u8]) -> Result<ProjectionResourceIndexRecord, CodecError> {
    let mut d = Decoder::new(b);
    let v = ProjectionResourceIndexRecord::new(
        dec_projection(&mut d)?,
        dec_resource_ord(&mut d)?,
        dec_resource(&mut d)?,
        dec_projection_rev(&mut d)?,
        d.fixed32()?,
    );
    d.finish()?;
    Ok(v)
}
fn encode_binding_head(v: &BindingHeadRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, v.thread_id);
    enc_binding_rev(&mut e, v.revision);
    e.u8(match v.lifecycle {
        BindingLifecycle::Unbound => 0,
        BindingLifecycle::Valid => 1,
        BindingLifecycle::Active => 2,
        BindingLifecycle::Stale => 3,
    });
    enc_path_digest(&mut e, v.selected_path_digest);
    Ok(e.finish())
}
fn decode_binding_head(b: &[u8]) -> Result<BindingHeadRecord, CodecError> {
    let mut d = Decoder::new(b);
    let thread = dec_thread(&mut d)?;
    let rev = dec_binding_rev(&mut d)?;
    let lifecycle = match d.u8()? {
        0 => BindingLifecycle::Unbound,
        1 => BindingLifecycle::Valid,
        2 => BindingLifecycle::Active,
        3 => BindingLifecycle::Stale,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "binding-head lifecycle",
                tag,
            });
        }
    };
    let v = BindingHeadRecord::new(thread, rev, lifecycle, dec_path_digest(&mut d)?);
    d.finish()?;
    Ok(v)
}
