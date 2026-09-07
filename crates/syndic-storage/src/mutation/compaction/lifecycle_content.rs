use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits};
use beryl_model::ContentRevision;

use super::*;
use crate::{ContentByteSpanRecord, ContentChunkRecord, ContentPieceRecord, ContentTextSpanRecord};

const FIXED_CONTENT_MAX_BYTES: usize = 65_536;

impl SyndicStorage {
    #[must_use]
    pub fn current_publish_lifecycle_continuation_content(&self) -> CurrentDomainCommand {
        self.handle.current_command(PublishLifecycleContentMutation)
    }
}

struct PublishLifecycleContentMutation;

struct FixedContentRecords {
    manifest: ContentManifestRecord,
    chunk: ContentChunkRecord,
    byte_span: ContentByteSpanRecord,
    text_span: ContentTextSpanRecord,
    piece: ContentPieceRecord,
}

impl DomainMutation<SyndicDomain> for PublishLifecycleContentMutation {
    type Error = SyndicMutationError;
    type Prepared = FixedContentRecords;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let content = crate::prepare_lifecycle_continuation_content()?;
        let [chunk] = content.chunks() else {
            return Err(SyndicMutationError::ContentNotComplete);
        };
        let [text_span] = content.text_spans() else {
            return Err(SyndicMutationError::ContentNotComplete);
        };
        let [piece] = content.pieces() else {
            return Err(SyndicMutationError::ContentNotComplete);
        };
        let current = point::<ContentManifestsFamily>(reader, &content.id())?;
        let revision = current
            .as_ref()
            .map_or(ContentRevision::new(1)?, |manifest| manifest.revision());
        let manifest = content.sealed_manifest(revision);
        if current.as_ref().is_some_and(|current| current != &manifest) {
            return Err(SyndicMutationError::ContentManifestConflict);
        }
        let records = FixedContentRecords {
            manifest,
            chunk: chunk.clone(),
            byte_span: ContentByteSpanRecord::for_chunk(chunk, 0)?,
            text_span: *text_span,
            piece: *piece,
        };
        records.validate(reader, current.is_some())?;
        if current.is_some() {
            return Err(SyndicMutationError::LifecycleContentAlreadyPublished {
                content: content.reference(revision),
            });
        }
        Ok(records)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ContentManifestsCodec>(1)?;
        reservation.reserve_records::<ContentChunksCodec>(1)?;
        reservation.reserve_records::<ContentByteSpansCodec>(1)?;
        reservation.reserve_records::<ContentTextSpansCodec>(1)?;
        reservation.reserve_records::<ContentPiecesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ContentManifestsCodec>(&prepared.manifest.id(), &prepared.manifest)?;
        mutations.put::<ContentChunksCodec>(&prepared.chunk_key(), &prepared.chunk)?;
        mutations.put::<ContentByteSpansCodec>(&prepared.byte_span_key(), &prepared.byte_span)?;
        mutations.put::<ContentTextSpansCodec>(&prepared.text_span_key(), &prepared.text_span)?;
        mutations.put::<ContentPiecesCodec>(&prepared.piece_key(), &prepared.piece)?;
        Ok(())
    }
}

impl FixedContentRecords {
    fn chunk_key(&self) -> ContentChunkKey {
        ContentChunkKey {
            owner: self.manifest.id(),
            ordinal: self.chunk.ordinal(),
        }
    }

    fn byte_span_key(&self) -> ContentByteSpanKey {
        ContentByteSpanKey {
            owner: self.manifest.id(),
            start: self.byte_span.start(),
        }
    }

    fn text_span_key(&self) -> ContentTextSpanKey {
        ContentTextSpanKey {
            owner: self.manifest.id(),
            logical_start: self.text_span.logical_start(),
        }
    }

    fn piece_key(&self) -> ContentPieceKey {
        ContentPieceKey {
            owner: self.manifest.id(),
            ordinal: self.piece.ordinal(),
        }
    }

