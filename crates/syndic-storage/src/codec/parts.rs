use beryl_model::*;

use super::{CodecError, invalid};

mod projection;
mod provider;
mod provider_observation;
mod provider_observation_header;
mod value;
pub(crate) use projection::*;
pub(crate) use provider::*;
pub(crate) use provider_observation::*;
pub(crate) use provider_observation_header::*;
pub(crate) use value::*;

pub(crate) struct Encoder {
    bytes: Vec<u8>,
}
impl Encoder {
    pub(crate) fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }
    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }
    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(crate) fn fixed16(&mut self, value: &[u8; 16]) {
        self.bytes.extend_from_slice(value);
    }
    pub(crate) fn fixed32(&mut self, value: &[u8; 32]) {
        self.bytes.extend_from_slice(value);
    }
    pub(crate) fn text(&mut self, value: &str) {
        self.u32(u32::try_from(value.len()).expect("bounded text fits u32"));
        self.bytes.extend_from_slice(value.as_bytes());
    }
    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.u32(u32::try_from(value.len()).expect("bounded bytes fit u32"));
        self.bytes.extend_from_slice(value);
    }
}

pub(crate) struct Decoder<'a> {
    remaining: &'a [u8],
}
impl<'a> Decoder<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }
    pub(crate) fn finish(self) -> Result<(), CodecError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(CodecError::TrailingBytes)
        }
    }
    pub(crate) fn take(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        if self.remaining.len() < length {
            return Err(CodecError::Truncated);
        }
        let (value, rest) = self.remaining.split_at(length);
        self.remaining = rest;
        Ok(value)
    }
    pub(crate) fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| CodecError::Truncated)?,
        ))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| CodecError::Truncated)?,
        ))
    }
    pub(crate) fn fixed16(&mut self) -> Result<[u8; 16], CodecError> {
        self.take(16)?.try_into().map_err(|_| CodecError::Truncated)
    }
    pub(crate) fn fixed32(&mut self) -> Result<[u8; 32], CodecError> {
        self.take(32)?.try_into().map_err(|_| CodecError::Truncated)
    }
    pub(crate) fn text(&mut self, kind: &'static str) -> Result<&'a str, CodecError> {
        let len = usize::try_from(self.u32()?).map_err(|_| CodecError::InvalidLength(kind))?;
        std::str::from_utf8(self.take(len)?).map_err(|_| CodecError::InvalidUtf8(kind))
    }
    pub(crate) fn bytes(&mut self, kind: &'static str) -> Result<&'a [u8], CodecError> {
        let len = usize::try_from(self.u32()?).map_err(|_| CodecError::InvalidLength(kind))?;
        self.take(len)
    }
}

macro_rules! id_helpers {
    ($encode:ident, $decode:ident, $ty:ty, $kind:literal) => {
        pub(crate) fn $encode(e: &mut Encoder, value: $ty) {
            e.fixed16(value.as_bytes());
        }
        pub(crate) fn $decode(d: &mut Decoder<'_>) -> Result<$ty, CodecError> {
            Ok(<$ty>::from_bytes(d.fixed16()?))
        }
    };
}
id_helpers!(enc_thread, dec_thread, SyndicThreadId, "thread id");
id_helpers!(enc_draft, dec_draft, SyndicDraftId, "draft id");
id_helpers!(enc_content, dec_content, SyndicContentId, "content id");
id_helpers!(enc_turn, dec_turn, SyndicTurnId, "turn id");
id_helpers!(enc_item, dec_item, SyndicItemId, "item id");
id_helpers!(
    enc_accepted,
    dec_accepted,
    SyndicAcceptedInputId,
    "accepted input id"
);
id_helpers!(
    enc_projection,
    dec_projection,
    SyndicProjectionId,
    "projection id"
);
id_helpers!(enc_resource, dec_resource, SyndicResourceId, "resource id");
id_helpers!(
    enc_snapshot,
    dec_snapshot,
    SyndicExecutionSnapshotId,
    "snapshot id"
);
id_helpers!(enc_marker, dec_marker, SyndicDraftMarkerId, "marker id");

macro_rules! revision_helpers {
    ($encode:ident, $decode:ident, $ty:ty, $kind:literal) => {
        pub(crate) fn $encode(e: &mut Encoder, value: $ty) {
            e.u64(value.get());
        }
        pub(crate) fn $decode(d: &mut Decoder<'_>) -> Result<$ty, CodecError> {
            <$ty>::new(d.u64()?).map_err(|source| invalid($kind, source))
        }
    };
}
revision_helpers!(
    enc_thread_rev,
    dec_thread_rev,
    ThreadRevision,
    "thread revision"
);
revision_helpers!(
    enc_draft_rev,
    dec_draft_rev,
    DraftRevision,
    "draft revision"
);
revision_helpers!(
    enc_content_rev,
    dec_content_rev,
    ContentRevision,
    "content revision"
);
revision_helpers!(
    enc_binding_rev,
    dec_binding_rev,
    BindingRevision,
    "binding revision"
);
revision_helpers!(
    enc_accepted_rev,
    dec_accepted_rev,
    AcceptedInputRevision,
    "accepted-input revision"
);
revision_helpers!(
    enc_input_gate_rev,
    dec_input_gate_rev,
    InputGateRevision,
    "input-gate revision"
);
revision_helpers!(
    enc_projection_rev,
    dec_projection_rev,
    ProjectionRevision,
    "projection revision"
);

pub(crate) fn enc_external(e: &mut Encoder, value: &str) {
    e.text(value);
}
pub(crate) fn dec_cas_thread(d: &mut Decoder<'_>) -> Result<CasThreadId, CodecError> {
    CasThreadId::new(d.text("CAS thread id")?).map_err(|source| invalid("CAS thread id", source))
}
pub(crate) fn dec_cas_turn(d: &mut Decoder<'_>) -> Result<CasTurnId, CodecError> {
    CasTurnId::new(d.text("CAS turn id")?).map_err(|source| invalid("CAS turn id", source))
}
pub(crate) fn dec_cas_item(d: &mut Decoder<'_>) -> Result<CasItemId, CodecError> {
    CasItemId::new(d.text("CAS item id")?).map_err(|source| invalid("CAS item id", source))
}

pub(crate) fn enc_opt<T>(e: &mut Encoder, value: Option<T>, encode: impl FnOnce(&mut Encoder, T)) {
    match value {
        Some(value) => {
            e.u8(1);
            encode(e, value);
        }
        None => e.u8(0),
    }
}
pub(crate) fn dec_opt<T>(
    d: &mut Decoder<'_>,
    kind: &'static str,
    decode: impl FnOnce(&mut Decoder<'_>) -> Result<T, CodecError>,
) -> Result<Option<T>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => decode(d).map(Some),
        tag => Err(CodecError::InvalidTag { kind, tag }),
    }
}

