use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, MutationContribution, ReconciliationReservation,
};
use beryl_model::DomainRevision;

use crate::{
    ContentChunkRecord, ContentLifecycle, ContentManifestRecord, PreparedContent, SyndicStorage,
    advance_content_chain, codec::*, domain::SyndicDomain,
};

mod validation;

use validation::validate_prepared_manifest;

use super::{SyndicMutationError, point, required};

/// Maximum chunk count admitted by one staging command.
pub const CONTENT_APPEND_MAX_CHUNKS: usize = 16;

/// One exact natural content object before any owner publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentBuild {
    manifest: ContentManifestRecord,
}

impl ContentBuild {
    #[must_use]
    pub fn from_prepared(content: &PreparedContent) -> Self {
        Self {
            manifest: content.building_manifest(),
        }
    }

    #[must_use]
    pub const fn manifest(&self) -> &ContentManifestRecord {
        &self.manifest
    }
}

/// One bounded contiguous append to a building content object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentAppend {
    expected: ContentManifestRecord,
    next: ContentManifestRecord,
    chunks: Vec<ContentChunkRecord>,
    spans: Vec<crate::ContentByteSpanRecord>,
    text_spans: Vec<crate::ContentTextSpanRecord>,
    pieces: Vec<crate::ContentPieceRecord>,
}

impl ContentAppend {
    pub fn prepare(
        manifest: &ContentManifestRecord,
        content: &PreparedContent,
    ) -> Result<Option<Self>, SyndicMutationError> {
        validate_prepared_manifest(manifest, content)?;
        if manifest.lifecycle() == ContentLifecycle::Sealed {
            return Ok(None);
        }
        let start = usize::try_from(manifest.chunk_count())
            .map_err(|_| SyndicMutationError::ContentManifestConflict)?;
        if start >= content.chunks().len() {
            return Ok(None);
        }
        let end = start
            .saturating_add(CONTENT_APPEND_MAX_CHUNKS)
            .min(content.chunks().len());
        let chunks = content.chunks()[start..end].to_vec();
        let spans = crate::content_byte_spans(&chunks, manifest.encoded_bytes())?;
        let first_ordinal = u64::try_from(start)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(SyndicMutationError::ContentManifestConflict)?;
        let last_ordinal =
            u64::try_from(end).map_err(|_| SyndicMutationError::ContentManifestConflict)?;
        let text_spans = content
            .text_spans()
            .iter()
            .copied()
            .filter(|span| (first_ordinal..=last_ordinal).contains(&span.chunk_ordinal().get()))
            .collect();
        let mut chain = manifest.chain_digest();
        let mut encoded_bytes = manifest.encoded_bytes();
        for chunk in &chunks {
            chain = advance_content_chain(chain, chunk);
            encoded_bytes = encoded_bytes
                .checked_add(
                    u64::try_from(chunk.bytes().len())
                        .map_err(|_| SyndicMutationError::ContentManifestConflict)?,
                )
                .ok_or(SyndicMutationError::ContentManifestConflict)?;
        }
        let pieces = content
            .pieces()
            .iter()
            .copied()
            .filter(|piece| {
                piece.encoded_end() > manifest.encoded_bytes()
                    && piece.encoded_end() <= encoded_bytes
            })
            .collect();
        let chunk_count =
            u64::try_from(end).map_err(|_| SyndicMutationError::ContentManifestConflict)?;
        let next = ContentManifestRecord::new(
            manifest.id(),
            manifest.revision().checked_next()?,
            manifest.encoding(),
            ContentLifecycle::Building,
            chunk_count,
            encoded_bytes,
            chain,
            manifest.expected(),
        );
        if chunk_count > manifest.expected().chunk_count()
            || encoded_bytes > manifest.expected().encoded_bytes()
            || (chunk_count == manifest.expected().chunk_count()
                && (encoded_bytes != manifest.expected().encoded_bytes()
                    || chain != manifest.expected().digest()))
        {
            return Err(SyndicMutationError::ContentManifestConflict);
        }
        Ok(Some(Self {
            expected: manifest.clone(),
            next,
            chunks,
            spans,
            text_spans,
            pieces,
        }))
    }