    fn validate(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        exists: bool,
    ) -> Result<(), SyndicMutationError> {
        let owner = self.manifest.id();
        let mut bytes = record_bytes::<ContentManifestsFamily>(&owner, &self.manifest)?;
        bytes = bytes
            .checked_add(validate_child::<ContentChunksFamily>(
                reader,
                CursorRange::closed(
                    ContentChunkKey {
                        owner,
                        ordinal: crate::ContentChunkOrdinal::FIRST,
                    },
                    ContentChunkKey {
                        owner,
                        ordinal: crate::ContentChunkOrdinal::new(u64::MAX)?,
                    },
                ),
                &self.chunk_key(),
                &self.chunk,
                exists,
            )?)
            .ok_or(SyndicMutationError::ContentNotComplete)?;
        bytes = bytes
            .checked_add(validate_child::<ContentByteSpansFamily>(
                reader,
                CursorRange::closed(
                    ContentByteSpanKey { owner, start: 0 },
                    ContentByteSpanKey {
                        owner,
                        start: u64::MAX,
                    },
                ),
                &self.byte_span_key(),
                &self.byte_span,
                exists,
            )?)
            .ok_or(SyndicMutationError::ContentNotComplete)?;
        bytes = bytes
            .checked_add(validate_child::<ContentTextSpansFamily>(
                reader,
                CursorRange::closed(
                    ContentTextSpanKey {
                        owner,
                        logical_start: 0,
                    },
                    ContentTextSpanKey {
                        owner,
                        logical_start: u64::MAX,
                    },
                ),
                &self.text_span_key(),
                &self.text_span,
                exists,
            )?)
            .ok_or(SyndicMutationError::ContentNotComplete)?;
        bytes = bytes
            .checked_add(validate_child::<ContentPiecesFamily>(
                reader,
                CursorRange::closed(
                    ContentPieceKey {
                        owner,
                        ordinal: crate::ContentPieceOrdinal::FIRST,
                    },
                    ContentPieceKey {
                        owner,
                        ordinal: crate::ContentPieceOrdinal::new(u64::MAX)?,
                    },
                ),
                &self.piece_key(),
                &self.piece,
                exists,
            )?)
            .ok_or(SyndicMutationError::ContentNotComplete)?;
        if bytes > FIXED_CONTENT_MAX_BYTES {
            return Err(SyndicMutationError::ContentNotComplete);
        }
        Ok(())
    }
}

fn validate_child<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    range: CursorRange<F::Key>,
    key: &F::Key,
    value: &F::Value,
    exists: bool,
) -> Result<usize, SyndicMutationError> {
    let page = reader.cursor::<ExactCodec<F>>(
        &range,
        CursorDirection::Forward,
        CursorReadLimits::new(2, FIXED_CONTENT_MAX_BYTES).expect("fixed nonzero limits"),
    )?;
    if page.has_more() || page.records().len() != usize::from(exists) {
        return Err(SyndicMutationError::ContentChunkConflict);
    }
    if let Some(record) = page.records().first() {
        if canonical_key::<F>(record.key())? != canonical_key::<F>(key)?
            || canonical_value::<F>(record.value())? != canonical_value::<F>(value)?
        {
            return Err(SyndicMutationError::ContentChunkConflict);
        }
    }
    record_bytes::<F>(key, value)
}

fn canonical_key<F: Family>(key: &F::Key) -> Result<Vec<u8>, SyndicMutationError> {
    F::encode_key(key).map_err(|_| SyndicMutationError::ContentChunkConflict)
}

fn canonical_value<F: Family>(value: &F::Value) -> Result<Vec<u8>, SyndicMutationError> {
    F::encode_value(value).map_err(|_| SyndicMutationError::ContentChunkConflict)
}

fn record_bytes<F: Family>(key: &F::Key, value: &F::Value) -> Result<usize, SyndicMutationError> {
    canonical_key::<F>(key)?
        .len()
        .checked_add(canonical_value::<F>(value)?.len())
        .ok_or(SyndicMutationError::ContentNotComplete)
}
