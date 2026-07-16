use beryl_model::*;

use crate::*;

use super::{keys::*, parts::*, *};

mod binding;
mod content;
mod input_gate;
mod projection;
pub(super) mod projection_build;
mod source;

use binding::*;
pub(crate) use content::*;
pub(crate) use input_gate::*;
use projection::*;
use projection_build::*;
use source::*;

macro_rules! id_family {
    ($marker:ident,$alias:ident,$name:literal,$key:ty,$value:ty,$decode_key:expr,$encode_value:ident,$decode_value:ident,$max:expr) => {
        id_family!(
            $marker,
            $alias,
            $name,
            $key,
            $value,
            $decode_key,
            $encode_value,
            $decode_value,
            $max,
            beryl_home_store::RecordVersion::new(1)
        );
    };
    ($marker:ident,$alias:ident,$name:literal,$key:ty,$value:ty,$decode_key:expr,$encode_value:ident,$decode_value:ident,$max:expr,$version:expr) => {
        pub(crate) struct $marker;
        pub(crate) type $alias = ExactCodec<$marker>;
        impl Family for $marker {
            type Key = $key;
            type Value = $value;
            const NAME: &'static str = $name;
            const RECORD_VERSION: beryl_home_store::RecordVersion = $version;
            const MAX_KEY_BYTES: usize = 16;
            const MAX_VALUE_BYTES: usize = $max;
            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
                Ok(key.as_bytes().to_vec())
            }
            fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
                $decode_key(encoded)
            }
            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
                $encode_value(value)
            }
            fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
                $decode_value(encoded)
            }
        }
    };
}

fn key16<T>(
    encoded: &[u8],
    kind: &'static str,
    make: impl FnOnce([u8; 16]) -> T,
) -> Result<T, CodecError> {
    let bytes: [u8; 16] = encoded
        .try_into()
        .map_err(|_| CodecError::InvalidLength(kind))?;
    Ok(make(bytes))
}

