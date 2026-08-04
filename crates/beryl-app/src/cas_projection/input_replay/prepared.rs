use beryl_backend::{
    StreamedInputDescriptor, StreamedInputDescriptorKind, StreamedInputHeader,
    StreamedInputSequenceDigestAccumulator, StreamedInputSourceError, StreamedInputSourceIdentity,
    StreamedInputSourceRevision, StreamedTextDescriptor, StreamedTextSourceId, TextSourceProof,
};
use beryl_home_store::{HomeGeneration, HomeStore};
use beryl_model::BerylHomeId;
use beryl_state::AssetState;
use sha2::{Digest, Sha256};
use syndic_storage::{ContentEncoding, ContentReference, SyndicReadError, SyndicStorage};

use crate::cas_projection::ProjectionCancellationToken;
use crate::cas_projection::connection::StreamedInputBrokerService;
#[cfg(feature = "test-faults")]
use crate::cas_projection::input_replay::diagnostics::OrdinaryInputReplayDiagnostics;
use crate::cas_projection::input_replay::{
    InputReplayContext, InputReplayPrepareError, InputReplayRecord, point_limit,
};

const ITEM_COUNT: u64 = 1;
const TEXT_ITEM_ORDINAL: u64 = 1;

/// Compact durable authority for one marker-free submitted or accepted text value.
pub(in crate::cas_projection) struct TextReplayAuthority {
    context: InputReplayContext,
    record: InputReplayRecord,
    assets: AssetState,
    content: ContentReference,
    header: StreamedInputHeader,
    source_id: StreamedTextSourceId,
    proof: TextSourceProof,
    pass_open: bool,
    descriptor_emitted: bool,
    #[cfg(feature = "test-faults")]
    diagnostics: OrdinaryInputReplayDiagnostics,
}

impl TextReplayAuthority {
    pub(in crate::cas_projection) fn prepare(
        context: InputReplayContext,
        record: InputReplayRecord,
        assets: AssetState,
        content: ContentReference,
        #[cfg(feature = "test-faults")] diagnostics: OrdinaryInputReplayDiagnostics,
    ) -> Result<Self, InputReplayPrepareError> {
        let summary = content.summary();
        if summary.image_marker_count() != 0 {
            return Err(InputReplayPrepareError::AssetReferenceSetMismatch);
        }
        let utf8_len = summary.logical_utf8_bytes();
        if utf8_len == 0 {
            return Err(InputReplayPrepareError::EmptyInput);
        }

        let source_identity = source_identity(
            context.home_id(),
            context.home_generation(),
            &record,
            content,
        );
        let source_revision = StreamedInputSourceRevision::new(content.revision().get());
        let source_id = text_source_id(source_identity, TEXT_ITEM_ORDINAL);
        let proof = text_source_proof(source_identity, source_revision, &record, content);
        let mut digest = StreamedInputSequenceDigestAccumulator::new(ITEM_COUNT);
        digest
            .push_text(TEXT_ITEM_ORDINAL, proof, utf8_len)
            .map_err(|_| InputReplayPrepareError::DescriptorInvalid)?;
        let sequence_digest = digest
            .finish()
            .map_err(|_| InputReplayPrepareError::DescriptorInvalid)?;
        Ok(Self {
            context,
            record,
            assets,
            content,
            header: StreamedInputHeader::new(
                source_identity,
                source_revision,
                ITEM_COUNT,
                sequence_digest,
            ),
            source_id,
            proof,
            pass_open: false,
            descriptor_emitted: false,
            #[cfg(feature = "test-faults")]
            diagnostics,
        })
    }

    pub(in crate::cas_projection) const fn header(&self) -> StreamedInputHeader {
        self.header
    }

    pub(in crate::cas_projection) fn fresh(&self) -> Self {
        Self {
            context: self.context.clone(),
            record: self.record.clone(),
            assets: self.assets,
            content: self.content,
            header: self.header,
            source_id: self.source_id,
            proof: self.proof,
            pass_open: false,
            descriptor_emitted: false,
            #[cfg(feature = "test-faults")]
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub(in crate::cas_projection) fn service<'a>(
        &'a mut self,
        store: &'a HomeStore,
        storage: SyndicStorage,
        cancellation: &'a ProjectionCancellationToken,
    ) -> TextReplayService<'a> {
        TextReplayService {
            authority: self,
            store,
            storage,
            cancellation,
        }
    }

