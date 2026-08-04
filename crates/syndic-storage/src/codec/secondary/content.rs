use crate::{ContentByteSpanRecord, ContentPieceRecord, ContentTextSpanRecord};

use super::*;

pub(crate) struct ContentByteSpansFamily;
pub(crate) type ContentByteSpansCodec = ExactCodec<ContentByteSpansFamily>;

impl Family for ContentByteSpansFamily {
    type Key = ContentByteSpanKey;
    type Value = ContentByteSpanRecord;
    const NAME: &'static str = "content-byte-spans";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = 80;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok((*key).encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ContentByteSpanKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_content(&mut encoder, value.content_id());
        enc_content_chunk_ord(&mut encoder, value.ordinal());
        encoder.u64(value.start());
        encoder.u64(value.end());
        encoder.fixed32(&value.chunk_digest());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let value = ContentByteSpanRecord::new(
            dec_content(&mut decoder)?,
            dec_content_chunk_ord(&mut decoder)?,
            decoder.u64()?,
            decoder.u64()?,
            decoder.fixed32()?,
        )
        .map_err(|source| invalid("content byte span", source))?;
        decoder.finish()?;
        Ok(value)
    }
}

pub(crate) struct ContentTextSpansFamily;
pub(crate) type ContentTextSpansCodec = ExactCodec<ContentTextSpansFamily>;

impl Family for ContentTextSpansFamily {
    type Key = ContentTextSpanKey;
    type Value = ContentTextSpanRecord;
    const NAME: &'static str = "content-text-spans";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = 128;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok((*key).encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ContentTextSpanKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_content(&mut encoder, value.content_id());
        enc_content_piece_ord(&mut encoder, value.piece_ordinal());
        enc_content_chunk_ord(&mut encoder, value.chunk_ordinal());
        encoder.u64(value.chunk_start());
        encoder.u64(value.logical_start());
        encoder.u64(value.logical_end());
        encoder.u64(value.encoded_start());
        encoder.u64(value.encoded_end());
        enc_bool(&mut encoder, value.break_before());
        encoder.fixed32(&value.digest());
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let value = ContentTextSpanRecord::new(
            dec_content(&mut decoder)?,
            dec_content_piece_ord(&mut decoder)?,
            dec_content_chunk_ord(&mut decoder)?,
            decoder.u64()?,
            decoder.u64()?,
            decoder.u64()?,
            decoder.u64()?,
            decoder.u64()?,
            dec_bool(&mut decoder, "content text-span break")?,
            decoder.fixed32()?,
        )
        .map_err(|source| invalid("content text span", source))?;
        decoder.finish()?;
        Ok(value)
    }
}

pub(crate) struct ContentPiecesFamily;
pub(crate) type ContentPiecesCodec = ExactCodec<ContentPiecesFamily>;

impl Family for ContentPiecesFamily {
    type Key = ContentPieceKey;
    type Value = ContentPieceRecord;
    const NAME: &'static str = "content-pieces";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = 128;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok((*key).encode())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        ContentPieceKey::decode(encoded)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        match value {
            ContentPieceRecord::Text(span) => {
                encoder.u8(0);
                enc_content(&mut encoder, span.content_id());
                enc_content_piece_ord(&mut encoder, span.piece_ordinal());
                enc_content_chunk_ord(&mut encoder, span.chunk_ordinal());
                encoder.u64(span.chunk_start());
                encoder.u64(span.logical_start());
                encoder.u64(span.logical_end());
                encoder.u64(span.encoded_start());
                encoder.u64(span.encoded_end());
                enc_bool(&mut encoder, span.break_before());
                encoder.fixed32(&span.digest());
            }
            ContentPieceRecord::ImageMarker {
                content_id,
                ordinal,
                atom_ordinal,
                marker_ordinal,
                logical_offset,
                encoded_start,
                encoded_end,
                marker_id,
                label,
                digest,
            } => {
                encoder.u8(1);
                enc_content(&mut encoder, *content_id);
                enc_content_piece_ord(&mut encoder, *ordinal);
                enc_composer_atom_ord(&mut encoder, *atom_ordinal);
                enc_input_marker_ord(&mut encoder, *marker_ordinal);
                encoder.u64(*logical_offset);
                encoder.u64(*encoded_start);
                encoder.u64(*encoded_end);
                enc_marker(&mut encoder, *marker_id);
                enc_image_label(&mut encoder, *label);
                encoder.fixed32(digest);
            }
        }
        Ok(encoder.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(encoded);
        let value = match decoder.u8()? {
            0 => ContentPieceRecord::text(
                ContentTextSpanRecord::new(
                    dec_content(&mut decoder)?,
                    dec_content_piece_ord(&mut decoder)?,
                    dec_content_chunk_ord(&mut decoder)?,
                    decoder.u64()?,
                    decoder.u64()?,
                    decoder.u64()?,
                    decoder.u64()?,
                    decoder.u64()?,
                    dec_bool(&mut decoder, "content text-piece break")?,
                    decoder.fixed32()?,
                )
                .map_err(|source| invalid("content text piece", source))?,
            ),
            1 => ContentPieceRecord::image_marker(
                dec_content(&mut decoder)?,
                dec_content_piece_ord(&mut decoder)?,
                dec_composer_atom_ord(&mut decoder)?,
                dec_input_marker_ord(&mut decoder)?,
                decoder.u64()?,
                decoder.u64()?,
                decoder.u64()?,
                dec_marker(&mut decoder)?,
                dec_image_label(&mut decoder)?,
                decoder.fixed32()?,
            )
            .map_err(|source| invalid("content image-marker piece", source))?,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "content piece",
                    tag,
                });
            }
        };
        decoder.finish()?;
        Ok(value)
    }
}