id_family!(
    ThreadsFamily,
    ThreadsCodec,
    "threads",
    SyndicThreadId,
    ThreadRecord,
    |b| key16(b, "thread key", SyndicThreadId::from_bytes),
    encode_thread_record,
    decode_thread_record,
    SMALL_MAX
);
id_family!(
    DraftsFamily,
    DraftsCodec,
    "drafts",
    SyndicDraftId,
    DraftRecord,
    |b| key16(b, "draft key", SyndicDraftId::from_bytes),
    encode_draft_record,
    decode_draft_record,
    SMALL_MAX
);
id_family!(
    TurnsFamily,
    TurnsCodec,
    "turns",
    SyndicTurnId,
    TurnRecord,
    |b| key16(b, "turn key", SyndicTurnId::from_bytes),
    encode_turn_record,
    decode_turn_record,
    SMALL_MAX
);
id_family!(
    TurnStatesFamily,
    TurnStatesCodec,
    "turn-states",
    SyndicTurnId,
    TurnStateRecord,
    |b| key16(b, "turn-state key", SyndicTurnId::from_bytes),
    encode_turn_state,
    decode_turn_state,
    SMALL_MAX
);
id_family!(
    AcceptedInputsFamily,
    AcceptedInputsCodec,
    "accepted-inputs",
    SyndicAcceptedInputId,
    AcceptedInputRecord,
    |b| key16(b, "accepted-input key", SyndicAcceptedInputId::from_bytes),
    encode_accepted_input,
    decode_accepted_input,
    SMALL_MAX
);
id_family!(
    CanonicalItemsFamily,
    CanonicalItemsCodec,
    "canonical-items",
    SyndicItemId,
    CanonicalItemRecord,
    |b| key16(b, "canonical-item key", SyndicItemId::from_bytes),
    encode_canonical_item,
    decode_canonical_item,
    SMALL_MAX,
    beryl_home_store::RecordVersion::new(2)
);
id_family!(
    ItemProjectionHeadsFamily,
    ItemProjectionHeadsCodec,
    "item-projection-heads",
    SyndicItemId,
    ItemProjectionHeadRecord,
    |b| key16(b, "item-projection head key", SyndicItemId::from_bytes),
    encode_item_projection_head,
    decode_item_projection_head,
    SMALL_MAX
);
id_family!(
    TranscriptHeadsFamily,
    TranscriptHeadsCodec,
    "transcript-view-heads",
    SyndicThreadId,
    TranscriptViewHeadRecord,
    |b| key16(b, "transcript-head key", SyndicThreadId::from_bytes),
    encode_transcript_head,
    decode_transcript_head,
    SMALL_MAX
);
id_family!(
    ProjectionsFamily,
    ProjectionsCodec,
    "projections",
    SyndicProjectionId,
    ProjectionRecord,
    |b| key16(b, "projection key", SyndicProjectionId::from_bytes),
    encode_projection_record,
    decode_projection_record,
    SMALL_MAX + 4096
);
id_family!(
    ResourcesFamily,
    ResourcesCodec,
    "resources",
    SyndicResourceId,
    ResourceMetadataRecord,
    |b| key16(b, "resource key", SyndicResourceId::from_bytes),
    encode_resource_record,
    decode_resource_record,
    SMALL_MAX
);
id_family!(
    HistorySummariesFamily,
    HistorySummariesCodec,
    "history-summaries",
    SyndicThreadId,
    HistorySummaryRecord,
    |b| key16(b, "history-summary key", SyndicThreadId::from_bytes),
    encode_history_summary,
    decode_history_summary,
    SMALL_MAX
);
id_family!(
    ExecutionSnapshotsFamily,
    ExecutionSnapshotsCodec,
    "execution-snapshots",
    SyndicExecutionSnapshotId,
    ExecutionSnapshotRecord,
    |b| key16(
        b,
        "execution-snapshot key",
        SyndicExecutionSnapshotId::from_bytes
    ),
    encode_execution_snapshot,
    decode_execution_snapshot,
    SMALL_MAX
);
id_family!(
    ActiveCasTurnsFamily,
    ActiveCasTurnsCodec,
    "active-cas-turns",
    SyndicExecutionSnapshotId,
    ActiveCasTurnRecord,
    |b| key16(
        b,
        "active-CAS-turn snapshot key",
        SyndicExecutionSnapshotId::from_bytes
    ),
    encode_active_cas_turn,
    decode_active_cas_turn,
    SMALL_MAX
);

pub(crate) struct ItemProjectionSetsFamily;
pub(crate) type ItemProjectionSetsCodec = ExactCodec<ItemProjectionSetsFamily>;
impl Family for ItemProjectionSetsFamily {
    type Key = ItemProjectionSetKey;
    type Value = ItemProjectionSetRecord;
    const NAME: &'static str = "item-projection-sets";
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ItemProjectionSetKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_item_projection_set(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_item_projection_set(encoded)
    }
}

pub(crate) struct ItemProjectionBuildsFamily;
pub(crate) type ItemProjectionBuildsCodec = ExactCodec<ItemProjectionBuildsFamily>;
impl Family for ItemProjectionBuildsFamily {
    type Key = ItemProjectionSetKey;
    type Value = ItemProjectionBuildRecord;
    const NAME: &'static str = "item-projection-builds";
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ItemProjectionSetKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_item_projection_build(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_item_projection_build(encoded)
    }
}

pub(crate) struct TranscriptBuildsFamily;
pub(crate) type TranscriptBuildsCodec = ExactCodec<TranscriptBuildsFamily>;
impl Family for TranscriptBuildsFamily {
    type Key = ThreadTranscriptBuildKey;
    type Value = TranscriptBuildRecord;
    const NAME: &'static str = "transcript-builds";
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ThreadTranscriptBuildKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_transcript_build(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_transcript_build(encoded)
    }
}

