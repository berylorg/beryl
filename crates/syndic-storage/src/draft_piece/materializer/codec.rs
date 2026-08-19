use beryl_home_store::RecordVersion;
use beryl_model::{ContentRevision, SyndicContentDigest, SyndicContentId};

use crate::{
    ContentEncoding, ContentReference, ContentSummary, DraftPieceDigestV1, ImageLabelOrdinal,
    codec::{
        CodecError, ExactCodec, Family, invalid,
        parts::{Decoder, Encoder},
    },
};

use super::model::*;

pub(crate) struct DraftComposerBuildsFamily;
pub(crate) struct DraftComposerMaterializationsFamily;
pub(crate) type DraftComposerBuildsCodec = ExactCodec<DraftComposerBuildsFamily>;
pub(crate) type DraftComposerMaterializationsCodec =
    ExactCodec<DraftComposerMaterializationsFamily>;

fn enc_format(e: &mut Encoder, value: DraftComposerFormatV1) {
    e.u8(match value {
        DraftComposerFormatV1::ComposerV1 => 1,
    });
}

fn dec_format(d: &mut Decoder<'_>) -> Result<DraftComposerFormatV1, CodecError> {
    match d.u8()? {
        1 => Ok(DraftComposerFormatV1::ComposerV1),
        tag => Err(CodecError::InvalidTag {
            kind: "draft composer format",
            tag,
        }),
    }
}

fn enc_operation(e: &mut Encoder, value: DraftComposerMaterializationOperationIdV1) {
    e.fixed16(&value.as_bytes());
}

fn dec_operation(
    d: &mut Decoder<'_>,
) -> Result<DraftComposerMaterializationOperationIdV1, CodecError> {
    Ok(DraftComposerMaterializationOperationIdV1::from_bytes(
        d.fixed16()?,
    ))
}

fn enc_build_key(e: &mut Encoder, value: DraftComposerBuildKeyV1) {
    super::super::codec::enc_root_reference(e, value.source());
    enc_format(e, value.format());
    enc_operation(e, value.operation());
}

fn dec_build_key(d: &mut Decoder<'_>) -> Result<DraftComposerBuildKeyV1, CodecError> {
    Ok(DraftComposerBuildKeyV1::new(
        super::super::codec::dec_root_reference(d)?,
        dec_format(d)?,
        dec_operation(d)?,
    ))
}

fn enc_mapping_key(e: &mut Encoder, value: DraftComposerMaterializationKeyV1) {
    super::super::codec::enc_root_reference(e, value.source());
    enc_format(e, value.format());
}

fn dec_mapping_key(d: &mut Decoder<'_>) -> Result<DraftComposerMaterializationKeyV1, CodecError> {
    Ok(DraftComposerMaterializationKeyV1::new(
        super::super::codec::dec_root_reference(d)?,
        dec_format(d)?,
    ))
}

fn enc_label(e: &mut Encoder, value: Option<ImageLabelOrdinal>) {
    match value {
        None => e.u8(0),
        Some(value) => {
            e.u8(1);
            e.u64(value.get());
        }
    }
}

fn dec_label(d: &mut Decoder<'_>) -> Result<Option<ImageLabelOrdinal>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => ImageLabelOrdinal::new(d.u64()?)
            .map(Some)
            .map_err(|error| invalid("draft composer maximum image label", error)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft composer image label option",
            tag,
        }),
    }
}

fn enc_summary(e: &mut Encoder, value: ContentSummary) {
    e.u64(value.chunk_count());
    e.u64(value.piece_count());
    e.u64(value.encoded_bytes());
    e.u64(value.logical_utf8_bytes());
    e.u64(value.atom_count());
    e.u64(value.image_marker_count());
    e.fixed32(&value.marker_digest());
    enc_label(e, value.maximum_image_label());
    e.fixed32(value.digest().as_bytes());
}

fn dec_summary(d: &mut Decoder<'_>) -> Result<ContentSummary, CodecError> {
    ContentSummary::new(
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.fixed32()?,
        dec_label(d)?,
        SyndicContentDigest::from_bytes(d.fixed32()?),
    )
    .map_err(|error| invalid("draft composer content summary", error))
}