    pub(in crate::cas_projection) fn begin_pass(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        if self.pass_open {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        self.check_authority(store, storage, cancellation)?;
        self.pass_open = true;
        self.descriptor_emitted = false;
        Ok(self.header)
    }

    pub(in crate::cas_projection) fn next_descriptor(
        &mut self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        self.check_authority(store, storage, cancellation)?;
        if !self.pass_open {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        if self.descriptor_emitted {
            self.pass_open = false;
            return Ok(None);
        }
        self.descriptor_emitted = true;
        Ok(Some(StreamedInputDescriptor::new(
            self.header.source_identity(),
            self.header.source_revision(),
            TEXT_ITEM_ORDINAL,
            StreamedInputDescriptorKind::Text(StreamedTextDescriptor::new(
                self.source_id,
                self.proof,
                self.content.summary().logical_utf8_bytes(),
            )),
        )))
    }

    pub(super) fn check_page_authority(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        source_id: StreamedTextSourceId,
    ) -> Result<(), StreamedInputSourceError> {
        self.check_authority(store, storage, cancellation)?;
        if !self.pass_open || !self.descriptor_emitted {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        if source_id != self.source_id {
            return Err(StreamedInputSourceError::TextSourceIdMismatch {
                item_ordinal: TEXT_ITEM_ORDINAL,
            });
        }
        Ok(())
    }

    pub(super) const fn content(&self) -> ContentReference {
        self.content
    }

    pub(super) const fn source_id(&self) -> StreamedTextSourceId {
        self.source_id
    }

    pub(super) const fn proof(&self) -> TextSourceProof {
        self.proof
    }

    #[cfg(feature = "test-faults")]
    pub(super) const fn diagnostics(&self) -> &OrdinaryInputReplayDiagnostics {
        &self.diagnostics
    }

    fn check_authority(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), StreamedInputSourceError> {
        check_source_cancelled(cancellation)?;
        self.check_home(store)?;
        self.record.check_durable_source(store, storage)?;
        if self
            .assets
            .owner_head(store, self.record.asset_owner())
            .map_err(|_| StreamedInputSourceError::ReadFailed)?
            .is_some()
        {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        let manifest = storage
            .content_manifest(store, self.content.id(), point_limit())
            .map_err(map_read_error)?
            .ok_or(StreamedInputSourceError::ReadFailed)?;
        let actual_revision = StreamedInputSourceRevision::new(manifest.revision().get());
        if actual_revision != self.header.source_revision() {
            return Err(StreamedInputSourceError::RevisionDrift {
                expected: self.header.source_revision(),
                actual: actual_revision,
            });
        }
        if manifest.sealed_reference() != Some(self.content) {
            return Err(StreamedInputSourceError::InvalidSource);
        }
        Ok(())
    }

    fn check_home(&self, store: &HomeStore) -> Result<(), StreamedInputSourceError> {
        self.context.check_home_source(
            store,
            self.header.source_identity(),
            |home_id, generation| source_identity(home_id, generation, &self.record, self.content),
        )
    }
}

pub(in crate::cas_projection) struct TextReplayService<'a> {
    authority: &'a mut TextReplayAuthority,
    store: &'a HomeStore,
    storage: SyndicStorage,
    cancellation: &'a ProjectionCancellationToken,
}

impl StreamedInputBrokerService for TextReplayService<'_> {
    fn header(&self) -> StreamedInputHeader {
        self.authority.header()
    }

    fn begin_pass(&mut self) -> Result<StreamedInputHeader, StreamedInputSourceError> {
        self.authority
            .begin_pass(self.store, self.storage, self.cancellation)
    }

    fn next_descriptor(
        &mut self,
    ) -> Result<Option<StreamedInputDescriptor>, StreamedInputSourceError> {
        self.authority
            .next_descriptor(self.store, self.storage, self.cancellation)
    }

    fn read_text_page(
        &mut self,
        source_id: StreamedTextSourceId,
        start: u64,
        max_utf8_bytes: usize,
    ) -> Result<beryl_backend::StreamedTextPage, StreamedInputSourceError> {
        self.authority.read_page(
            self.store,
            self.storage,
            self.cancellation,
            source_id,
            start,
            max_utf8_bytes,
        )
    }
}

#[cfg(feature = "test-faults")]
impl TextReplayService<'_> {
    pub(super) const fn diagnostics(&self) -> &OrdinaryInputReplayDiagnostics {
        self.authority.diagnostics()
    }
}

pub(in crate::cas_projection) fn check_cancelled(
    cancellation: &ProjectionCancellationToken,
) -> Result<(), InputReplayPrepareError> {
    if cancellation.is_cancelled() {
        Err(InputReplayPrepareError::Cancelled)
    } else {
        Ok(())
    }
}

fn check_source_cancelled(
    cancellation: &ProjectionCancellationToken,
) -> Result<(), StreamedInputSourceError> {
    if cancellation.is_cancelled() {
        Err(StreamedInputSourceError::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn map_read_error(error: SyndicReadError) -> StreamedInputSourceError {
    match error {
        SyndicReadError::Read(_) => StreamedInputSourceError::ReadFailed,
        SyndicReadError::ConcurrentChange { .. } => StreamedInputSourceError::ReadFailed,
        SyndicReadError::InvalidContentTextOffset { .. }
        | SyndicReadError::InvalidContentTextSegmentCursor { .. }
        | SyndicReadError::InvalidContentTextSegmentOffset { .. }
        | SyndicReadError::ContentTextReadLimitTooSmall { .. }
        | SyndicReadError::ContentTextContainsImageMarkers { .. } => {
            StreamedInputSourceError::MalformedTextSegmentation {
                item_ordinal: TEXT_ITEM_ORDINAL,
            }
        }
        _ => StreamedInputSourceError::InvalidSource,
    }
}

fn source_identity(
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    record: &InputReplayRecord,
    content: ContentReference,
) -> StreamedInputSourceIdentity {
    let summary = content.summary();
    let mut digest = Sha256::new();
    digest.update(b"beryl.app.input-replay-source.v1\0");
    digest.update(home_id.as_bytes());
    digest.update(home_generation.get().to_be_bytes());
    record.hash_into(&mut digest);
    digest.update(content.id().as_bytes());
    digest.update(content.revision().get().to_be_bytes());
    digest.update([encoding_tag(content.encoding())]);
    digest.update(summary.chunk_count().to_be_bytes());
    digest.update(summary.piece_count().to_be_bytes());
    digest.update(summary.encoded_bytes().to_be_bytes());
    digest.update(summary.logical_utf8_bytes().to_be_bytes());
    digest.update(summary.atom_count().to_be_bytes());
    digest.update(summary.image_marker_count().to_be_bytes());
    digest.update(summary.marker_digest());
    digest.update(summary.digest().as_bytes());
    StreamedInputSourceIdentity::new(digest.finalize().into())
}

fn text_source_id(
    source_identity: StreamedInputSourceIdentity,
    item_ordinal: u64,
) -> StreamedTextSourceId {
    let mut digest = Sha256::new();
    digest.update(b"beryl.app.input-replay-text-source.v1\0");
    digest.update(source_identity.as_bytes());
    digest.update(item_ordinal.to_be_bytes());
    StreamedTextSourceId::new(digest.finalize().into())
}

fn text_source_proof(
    source_identity: StreamedInputSourceIdentity,
    source_revision: StreamedInputSourceRevision,
    record: &InputReplayRecord,
    content: ContentReference,
) -> TextSourceProof {
    let mut digest = Sha256::new();
    digest.update(b"beryl.app.input-replay-text-proof.v1\0");
    digest.update(source_identity.as_bytes());
    digest.update(source_revision.get().to_be_bytes());
    record.hash_into(&mut digest);
    digest.update(content.id().as_bytes());
    digest.update(content.summary().digest().as_bytes());
    digest.update(content.summary().logical_utf8_bytes().to_be_bytes());
    TextSourceProof::new(digest.finalize().into())
}

const fn encoding_tag(encoding: ContentEncoding) -> u8 {
    match encoding {
        ContentEncoding::ComposerV1 => 1,
        ContentEncoding::Utf8V1 => 2,
        ContentEncoding::ProviderItemV1 => 3,
    }
}
