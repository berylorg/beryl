use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuilder, MutationContribution, ReconciliationReservation,
};
use beryl_model::{DomainRevision, DraftRevision, SyndicContentId, SyndicDraftId, SyndicThreadId};

use crate::{
    ContentChunkRecord, ContentLifecycle, ContentManifestRecord, ContentReference, ContentSummary,
    DraftByThreadRecord, DraftRecord, HistorySummaryRecord, PreparedContent, SyndicCurrentDraft,
    SyndicStorage, SyndicTimestamp, advance_content_chain, codec::*, domain::SyndicDomain,
};

mod validation;

use validation::{validate_prepared_manifest, validate_publishable};

use super::{SyndicMutationError, current_draft, point, required};

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

/// Dirty-only decision against one stabilized current draft metadata record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPayloadUpdateDecision {
    NoChange,
    Update(DraftPayloadUpdate),
}

/// Final atomic publication of one complete content object as a draft revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPayloadUpdate {
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    expected_revision: DraftRevision,
    content_id: SyndicContentId,
    content_summary: ContentSummary,
    updated_at: SyndicTimestamp,
}

impl DraftPayloadUpdate {
    /// Prepares a dirty draft revision from newly prepared in-memory content.
    pub fn prepare(
        current: &SyndicCurrentDraft,
        content: &PreparedContent,
        updated_at: SyndicTimestamp,
    ) -> Result<DraftPayloadUpdateDecision, SyndicMutationError> {
        Self::prepare_fields(
            current,
            content.id(),
            content.encoding(),
            content.summary(),
            updated_at,
        )
    }

    /// Prepares a dirty draft revision from an exact already-sealed content reference.
    ///
    /// This is the bounded publication boundary for content constructed directly in durable
    /// storage. The mutation still validates the referenced manifest before publishing the draft.
    pub fn prepare_reference(
        current: &SyndicCurrentDraft,
        content: ContentReference,
        updated_at: SyndicTimestamp,
    ) -> Result<DraftPayloadUpdateDecision, SyndicMutationError> {
        Self::prepare_fields(
            current,
            content.id(),
            content.encoding(),
            content.summary(),
            updated_at,
        )
    }

    fn prepare_fields(
        current: &SyndicCurrentDraft,
        content_id: SyndicContentId,
        content_encoding: crate::ContentEncoding,
        content_summary: ContentSummary,
        updated_at: SyndicTimestamp,
    ) -> Result<DraftPayloadUpdateDecision, SyndicMutationError> {
        let current_content = current.draft().content();
        if current_content.id() == content_id
            && current_content.encoding() == content_encoding
            && current_content.summary() == content_summary
        {
            return Ok(DraftPayloadUpdateDecision::NoChange);
        }
        if updated_at < current.draft().updated_at() {
            return Err(SyndicMutationError::TimestampRegressed);
        }
        Ok(DraftPayloadUpdateDecision::Update(Self {
            thread_id: current.thread().id(),
            draft_id: current.draft().id(),
            expected_revision: current.draft().revision(),
            content_id,
            content_summary,
            updated_at,
        }))
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    #[must_use]
    pub const fn expected_revision(&self) -> DraftRevision {
        self.expected_revision
    }
    #[must_use]
    pub const fn content_id(&self) -> SyndicContentId {
        self.content_id
    }

    #[must_use]
    pub fn matches_committed(&self, current: &SyndicCurrentDraft) -> bool {
        let Some(expected_revision) = self.expected_revision.get().checked_add(1) else {
            return false;
        };
        let content = current.draft().content();
        current.thread().id() == self.thread_id
            && current.draft().id() == self.draft_id
            && current.draft().revision().get() == expected_revision
            && content.id() == self.content_id
            && content.summary() == self.content_summary
            && current.draft().updated_at() == self.updated_at
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

    #[must_use]
    pub fn update_draft_payload(
        &self,
        expected_domain_revision: DomainRevision,
        update: DraftPayloadUpdate,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, PublishDraftMutation { update })
    }
}

struct BeginContentMutation {
    build: ContentBuild,
}
struct AppendContentMutation {
    append: ContentAppend,
}
struct PublishDraftMutation {
    update: DraftPayloadUpdate,
}

impl DomainMutation<SyndicDomain> for BeginContentMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if point::<ContentManifestsFamily>(reader, &self.build.manifest.id())?.is_some() {
            return Err(SyndicMutationError::ContentIdentityCollision);
        }
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ContentManifestsCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ContentManifestsCodec>(&self.build.manifest.id(), &self.build.manifest)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for AppendContentMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
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
        Ok(())
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
        &self,
        _reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        for chunk in &self.append.chunks {
            mutations.put::<ContentChunksCodec>(
                &ContentChunkKey {
                    owner: chunk.content_id(),
                    ordinal: chunk.ordinal(),
                },
                chunk,
            )?;
        }
        for span in &self.append.spans {
            mutations.put::<ContentByteSpansCodec>(
                &ContentByteSpanKey {
                    owner: span.content_id(),
                    start: span.start(),
                },
                span,
            )?;
        }
        for span in &self.append.text_spans {
            mutations.put::<ContentTextSpansCodec>(
                &ContentTextSpanKey {
                    owner: span.content_id(),
                    logical_start: span.logical_start(),
                },
                span,
            )?;
        }
        for piece in &self.append.pieces {
            mutations.put::<ContentPiecesCodec>(
                &ContentPieceKey {
                    owner: piece.content_id(),
                    ordinal: piece.ordinal(),
                },
                piece,
            )?;
        }
        mutations.put::<ContentManifestsCodec>(&self.append.next.id(), &self.append.next)?;
        Ok(())
    }
}

