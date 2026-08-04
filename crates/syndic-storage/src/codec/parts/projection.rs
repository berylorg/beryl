use crate::{ContentEncoding, ProjectionTextSource, ProjectionTextSourceCursor, SyndicRecordError};

use super::{
    CodecError, Decoder, Encoder, dec_content_piece_ord, dec_content_ref,
    dec_provider_narrative_reference, enc_content_piece_ord, enc_content_ref,
    enc_provider_narrative_reference,
};

pub(crate) fn enc_projection_text_source(encoder: &mut Encoder, source: ProjectionTextSource) {
    match source {
        ProjectionTextSource::Composer(content) => {
            encoder.u8(0);
            enc_content_ref(encoder, content);
        }
        ProjectionTextSource::ProviderNarrative(narrative) => {
            encoder.u8(1);
            enc_provider_narrative_reference(encoder, narrative);
        }
    }
}

pub(crate) fn dec_projection_text_source(
    decoder: &mut Decoder<'_>,
) -> Result<ProjectionTextSource, CodecError> {
    match decoder.u8()? {
        0 => {
            let content = dec_content_ref(decoder)?;
            if content.encoding() != ContentEncoding::ComposerV1 {
                return Err(super::invalid(
                    "projection composer source",
                    SyndicRecordError::InvalidContentEncoding,
                ));
            }
            Ok(ProjectionTextSource::composer(content))
        }
        1 => Ok(ProjectionTextSource::provider_narrative(
            dec_provider_narrative_reference(decoder)?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "projection text source",
            tag,
        }),
    }
}

pub(crate) fn enc_projection_text_source_cursor(
    encoder: &mut Encoder,
    cursor: ProjectionTextSourceCursor,
) {
    match cursor {
        ProjectionTextSourceCursor::Composer(ordinal) => {
            encoder.u8(0);
            enc_content_piece_ord(encoder, ordinal);
        }
        ProjectionTextSourceCursor::ProviderNarrative { logical_start } => {
            encoder.u8(1);
            encoder.u64(logical_start);
        }
    }
}

pub(crate) fn dec_projection_text_source_cursor(
    decoder: &mut Decoder<'_>,
) -> Result<ProjectionTextSourceCursor, CodecError> {
    match decoder.u8()? {
        0 => Ok(ProjectionTextSourceCursor::Composer(dec_content_piece_ord(
            decoder,
        )?)),
        1 => Ok(ProjectionTextSourceCursor::ProviderNarrative {
            logical_start: decoder.u64()?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "projection text source cursor",
            tag,
        }),
    }
}
