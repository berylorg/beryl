use beryl_model::*;

use crate::*;

use super::{keys::*, parts::*, *};

mod binding;
mod compaction;
mod content;
mod core_record;
mod input_gate;
mod projection;
pub(super) mod projection_build;
mod provider;
mod query;
mod route;
mod source;
mod stop;
mod thread_properties;

use binding::*;
pub(crate) use compaction::*;
pub(crate) use content::*;
use core_record::*;
pub(crate) use core_record::{decode_draft_record, encode_draft_record};
pub(crate) use input_gate::*;
use projection::*;
use projection_build::*;
pub(crate) use provider::*;
pub(crate) use query::*;
pub(crate) use route::*;
use source::*;
pub(crate) use stop::*;
use thread_properties::*;

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
    SMALL_MAX,
    beryl_home_store::RecordVersion::new(2)
);
id_family!(
    ImageLabelAuthorityHeadsFamily,
    ImageLabelAuthorityHeadsCodec,
    "image-label-authority-heads",
    SyndicThreadId,
    ImageLabelAuthorityHeadV1,
    |b| key16(
        b,
        "image-label authority head key",
        SyndicThreadId::from_bytes
    ),
    encode_image_label_authority_head,
    decode_image_label_authority_head,
    SMALL_MAX
);
id_family!(
    DraftImageLabelProtectionHeadsFamily,
    DraftImageLabelProtectionHeadsCodec,
    "draft-image-label-protection-heads",
    SyndicThreadId,
    DraftImageLabelProtectionHeadV1,
    |b| key16(
        b,
        "draft image-label protection head key",
        SyndicThreadId::from_bytes
    ),
    encode_draft_image_label_protection_head,
    decode_draft_image_label_protection_head,
    SMALL_MAX
);
id_family!(
    ThreadExecutionsFamily,
    ThreadExecutionsCodec,
    "thread-executions",
    SyndicThreadId,
    ThreadExecutionRecord,
    |b| key16(b, "thread-execution key", SyndicThreadId::from_bytes),
    encode_thread_execution,
    decode_thread_execution,
    SMALL_MAX
);
id_family!(
    ThreadAttributesFamily,
    ThreadAttributesCodec,
    "thread-attributes",
    SyndicThreadId,
    ThreadAttributesRecord,
    |b| key16(b, "thread-attributes key", SyndicThreadId::from_bytes),
    encode_thread_attributes,
    decode_thread_attributes,
    SMALL_MAX
);
id_family!(
    ThreadUsageFamily,
    ThreadUsageCodec,
    "thread-usage",
    SyndicThreadId,
    ThreadUsageRecord,
    |b| key16(b, "thread-usage key", SyndicThreadId::from_bytes),
    encode_thread_usage,
    decode_thread_usage,
    SMALL_MAX
);
id_family!(
    ThreadCatalogSummariesFamily,
    ThreadCatalogSummariesCodec,
    "thread-catalog-summaries",
    SyndicThreadId,
    ThreadCatalogSummaryRecord,
    |b| key16(b, "thread-catalog-summary key", SyndicThreadId::from_bytes),
    encode_thread_catalog_summary,
    decode_thread_catalog_summary,
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
    SMALL_MAX,
    beryl_home_store::RecordVersion::new(2)
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
    SMALL_MAX,
    beryl_home_store::RecordVersion::new(3)
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
    SMALL_MAX,
    beryl_home_store::RecordVersion::new(2)
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
    SMALL_MAX,
    beryl_home_store::RecordVersion::new(3)
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
    SMALL_MAX,
    beryl_home_store::RecordVersion::new(2)
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
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
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
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
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
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
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
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
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
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(3);
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
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
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