impl DomainMutation<SyndicDomain> for PublishDraftMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        let current = current_draft(reader, self.update.thread_id)?;
        if current.id() != self.update.draft_id {
            return Err(SyndicMutationError::CurrentDraftConflict);
        }
        if current.revision() != self.update.expected_revision {
            return Err(SyndicMutationError::DraftRevisionConflict {
                expected: self.update.expected_revision,
                current: current.revision(),
            });
        }
        if current.content().id() == self.update.content_id
            && current.content().summary() == self.update.content_summary
        {
            return Err(SyndicMutationError::UnchangedPayload);
        }
        if self.update.updated_at < current.updated_at() {
            return Err(SyndicMutationError::TimestampRegressed);
        }
        let manifest = required::<ContentManifestsFamily>(reader, &self.update.content_id)?;
        validate_publishable(&manifest, self.update.content_summary)?;
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ContentManifestsCodec>(1)?;
        reservation.reserve_records::<DraftsCodec>(1)?;
        reservation.reserve_records::<DraftByThreadCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let thread = required::<ThreadsFamily>(reader, &self.update.thread_id)?;
        let current = current_draft(reader, self.update.thread_id)?;
        let manifest = required::<ContentManifestsFamily>(reader, &self.update.content_id)?;
        let content = match manifest.lifecycle() {
            ContentLifecycle::Sealed => manifest
                .sealed_reference()
                .ok_or(SyndicMutationError::ContentManifestConflict)?,
            ContentLifecycle::Building => {
                let sealed = ContentManifestRecord::new(
                    manifest.id(),
                    manifest.revision().checked_next()?,
                    manifest.encoding(),
                    ContentLifecycle::Sealed,
                    manifest.chunk_count(),
                    manifest.encoded_bytes(),
                    manifest.chain_digest(),
                    manifest.expected(),
                );
                let reference = sealed
                    .sealed_reference()
                    .ok_or(SyndicMutationError::ContentManifestConflict)?;
                mutations.put::<ContentManifestsCodec>(&sealed.id(), &sealed)?;
                reference
            }
            ContentLifecycle::Live | ContentLifecycle::Finalized => {
                return Err(SyndicMutationError::ContentManifestConflict);
            }
        };
        let revision = current.revision().checked_next()?;
        let next = DraftRecord::new(
            current.id(),
            current.thread_id(),
            revision,
            current.submission_intent(),
            content,
            current.created_at(),
            self.update.updated_at,
        );
        let index = DraftByThreadRecord::new(thread.id(), next.id(), revision, thread.revision());
        let current_summary = required::<HistorySummariesFamily>(reader, &thread.id())?;
        let next_activity = current_summary
            .last_activity_at()
            .max(self.update.updated_at);
        mutations.put::<DraftsCodec>(&next.id(), &next)?;
        mutations.put::<DraftByThreadCodec>(&thread.id(), &index)?;
        if next_activity != current_summary.last_activity_at() {
            let summary = HistorySummaryRecord::new(
                current_summary.thread_id(),
                current_summary.revision().checked_next()?,
                current_summary.thread_revision(),
                current_summary.committed_tail(),
                current_summary.selected_path_digest(),
                current_summary.complete(),
                next_activity,
            );
            mutations.put::<HistorySummariesCodec>(&thread.id(), &summary)?;
        }
        Ok(())
    }
}
