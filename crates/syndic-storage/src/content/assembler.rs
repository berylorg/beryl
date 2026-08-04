use super::*;

/// Incremental bounded-page assembler for one exact sealed composer content object.
pub struct ComposerContentAssembler {
    reference: crate::ContentReference,
    next_ordinal: u64,
    encoded: Vec<u8>,
    chain: beryl_model::SyndicContentDigest,
}

impl ComposerContentAssembler {
    pub fn new(reference: crate::ContentReference) -> Result<Self, SyndicRecordError> {
        if reference.encoding() != ContentEncoding::ComposerV1 {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        let capacity = usize::try_from(reference.summary().encoded_bytes()).map_err(|_| {
            SyndicRecordError::LengthOverflow {
                kind: "composer content",
            }
        })?;
        Ok(Self {
            reference,
            next_ordinal: 1,
            encoded: Vec::with_capacity(capacity),
            chain: content_chain_seed(ContentEncoding::ComposerV1),
        })
    }

    pub fn push(&mut self, chunk: &ContentChunkRecord) -> Result<(), SyndicRecordError> {
        if chunk.content_id() != self.reference.id() || chunk.ordinal().get() != self.next_ordinal {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        self.encoded
            .len()
            .checked_add(chunk.bytes().len())
            .filter(|length| {
                u64::try_from(*length)
                    .ok()
                    .is_some_and(|length| length <= self.reference.summary().encoded_bytes())
            })
            .ok_or(SyndicRecordError::InvalidContentEncoding)?;
        self.encoded.extend_from_slice(chunk.bytes());
        self.chain = advance_content_chain(self.chain, chunk);
        self.next_ordinal =
            self.next_ordinal
                .checked_add(1)
                .ok_or(SyndicRecordError::LengthOverflow {
                    kind: "content chunks",
                })?;
        Ok(())
    }

    pub fn finish(self) -> Result<ComposerPayload, SyndicRecordError> {
        let summary = self.reference.summary();
        if self.next_ordinal.saturating_sub(1) != summary.chunk_count()
            || u64::try_from(self.encoded.len()).ok() != Some(summary.encoded_bytes())
            || self.chain != summary.digest()
        {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        let payload = decode_composer_content(&self.encoded)?;
        if u64::try_from(payload.utf8_bytes()).ok() != Some(summary.logical_utf8_bytes())
            || u64::try_from(payload.atoms().len()).ok() != Some(summary.atom_count())
            || u64::try_from(payload.image_marker_count()).ok()
                != Some(summary.image_marker_count())
            || input_marker_digest(
                payload
                    .atoms()
                    .iter()
                    .filter_map(ComposerAtom::image_marker_value)
                    .map(|marker| (marker.marker_id(), marker.label())),
            ) != summary.marker_digest()
            || payload
                .atoms()
                .iter()
                .filter_map(ComposerAtom::image_marker_value)
                .map(|marker| marker.label())
                .max()
                != summary.maximum_image_label()
        {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        Ok(payload)
    }
}