pub(crate) fn enc_parent(e: &mut Encoder, value: crate::ConversationParent) {
    match value {
        crate::ConversationParent::Root => e.u8(0),
        crate::ConversationParent::Turn(id) => {
            e.u8(1);
            enc_turn(e, id);
        }
    }
}
pub(crate) fn dec_parent(d: &mut Decoder<'_>) -> Result<crate::ConversationParent, CodecError> {
    match d.u8()? {
        0 => Ok(crate::ConversationParent::Root),
        1 => dec_turn(d).map(crate::ConversationParent::Turn),
        tag => Err(CodecError::InvalidTag {
            kind: "conversation parent",
            tag,
        }),
    }
}

pub(crate) fn enc_context_owner(e: &mut Encoder, value: DiscussionContextOwnerId) {
    match value {
        DiscussionContextOwnerId::Draft(id) => {
            e.u8(0);
            enc_draft(e, id);
        }
        DiscussionContextOwnerId::SubmittedTurn(id) => {
            e.u8(1);
            enc_turn(e, id);
        }
    }
}
pub(crate) fn dec_context_owner(
    d: &mut Decoder<'_>,
) -> Result<DiscussionContextOwnerId, CodecError> {
    match d.u8()? {
        0 => dec_draft(d).map(DiscussionContextOwnerId::Draft),
        1 => dec_turn(d).map(DiscussionContextOwnerId::SubmittedTurn),
        tag => Err(CodecError::InvalidTag {
            kind: "context owner",
            tag,
        }),
    }
}

pub(crate) fn enc_path_digest(e: &mut Encoder, value: SyndicPathDigest) {
    e.fixed32(value.as_bytes());
}
pub(crate) fn dec_path_digest(d: &mut Decoder<'_>) -> Result<SyndicPathDigest, CodecError> {
    Ok(SyndicPathDigest::from_bytes(d.fixed32()?))
}

pub(crate) fn enc_native_turn_count(e: &mut Encoder, value: CasNativeTurnCount) {
    e.u64(value.get());
}

pub(crate) fn dec_native_turn_count(d: &mut Decoder<'_>) -> Result<CasNativeTurnCount, CodecError> {
    Ok(CasNativeTurnCount::new(d.u64()?))
}

pub(crate) fn enc_tool_profile(e: &mut Encoder, value: CasConversationToolProfile) {
    e.u8(value.version() as u8);
    e.fixed32(&value.digest());
}

pub(crate) fn dec_tool_profile(
    d: &mut Decoder<'_>,
) -> Result<CasConversationToolProfile, CodecError> {
    match d.u8()? {
        1 => Ok(CasConversationToolProfile::v1(d.fixed32()?)),
        tag => Err(CodecError::InvalidTag {
            kind: "CAS conversation-tool profile version",
            tag,
        }),
    }
}