fn enc_reference(e: &mut Encoder, value: ContentReference) {
    e.fixed16(value.id().as_bytes());
    e.u64(value.revision().get());
    enc_summary(e, value.summary());
}

fn dec_reference(d: &mut Decoder<'_>) -> Result<ContentReference, CodecError> {
    let id = SyndicContentId::from_bytes(d.fixed16()?);
    let revision = ContentRevision::new(d.u64()?)
        .map_err(|error| invalid("draft composer content revision", error))?;
    Ok(ContentReference::new(
        id,
        revision,
        ContentEncoding::ComposerV1,
        dec_summary(d)?,
    ))
}

fn enc_cursor(e: &mut Encoder, value: DraftComposerSourceCursorV1) {
    e.u64(value.piece_index());
    e.u64(value.atom_encoded_offset());
}

fn dec_cursor(d: &mut Decoder<'_>) -> Result<DraftComposerSourceCursorV1, CodecError> {
    Ok(DraftComposerSourceCursorV1::new(d.u64()?, d.u64()?))
}

fn enc_encoder(e: &mut Encoder, value: &DraftComposerEncoderStateV1) {
    enc_cursor(e, value.cursor());
    e.u64(value.source_piece_count());
    e.u64(value.encoded_bytes());
    e.u64(value.logical_utf8_bytes());
    e.u64(value.chunk_count());
    e.u64(value.piece_count());
    e.u64(value.marker_count());
    e.fixed32(&value.marker_digest());
    enc_label(e, value.maximum_image_label());
    e.fixed32(value.chain_digest().as_bytes());
    e.bytes(value.carry());
    e.u8(u8::from(value.break_before()));
    e.u64(
        value
            .active_text_span_encoded_start()
            .map_or(0, |value| value + 1),
    );
    e.u64(
        value
            .active_text_span_logical_start()
            .map_or(0, |value| value + 1),
    );
}

fn dec_bool(d: &mut Decoder<'_>, kind: &'static str) -> Result<bool, CodecError> {
    match d.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        tag => Err(CodecError::InvalidTag { kind, tag }),
    }
}

fn dec_encoder(d: &mut Decoder<'_>) -> Result<DraftComposerEncoderStateV1, CodecError> {
    let cursor = dec_cursor(d)?;
    let source_piece_count = d.u64()?;
    let encoded_bytes = d.u64()?;
    let logical_utf8_bytes = d.u64()?;
    let chunk_count = d.u64()?;
    let piece_count = d.u64()?;
    let marker_count = d.u64()?;
    let marker_digest = d.fixed32()?;
    let maximum_image_label = dec_label(d)?;
    let chain_digest = SyndicContentDigest::from_bytes(d.fixed32()?);
    let carry = d.bytes("draft composer encoder carry")?.to_vec();
    if carry.len() > DRAFT_COMPOSER_CARRY_MAX_BYTES {
        return Err(CodecError::InvalidLength("draft composer encoder carry"));
    }
    let break_before = dec_bool(d, "draft composer break-before")?;
    let encoded_start = d.u64()?;
    let logical_start = d.u64()?;
    Ok(DraftComposerEncoderStateV1::new(
        cursor,
        source_piece_count,
        encoded_bytes,
        logical_utf8_bytes,
        chunk_count,
        piece_count,
        marker_count,
        marker_digest,
        maximum_image_label,
        chain_digest,
        carry,
        break_before,
        (encoded_start != 0).then(|| encoded_start - 1),
        (logical_start != 0).then(|| logical_start - 1),
    ))
}

fn enc_records(e: &mut Encoder, value: DraftComposerRecordFrontierV1) {
    enc_cursor(e, value.cursor());
    e.u64(value.encoded_bytes());
    e.u64(value.logical_utf8_bytes());
    e.u64(value.piece_count());
    e.u64(value.marker_count());
    e.fixed32(&value.marker_digest());
    enc_label(e, value.maximum_image_label());
    e.u64(value.chunk_start());
    e.u64(value.chunk_ordinal());
    e.u8(u8::from(value.break_before()));
}