pub(crate) struct ContextEnvelopesFamily;
pub(crate) type ContextEnvelopesCodec = ExactCodec<ContextEnvelopesFamily>;
impl Family for ContextEnvelopesFamily {
    type Key = ContextOwnerKey;
    type Value = ContextEnvelopeRecord;
    const NAME: &'static str = "context-envelopes";
    const MAX_KEY_BYTES: usize = 17;
    const MAX_VALUE_BYTES: usize = SMALL_MAX + 512;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(enc_context_key(key))
    }
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        dec_context_key(encoded)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_context_record(value)
    }
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_context_record(encoded)
    }
}

pub(crate) struct SourceEventsFamily;
pub(crate) type SourceEventsCodec = ExactCodec<SourceEventsFamily>;
impl Family for SourceEventsFamily {
    type Key = TurnEventKey;
    type Value = SourceEventRecord;
    const NAME: &'static str = "source-events";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(2);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = LARGE_MAX;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        TurnEventKey::decode(encoded)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_source_event(value)
    }
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_source_event(encoded)
    }
}

pub(crate) struct BindingsFamily;
pub(crate) type BindingsCodec = ExactCodec<BindingsFamily>;
impl Family for BindingsFamily {
    type Key = BindingKey;
    type Value = BindingRecord;
    const NAME: &'static str = "bindings";
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;
    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        BindingKey::decode(encoded)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_binding_record(value)
    }
    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_binding_record(encoded)
    }
}