pub(crate) fn enc_timestamp(e: &mut Encoder, value: crate::SyndicTimestamp) {
    e.u64(value.unix_millis());
}
pub(crate) fn dec_timestamp(d: &mut Decoder<'_>) -> Result<crate::SyndicTimestamp, CodecError> {
    Ok(crate::SyndicTimestamp::from_unix_millis(d.u64()?))
}

pub(crate) fn enc_bool(e: &mut Encoder, value: bool) {
    e.u8(u8::from(value));
}
pub(crate) fn dec_bool(d: &mut Decoder<'_>, kind: &'static str) -> Result<bool, CodecError> {
    match d.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        tag => Err(CodecError::InvalidTag { kind, tag }),
    }
}

macro_rules! ordinal_helpers {
    ($encode:ident, $decode:ident, $ty:ty, $kind:literal) => {
        pub(crate) fn $encode(e: &mut Encoder, value: $ty) {
            e.u64(value.get());
        }
        pub(crate) fn $decode(d: &mut Decoder<'_>) -> Result<$ty, CodecError> {
            <$ty>::new(d.u64()?).map_err(|source| invalid($kind, source))
        }
    };
}
ordinal_helpers!(
    enc_content_chunk_ord,
    dec_content_chunk_ord,
    crate::ContentChunkOrdinal,
    "content-chunk ordinal"
);
ordinal_helpers!(
    enc_accepted_ord,
    dec_accepted_ord,
    crate::AcceptedInputOrdinal,
    "accepted-input ordinal"
);
ordinal_helpers!(
    enc_image_label,
    dec_image_label,
    crate::ImageLabelOrdinal,
    "image-label ordinal"
);
ordinal_helpers!(
    enc_input_marker_ord,
    dec_input_marker_ord,
    crate::InputMarkerOrdinal,
    "input-marker ordinal"
);
ordinal_helpers!(
    enc_source_seq,
    dec_source_seq,
    crate::SourceEventSequence,
    "source-event sequence"
);
ordinal_helpers!(
    enc_transcript_pos,
    dec_transcript_pos,
    crate::TranscriptPosition,
    "transcript position"
);
ordinal_helpers!(
    enc_transcript_generation,
    dec_transcript_generation,
    crate::TranscriptGeneration,
    "transcript generation"
);
ordinal_helpers!(
    enc_item_projection_generation,
    dec_item_projection_generation,
    crate::ItemProjectionGeneration,
    "item-projection generation"
);
ordinal_helpers!(
    enc_turn_depth,
    dec_turn_depth,
    crate::TurnDepth,
    "turn depth"
);
ordinal_helpers!(
    enc_thread_lineage_depth,
    dec_thread_lineage_depth,
    crate::ThreadLineageDepth,
    "thread-lineage depth"
);
ordinal_helpers!(
    enc_activity_query_revision,
    dec_activity_query_revision,
    crate::ActivityQueryRevision,
    "activity-query revision"
);
ordinal_helpers!(
    enc_activity_work_period,
    dec_activity_work_period,
    crate::ActivityWorkPeriod,
    "activity work period"
);

pub(crate) fn enc_image_label_frontier(e: &mut Encoder, value: crate::ImageLabelFrontier) {
    e.u64(value.get());
}

pub(crate) fn dec_image_label_frontier(
    d: &mut Decoder<'_>,
) -> Result<crate::ImageLabelFrontier, CodecError> {
    Ok(crate::ImageLabelFrontier::from_raw(d.u64()?))
}
ordinal_helpers!(
    enc_turn_state_rev,
    dec_turn_state_rev,
    crate::TurnStateRevision,
    "turn-state revision"
);
ordinal_helpers!(
    enc_context_rev,
    dec_context_rev,
    crate::ContextEnvelopeRevision,
    "context revision"
);
ordinal_helpers!(
    enc_item_ord,
    dec_item_ord,
    crate::TurnItemOrdinal,
    "item ordinal"
);
ordinal_helpers!(
    enc_item_event_ord,
    dec_item_event_ord,
    crate::ItemSourceEventOrdinal,
    "item source-event ordinal"
);
ordinal_helpers!(
    enc_content_piece_ord,
    dec_content_piece_ord,
    crate::ContentPieceOrdinal,
    "content piece ordinal"
);
ordinal_helpers!(
    enc_composer_atom_ord,
    dec_composer_atom_ord,
    crate::ComposerAtomOrdinal,
    "composer atom ordinal"
);
ordinal_helpers!(
    enc_projection_ord,
    dec_projection_ord,
    crate::ProjectionOrdinal,
    "projection ordinal"
);
ordinal_helpers!(
    enc_resource_ord,
    dec_resource_ord,
    crate::ResourceOrdinal,
    "resource ordinal"
);