fn dec_records(d: &mut Decoder<'_>) -> Result<DraftComposerRecordFrontierV1, CodecError> {
    Ok(DraftComposerRecordFrontierV1::new(
        dec_cursor(d)?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.fixed32()?,
        dec_label(d)?,
        d.u64()?,
        d.u64()?,
        dec_bool(d, "draft composer record break-before")?,
    ))
}

fn enc_phase(e: &mut Encoder, value: DraftComposerBuildPhaseV1) {
    match value {
        DraftComposerBuildPhaseV1::Planning => e.u8(0),
        DraftComposerBuildPhaseV1::Writing => e.u8(1),
        DraftComposerBuildPhaseV1::Draining { final_chunk } => {
            e.u8(2);
            e.u8(u8::from(final_chunk));
        }
        DraftComposerBuildPhaseV1::ReadyToSeal => e.u8(3),
    }
}

fn dec_phase(d: &mut Decoder<'_>) -> Result<DraftComposerBuildPhaseV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftComposerBuildPhaseV1::Planning),
        1 => Ok(DraftComposerBuildPhaseV1::Writing),
        2 => Ok(DraftComposerBuildPhaseV1::Draining {
            final_chunk: dec_bool(d, "draft composer final chunk")?,
        }),
        3 => Ok(DraftComposerBuildPhaseV1::ReadyToSeal),
        tag => Err(CodecError::InvalidTag {
            kind: "draft composer build phase",
            tag,
        }),
    }
}

fn enc_lifecycle(e: &mut Encoder, value: &DraftComposerBuildLifecycleV1) {
    match value {
        DraftComposerBuildLifecycleV1::Open(phase) => {
            e.u8(0);
            enc_phase(e, *phase);
        }
        DraftComposerBuildLifecycleV1::Cancelled => e.u8(1),
        DraftComposerBuildLifecycleV1::Failed(DraftComposerFailureReasonV1::Operational) => {
            e.u8(2);
            e.u8(0);
        }
        DraftComposerBuildLifecycleV1::Superseded(successor) => {
            e.u8(3);
            enc_operation(e, *successor);
        }
        DraftComposerBuildLifecycleV1::Sealed(reference) => {
            e.u8(4);
            enc_reference(e, *reference);
        }
    }
}

fn dec_lifecycle(d: &mut Decoder<'_>) -> Result<DraftComposerBuildLifecycleV1, CodecError> {
    match d.u8()? {
        0 => Ok(DraftComposerBuildLifecycleV1::Open(dec_phase(d)?)),
        1 => Ok(DraftComposerBuildLifecycleV1::Cancelled),
        2 => match d.u8()? {
            0 => Ok(DraftComposerBuildLifecycleV1::Failed(
                DraftComposerFailureReasonV1::Operational,
            )),
            tag => Err(CodecError::InvalidTag {
                kind: "draft composer failure reason",
                tag,
            }),
        },
        3 => Ok(DraftComposerBuildLifecycleV1::Superseded(dec_operation(d)?)),
        4 => Ok(DraftComposerBuildLifecycleV1::Sealed(dec_reference(d)?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft composer lifecycle",
            tag,
        }),
    }
}

fn enc_optional_reference(e: &mut Encoder, value: Option<ContentReference>) {
    match value {
        None => e.u8(0),
        Some(value) => {
            e.u8(1);
            enc_reference(e, value);
        }
    }
}

fn dec_optional_reference(d: &mut Decoder<'_>) -> Result<Option<ContentReference>, CodecError> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(dec_reference(d)?)),
        tag => Err(CodecError::InvalidTag {
            kind: "draft composer output option",
            tag,
        }),
    }
}

fn enc_optional_revision(e: &mut Encoder, value: Option<ContentRevision>) {
    e.u64(value.map_or(0, ContentRevision::get));
}

fn dec_optional_revision(d: &mut Decoder<'_>) -> Result<Option<ContentRevision>, CodecError> {
    let value = d.u64()?;
    if value == 0 {
        Ok(None)
    } else {
        ContentRevision::new(value)
            .map(Some)
            .map_err(|error| invalid("draft composer output revision", error))
    }
}

fn encode_build_key(value: &DraftComposerBuildKeyV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_build_key(&mut e, *value);
    Ok(e.finish())
}

