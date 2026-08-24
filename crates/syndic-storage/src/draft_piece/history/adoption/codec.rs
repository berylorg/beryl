use beryl_home_store::RecordVersion;
use beryl_model::SyndicDraftId;

use crate::codec::{
    CodecError, ExactCodec, Family,
    parts::{Decoder, Encoder},
};

use super::super::super::{
    DraftEditorCandidateSessionIdV1, DraftPieceOperationIdV1, DraftPieceRootRecordV1, dec_position,
    dec_root_reference, dec_session_head, enc_position, enc_root_reference, enc_session_head,
};
use super::super::{
    dec_history_frontier, dec_history_transition, enc_history_frontier, enc_history_transition,
};
use super::model::*;

pub(crate) struct DraftHistoricalRootAdoptionsFamily;
pub(crate) type DraftHistoricalRootAdoptionsCodec = ExactCodec<DraftHistoricalRootAdoptionsFamily>;

impl Family for DraftHistoricalRootAdoptionsFamily {
    type Key = DraftHistoricalRootAdoptionKeyV1;
    type Value = DraftHistoricalRootAdoptionV1;
    const NAME: &'static str = "draft-historical-root-adoptions";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 48;
    const MAX_VALUE_BYTES: usize = 65_536;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(encode_key(*key))
    }

    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let key = decode_key(&mut decoder)?;
        decoder.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        if !value.is_locally_valid()
            || value.request_bytes()
                != canonical_historical_root_adoption_request_bytes(value.request())
        {
            return Err(CodecError::InvalidLength("draft historical-root adoption"));
        }
        let mut encoder = Encoder::new();
        enc_request(&mut encoder, value.request());
        encoder.bytes(value.request_bytes());
        enc_history_frontier(&mut encoder, value.source_history());
        enc_history_transition(&mut encoder, value.selected_transition());
        enc_root_reference(&mut encoder, value.target_root().reference());
        enc_outcome(&mut encoder, value.outcome());
        enc_optional_transition(&mut encoder, value.successor_transition());
        enc_optional_history(&mut encoder, value.successor_history());
        enc_optional_session(&mut encoder, value.successor_candidate());
        Ok(encoder.finish())
    }

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let request = dec_request(&mut decoder)?;
        let request_bytes = decoder.bytes("draft historical-root request")?.to_vec();
        let source_history = Box::new(dec_history_frontier(&mut decoder)?);
        let selected_transition = Box::new(dec_history_transition(&mut decoder)?);
        let target_root = Box::new(DraftPieceRootRecordV1::new(dec_root_reference(
            &mut decoder,
        )?));
        let outcome = dec_outcome(&mut decoder)?;
        let successor_transition = dec_optional_transition(&mut decoder)?;
        let successor_history = dec_optional_history(&mut decoder)?;
        let successor_candidate = dec_optional_session(&mut decoder)?;
        decoder.finish()?;
        let value = DraftHistoricalRootAdoptionV1::new(
            request,
            request_bytes,
            source_history,
            selected_transition,
            target_root,
            outcome,
            successor_transition,
            successor_history,
            successor_candidate,
        );
        if !value.is_locally_valid()
            || value.request_bytes() != canonical_historical_root_adoption_request_bytes(request)
        {
            return Err(CodecError::InvalidLength("draft historical-root adoption"));
        }
        Ok(value)
    }
}

pub(crate) fn canonical_historical_root_adoption_request_bytes(
    request: DraftHistoricalRootAdoptionRequestV1,
) -> Vec<u8> {
    let mut encoder = Encoder::new();
    enc_request(&mut encoder, request);
    encoder.finish()
}

fn encode_key(key: DraftHistoricalRootAdoptionKeyV1) -> Vec<u8> {
    let mut encoder = Encoder::new();
    enc_key(&mut encoder, key);
    encoder.finish()
}

fn enc_key(encoder: &mut Encoder, key: DraftHistoricalRootAdoptionKeyV1) {
    encoder.fixed16(&key.draft_id().as_bytes());
    encoder.fixed16(&key.session_id().as_bytes());
    encoder.fixed16(&key.operation_id().as_bytes());
}

fn decode_key(decoder: &mut Decoder<'_>) -> Result<DraftHistoricalRootAdoptionKeyV1, CodecError> {
    Ok(DraftHistoricalRootAdoptionKeyV1::new(
        SyndicDraftId::from_bytes(decoder.fixed16()?),
        DraftEditorCandidateSessionIdV1::from_bytes(decoder.fixed16()?),
        DraftPieceOperationIdV1::from_bytes(decoder.fixed16()?),
    ))
}

fn enc_request(encoder: &mut Encoder, request: DraftHistoricalRootAdoptionRequestV1) {
    enc_key(encoder, request.key());
    super::super::enc_history_reference(encoder, request.source_history());
    enc_transition_reference(encoder, request.selected_transition());
    encoder.u8(match request.direction() {
        DraftHistoricalRootDirectionV1::Undo => 1,
        DraftHistoricalRootDirectionV1::Redo => 2,
    });
    enc_root_reference(encoder, request.target_root());
    enc_position(encoder, request.caret());
    enc_position(encoder, request.selection());
}

fn dec_request(
    decoder: &mut Decoder<'_>,
) -> Result<DraftHistoricalRootAdoptionRequestV1, CodecError> {
    let key = decode_key(decoder)?;
    let source_history = super::super::dec_history_reference(decoder)?;
    let selected_transition = dec_transition_reference(decoder)?;
    let direction = match decoder.u8()? {
        1 => DraftHistoricalRootDirectionV1::Undo,
        2 => DraftHistoricalRootDirectionV1::Redo,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "draft historical-root direction",
                tag,
            });
        }
    };
    Ok(DraftHistoricalRootAdoptionRequestV1::new(
        key.draft_id(),
        key.session_id(),
        key.operation_id(),
        source_history,
        selected_transition,
        direction,
        dec_root_reference(decoder)?,
        dec_position(decoder)?,
        dec_position(decoder)?,
    ))
}