    #[must_use]
    pub const fn next_manifest(&self) -> &ContentManifestRecord {
        &self.next
    }
}

impl SyndicStorage {
    #[must_use]
    pub fn begin_content(
        &self,
        expected_domain_revision: DomainRevision,
        build: ContentBuild,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, BeginContentMutation { build })
    }

    #[must_use]
    pub fn append_content(
        &self,
        expected_domain_revision: DomainRevision,
        append: ContentAppend,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, AppendContentMutation { append })
    }
}

struct BeginContentMutation {
    build: ContentBuild,
}
struct AppendContentMutation {
    append: ContentAppend,
}

impl DomainMutation<SyndicDomain> for BeginContentMutation {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if point::<ContentManifestsFamily>(reader, &self.build.manifest.id())?.is_some() {
            return Err(SyndicMutationError::ContentIdentityCollision);
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ContentManifestsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ContentManifestsCodec>(
            &prepared.build.manifest.id(),
            &prepared.build.manifest,
        )?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for AppendContentMutation {
    type Error = SyndicMutationError;
    type Prepared = Self;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let current = required::<ContentManifestsFamily>(reader, &self.append.expected.id())?;
        if current != self.append.expected {
            return Err(SyndicMutationError::ContentManifestConflict);
        }
        for chunk in &self.append.chunks {
            let key = ContentChunkKey {
                owner: chunk.content_id(),
                ordinal: chunk.ordinal(),
            };
            if point::<ContentChunksFamily>(reader, &key)?.is_some() {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
        }
        for span in &self.append.spans {
            let key = ContentByteSpanKey {
                owner: span.content_id(),
                start: span.start(),
            };
            if point::<ContentByteSpansFamily>(reader, &key)?.is_some() {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
        }
        for span in &self.append.text_spans {
            let key = ContentTextSpanKey {
                owner: span.content_id(),
                logical_start: span.logical_start(),
            };
            if point::<ContentTextSpansFamily>(reader, &key)?.is_some() {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
        }
        for piece in &self.append.pieces {
            let key = ContentPieceKey {
                owner: piece.content_id(),
                ordinal: piece.ordinal(),
            };
            if point::<ContentPiecesFamily>(reader, &key)?.is_some() {
                return Err(SyndicMutationError::ContentChunkConflict);
            }
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ContentChunksCodec>(self.append.chunks.len())?;
        reservation.reserve_records::<ContentByteSpansCodec>(self.append.spans.len())?;
        reservation
            .reserve_records::<ContentTextSpansCodec>(self.append.text_spans.len().max(1))?;
        reservation.reserve_records::<ContentPiecesCodec>(self.append.pieces.len().max(1))?;
        reservation.reserve_records::<ContentManifestsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for chunk in &prepared.append.chunks {
            mutations.put::<ContentChunksCodec>(
                &ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
                chunk,
            )?;
        }
        for span in &prepared.append.spans {
            mutations.put::<ContentByteSpansCodec>(
                &ContentByteSpanKey {
                    owner: span.content_id(),
                    start: span.start(),
                },
                span,
            )?;
        }
        for span in &prepared.append.text_spans {
            mutations.put::<ContentTextSpansCodec>(
                &ContentTextSpanKey {
                    owner: span.content_id(),
                    logical_start: span.logical_start(),
                },
                span,
            )?;
        }
        for piece in &prepared.append.pieces {
            mutations.put::<ContentPiecesCodec>(
                &ContentPieceKey {
                    owner: piece.content_id(),
                    ordinal: piece.ordinal(),
                },
                piece,
            )?;
        }
        mutations
            .put::<ContentManifestsCodec>(&prepared.append.next.id(), &prepared.append.next)?;
        Ok(())
    }
}