fn encode_thread_record(value: &ThreadRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_thread(&mut e, value.id());
    enc_thread_rev(&mut e, value.revision());
    enc_opt(&mut e, value.committed_tail(), enc_turn);
    enc_draft(&mut e, value.current_draft_id());
    enc_opt(&mut e, value.parent_thread_id(), enc_thread);
    enc_opt(&mut e, value.context_owner_id(), enc_context_owner);
    enc_path_digest(&mut e, value.selected_path_digest());
    Ok(e.finish())
}
fn decode_thread_record(bytes: &[u8]) -> Result<ThreadRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ThreadRecord::new(
        dec_thread(&mut d)?,
        dec_thread_rev(&mut d)?,
        dec_opt(&mut d, "committed tail", dec_turn)?,
        dec_draft(&mut d)?,
        dec_opt(&mut d, "parent thread", dec_thread)?,
        dec_opt(&mut d, "context owner", dec_context_owner)?,
        dec_path_digest(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn encode_draft_record(value: &DraftRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_draft(&mut e, value.id());
    enc_thread(&mut e, value.thread_id());
    enc_draft_rev(&mut e, value.revision());
    enc_parent(&mut e, value.parent());
    enc_opt(&mut e, value.context_owner_id(), enc_context_owner);
    enc_opt(&mut e, value.replacement_edit_intent(), |e, intent| {
        enc_turn(e, intent.target_turn_id());
        enc_selected_path(e, intent.selected_path());
        enc_transcript_generation(e, intent.transcript_entry().generation());
        enc_transcript_pos(e, intent.transcript_entry().position());
    });
    enc_content_ref(&mut e, value.content());
    enc_timestamp(&mut e, value.created_at());
    enc_timestamp(&mut e, value.updated_at());
    Ok(e.finish())
}
fn decode_draft_record(bytes: &[u8]) -> Result<DraftRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = DraftRecord::new(
        dec_draft(&mut d)?,
        dec_thread(&mut d)?,
        dec_draft_rev(&mut d)?,
        dec_parent(&mut d)?,
        dec_opt(&mut d, "draft context owner", dec_context_owner)?,
        dec_opt(&mut d, "replacement edit intent", |d| {
            Ok(ReplacementEditIntent::new(
                dec_turn(d)?,
                dec_selected_path(d)?,
                CurrentTranscriptEntryProof::new(
                    dec_transcript_generation(d)?,
                    dec_transcript_pos(d)?,
                ),
            ))
        })?,
        dec_content_ref(&mut d)?,
        dec_timestamp(&mut d)?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn encode_context_record(value: &ContextEnvelopeRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_context_owner(&mut e, value.owner());
    enc_context_rev(&mut e, value.revision());
    enc_context_envelope(&mut e, value.envelope());
    Ok(e.finish())
}
fn decode_context_record(bytes: &[u8]) -> Result<ContextEnvelopeRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = ContextEnvelopeRecord::new(
        dec_context_owner(&mut d)?,
        dec_context_rev(&mut d)?,
        dec_context_envelope(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn encode_turn_record(value: &TurnRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_turn(&mut e, value.id());
    enc_thread(&mut e, value.origin_thread_id());
    enc_turn_kind(&mut e, value.kind());
    enc_parent(&mut e, value.parent());
    enc_opt(&mut e, value.ancestor_skip(), enc_turn);
    enc_turn_depth(&mut e, value.depth());
    enc_path_digest(&mut e, value.chain_digest());
    enc_timestamp(&mut e, value.submitted_at());
    Ok(e.finish())
}
fn decode_turn_record(bytes: &[u8]) -> Result<TurnRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = TurnRecord::new(
        dec_turn(&mut d)?,
        dec_thread(&mut d)?,
        dec_turn_kind(&mut d)?,
        dec_parent(&mut d)?,
        dec_opt(&mut d, "turn ancestor skip", dec_turn)?,
        dec_turn_depth(&mut d)?,
        dec_path_digest(&mut d)?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}

fn encode_turn_state(value: &TurnStateRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_turn(&mut e, value.turn_id());
    enc_turn_state_rev(&mut e, value.revision());
    enc_turn_lifecycle(&mut e, value.lifecycle());
    e.u64(value.source_event_count());
    e.u64(value.item_count());
    e.u64(value.finalized_item_count());
    e.u64(value.open_item_count());
    e.u64(value.history_blocking_item_count());
    enc_opt(&mut e, value.end_status(), enc_turn_end_status);
    enc_timestamp(&mut e, value.updated_at());
    Ok(e.finish())
}
fn decode_turn_state(bytes: &[u8]) -> Result<TurnStateRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = TurnStateRecord::with_capture_frontiers(
        dec_turn(&mut d)?,
        dec_turn_state_rev(&mut d)?,
        dec_turn_lifecycle(&mut d)?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        dec_opt(&mut d, "turn end status", dec_turn_end_status)?,
        dec_timestamp(&mut d)?,
    )
    .map_err(|source| invalid("turn state", source))?;
    d.finish()?;
    Ok(value)
}

fn encode_accepted_input(value: &AcceptedInputRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_accepted(&mut e, value.id());
    enc_thread(&mut e, value.thread_id());
    enc_accepted_rev(&mut e, value.revision());
    enc_accepted_ord(&mut e, value.ordinal());
    enc_input_gate_rev(&mut e, value.gate_revision());
    enc_accepted_disposition(&mut e, value.disposition());
    enc_accepted_lifecycle(&mut e, value.lifecycle());
    enc_content_ref(&mut e, value.content());
    e.u64(value.marker_count());
    enc_timestamp(&mut e, value.admitted_at());
    Ok(e.finish())
}
fn decode_accepted_input(bytes: &[u8]) -> Result<AcceptedInputRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = AcceptedInputRecord::new(
        dec_accepted(&mut d)?,
        dec_thread(&mut d)?,
        dec_accepted_rev(&mut d)?,
        dec_accepted_ord(&mut d)?,
        dec_input_gate_rev(&mut d)?,
        dec_accepted_disposition(&mut d)?,
        dec_accepted_lifecycle(&mut d)?,
        dec_content_ref(&mut d)?,
        d.u64()?,
        dec_timestamp(&mut d)?,
    );
    d.finish()?;
    Ok(value)
}