fn enc_transition_reference(
    encoder: &mut Encoder,
    value: super::super::DraftEditHistoryTransitionReferenceV1,
) {
    super::super::codec::enc_transition_key(encoder, value.key());
    encoder.u64(value.cumulative_encoded_bytes());
    encoder.u64(value.journal_depth());
    encoder.fixed32(&value.digest().as_bytes());
}

fn dec_transition_reference(
    decoder: &mut Decoder<'_>,
) -> Result<super::super::DraftEditHistoryTransitionReferenceV1, CodecError> {
    let draft_id = SyndicDraftId::from_bytes(decoder.fixed16()?);
    let cumulative_encoded_bytes = decoder.u64()?;
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes(decoder.fixed16()?);
    let key = super::super::DraftEditHistoryTransitionKeyV1::new(
        draft_id,
        session_id,
        cumulative_encoded_bytes,
    );
    Ok(super::super::DraftEditHistoryTransitionReferenceV1::new(
        key,
        decoder.u64()?,
        decoder.u64()?,
        super::super::super::DraftPieceDigestV1::from_bytes(decoder.fixed32()?),
    ))
}

fn enc_outcome(encoder: &mut Encoder, value: DraftHistoricalRootAdoptionSettlementOutcomeV1) {
    match value {
        DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed => encoder.u8(1),
        DraftHistoricalRootAdoptionSettlementOutcomeV1::Rejected => encoder.u8(2),
        DraftHistoricalRootAdoptionSettlementOutcomeV1::Conflict => encoder.u8(3),
        DraftHistoricalRootAdoptionSettlementOutcomeV1::Cancelled => encoder.u8(4),
        DraftHistoricalRootAdoptionSettlementOutcomeV1::Error(reason) => {
            encoder.u8(5);
            encoder.u8(match reason {
                DraftHistoricalRootAdoptionErrorReasonV1::InvalidAuthority => 1,
                DraftHistoricalRootAdoptionErrorReasonV1::HistoryCapacityUnavailable => 2,
                DraftHistoricalRootAdoptionErrorReasonV1::OccupiedIdentity => 3,
            });
        }
    }
}

fn dec_outcome(
    decoder: &mut Decoder<'_>,
) -> Result<DraftHistoricalRootAdoptionSettlementOutcomeV1, CodecError> {
    match decoder.u8()? {
        1 => Ok(DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed),
        2 => Ok(DraftHistoricalRootAdoptionSettlementOutcomeV1::Rejected),
        3 => Ok(DraftHistoricalRootAdoptionSettlementOutcomeV1::Conflict),
        4 => Ok(DraftHistoricalRootAdoptionSettlementOutcomeV1::Cancelled),
        5 => Ok(DraftHistoricalRootAdoptionSettlementOutcomeV1::Error(
            match decoder.u8()? {
                1 => DraftHistoricalRootAdoptionErrorReasonV1::InvalidAuthority,
                2 => DraftHistoricalRootAdoptionErrorReasonV1::HistoryCapacityUnavailable,
                3 => DraftHistoricalRootAdoptionErrorReasonV1::OccupiedIdentity,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "draft historical-root error",
                        tag,
                    });
                }
            },
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "draft historical-root outcome",
            tag,
        }),
    }
}

fn enc_optional_transition(
    encoder: &mut Encoder,
    value: Option<&super::super::DraftEditHistoryTransitionV1>,
) {
    encoder.u8(u8::from(value.is_some()));
    if let Some(value) = value {
        enc_history_transition(encoder, value);
    }
}

fn dec_optional_transition(
    decoder: &mut Decoder<'_>,
) -> Result<Option<Box<super::super::DraftEditHistoryTransitionV1>>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => Ok(Some(Box::new(dec_history_transition(decoder)?))),
        tag => Err(CodecError::InvalidTag {
            kind: "optional historical transition",
            tag,
        }),
    }
}

fn enc_optional_history(
    encoder: &mut Encoder,
    value: Option<&super::super::DraftEditHistoryFrontierV1>,
) {
    encoder.u8(u8::from(value.is_some()));
    if let Some(value) = value {
        enc_history_frontier(encoder, value);
    }
}

fn dec_optional_history(
    decoder: &mut Decoder<'_>,
) -> Result<Option<Box<super::super::DraftEditHistoryFrontierV1>>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => Ok(Some(Box::new(dec_history_frontier(decoder)?))),
        tag => Err(CodecError::InvalidTag {
            kind: "optional historical frontier",
            tag,
        }),
    }
}

fn enc_optional_session(
    encoder: &mut Encoder,
    value: Option<&super::super::super::DraftEditorCandidateSessionV1>,
) {
    encoder.u8(u8::from(value.is_some()));
    if let Some(value) = value {
        enc_session_head(encoder, value);
    }
}

fn dec_optional_session(
    decoder: &mut Decoder<'_>,
) -> Result<Option<Box<super::super::super::DraftEditorCandidateSessionV1>>, CodecError> {
    match decoder.u8()? {
        0 => Ok(None),
        1 => Ok(Some(Box::new(dec_session_head(decoder)?))),
        tag => Err(CodecError::InvalidTag {
            kind: "optional historical candidate",
            tag,
        }),
    }
}