fn decode_build_key(bytes: &[u8]) -> Result<DraftComposerBuildKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_build_key(&mut d)?;
    d.finish()?;
    Ok(value)
}

fn encode_build(value: &DraftComposerBuildRecordV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_build_key(&mut e, value.key());
    enc_encoder(&mut e, value.encoder());
    enc_records(&mut e, value.records());
    enc_optional_reference(&mut e, value.output());
    enc_optional_revision(&mut e, value.output_revision());
    e.u64(value.output_chunk_count());
    e.u64(value.output_encoded_bytes());
    e.fixed32(value.output_chain_digest().as_bytes());
    enc_lifecycle(&mut e, value.lifecycle());
    let bytes = e.finish();
    if bytes.len() > 65_536 {
        return Err(CodecError::InvalidLength("draft composer build"));
    }
    Ok(bytes)
}

fn decode_build(bytes: &[u8]) -> Result<DraftComposerBuildRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = DraftComposerBuildRecordV1::new(
        dec_build_key(&mut d)?,
        dec_encoder(&mut d)?,
        dec_records(&mut d)?,
        dec_optional_reference(&mut d)?,
        dec_optional_revision(&mut d)?,
        d.u64()?,
        d.u64()?,
        SyndicContentDigest::from_bytes(d.fixed32()?),
        dec_lifecycle(&mut d)?,
    );
    d.finish()?;
    if let Some(reason) = value.local_shape_error() {
        return Err(CodecError::InvalidLength(reason));
    }
    Ok(value)
}

fn encode_mapping_key(value: &DraftComposerMaterializationKeyV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_mapping_key(&mut e, *value);
    Ok(e.finish())
}

fn decode_mapping_key(bytes: &[u8]) -> Result<DraftComposerMaterializationKeyV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let value = dec_mapping_key(&mut d)?;
    d.finish()?;
    Ok(value)
}

fn encode_mapping(value: &DraftComposerMaterializationRecordV1) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_mapping_key(&mut e, value.key());
    enc_operation(&mut e, value.sealing_operation());
    e.fixed32(value.source_digest().as_bytes());
    e.u64(value.source_piece_count());
    e.u64(value.source_utf8_bytes());
    e.u64(value.source_marker_count());
    enc_reference(&mut e, value.content());
    Ok(e.finish())
}

fn decode_mapping(bytes: &[u8]) -> Result<DraftComposerMaterializationRecordV1, CodecError> {
    let mut d = Decoder::new(bytes);
    let key = dec_mapping_key(&mut d)?;
    let sealing_operation = dec_operation(&mut d)?;
    let source_digest = DraftPieceDigestV1::from_bytes(d.fixed32()?);
    let source_piece_count = d.u64()?;
    let source_utf8_bytes = d.u64()?;
    let source_marker_count = d.u64()?;
    let value =
        DraftComposerMaterializationRecordV1::new(key, sealing_operation, dec_reference(&mut d)?);
    if value.source_digest() != source_digest
        || value.source_piece_count() != source_piece_count
        || value.source_utf8_bytes() != source_utf8_bytes
        || value.source_marker_count() != source_marker_count
    {
        return Err(CodecError::InvalidLength(
            "draft composer source summary closure",
        ));
    }
    d.finish()?;
    Ok(value)
}

impl Family for DraftComposerBuildsFamily {
    type Key = DraftComposerBuildKeyV1;
    type Value = DraftComposerBuildRecordV1;
    const NAME: &'static str = "draft-composer-builds";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 512;
    const MAX_VALUE_BYTES: usize = 65_536;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        encode_build_key(key)
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        decode_build_key(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_build(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_build(encoded)
    }
}

impl Family for DraftComposerMaterializationsFamily {
    type Key = DraftComposerMaterializationKeyV1;
    type Value = DraftComposerMaterializationRecordV1;
    const NAME: &'static str = "draft-composer-materializations";
    const RECORD_VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 496;
    const MAX_VALUE_BYTES: usize = 768;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        encode_mapping_key(key)
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        decode_mapping_key(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        encode_mapping(value)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        decode_mapping(encoded)
    }
}
