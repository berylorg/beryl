use beryl_backend::{
    STREAMED_TEXT_MAX_PAGE_BYTES, StreamedInputSourceError, StreamedInputSourceIdentity,
    StreamedInputSourceRevision,
};
use beryl_home_store::{CursorReadLimits, HomeStore};
use beryl_model::SealedAssetReferenceSetProof;
use beryl_state::{
    ASSET_REFERENCE_PAGE_MAX_STORED_BYTES, AssetLabelDisposition, AssetMetadataRecord,
    AssetOwnerHeadRecord, AssetReferenceEntryRecord, AssetReferenceOrdinal, AssetSidecarState,
    AssetState, RecordRevision,
};
use syndic_storage::{
    ContentEncoding, ContentReference, SyndicContentTextSegment, SyndicContentTextSegmentBoundary,
    SyndicContentTextSegmentRangeRead, SyndicStorage,
};

use super::{
    error::MarkerReplayError, identity::marker_source_identity, path::project_runtime_path,
};
use crate::cas_projection::ProjectionCancellationToken;
#[cfg(feature = "test-faults")]
use crate::cas_projection::input_replay::diagnostics::OrdinaryInputReplayDiagnostics;
use crate::cas_projection::input_replay::{InputReplayContext, InputReplayRecord, point_limit};

pub(super) struct MarkerSource {
    context: InputReplayContext,
    record: InputReplayRecord,
    content: ContentReference,
    asset_proof: SealedAssetReferenceSetProof,
    owner_head: AssetOwnerHeadRecord,
    assets: AssetState,
    source_identity: StreamedInputSourceIdentity,
    source_revision: StreamedInputSourceRevision,
    #[cfg(feature = "test-faults")]
    diagnostics: OrdinaryInputReplayDiagnostics,
}

impl MarkerSource {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        store: &HomeStore,
        storage: SyndicStorage,
        assets: AssetState,
        context: InputReplayContext,
        record: InputReplayRecord,
        content: ContentReference,
        asset_proof: SealedAssetReferenceSetProof,
        owner_head: AssetOwnerHeadRecord,
        cancellation: &ProjectionCancellationToken,
        #[cfg(feature = "test-faults")] diagnostics: OrdinaryInputReplayDiagnostics,
    ) -> Result<Self, MarkerReplayError> {
        let summary = content.summary();
        if content.encoding() != ContentEncoding::ComposerV1
            || summary.image_marker_count() == 0
            || content
                .sealed_marker_summary()
                .map_err(|_| MarkerReplayError::InvalidSource)?
                .sequential()
                != asset_proof.sequential()
            || asset_proof.entry_frontier() != summary.image_marker_count()
            || owner_head.owner() != record.asset_owner()
            || owner_head.set() != asset_proof
        {
            return Err(MarkerReplayError::InvalidSource);
        }
        let source_identity = marker_source_identity(
            context.home_id(),
            context.home_generation(),
            &record,
            content,
            asset_proof,
            owner_head.owner_revision(),
            context.runtime_mode(),
        );
        let source = Self {
            context,
            record,
            content,
            asset_proof,
            owner_head,
            assets,
            source_identity,
            source_revision: StreamedInputSourceRevision::new(content.revision().get()),
            #[cfg(feature = "test-faults")]
            diagnostics,
        };
        source.check_authority(store, storage, cancellation)?;
        Ok(source)
    }

    pub(super) fn fresh(&self) -> Self {
        Self {
            context: self.context.clone(),
            record: self.record.clone(),
            content: self.content,
            asset_proof: self.asset_proof,
            owner_head: self.owner_head.clone(),
            assets: self.assets,
            source_identity: self.source_identity,
            source_revision: self.source_revision,
            #[cfg(feature = "test-faults")]
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub(super) const fn source_identity(&self) -> StreamedInputSourceIdentity {
        self.source_identity
    }

    pub(super) const fn source_revision(&self) -> StreamedInputSourceRevision {
        self.source_revision
    }

    pub(super) const fn record(&self) -> &InputReplayRecord {
        &self.record
    }

    pub(super) const fn content(&self) -> ContentReference {
        self.content
    }

    pub(super) const fn asset_proof(&self) -> SealedAssetReferenceSetProof {
        self.asset_proof
    }

    pub(super) const fn owner_revision(&self) -> RecordRevision {
        self.owner_head.owner_revision()
    }

    #[cfg(feature = "test-faults")]
    pub(super) const fn diagnostics(&self) -> &OrdinaryInputReplayDiagnostics {
        &self.diagnostics
    }

    pub(super) fn check_authority(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), MarkerReplayError> {
        check_cancelled(cancellation)?;
        self.check_home(store)?;
        self.record
            .check_durable_source(store, storage)
            .map_err(|error| match error {
                StreamedInputSourceError::InvalidSource => MarkerReplayError::InvalidSource,
                _ => MarkerReplayError::ReadUnavailable,
            })?;

        {
            let manifest = storage
                .content_manifest(store, self.content.id(), point_limit())?
                .ok_or(MarkerReplayError::ReadUnavailable)?;
            check_cancelled(cancellation)?;
            let actual_revision = StreamedInputSourceRevision::new(manifest.revision().get());
            if actual_revision != self.source_revision {
                return Err(MarkerReplayError::RevisionDrift {
                    expected: self.source_revision,
                    actual: actual_revision,
                });
            }
            if manifest.sealed_reference() != Some(self.content) {
                return Err(MarkerReplayError::InvalidSource);
            }
        }

        {
            let actual_head = self.assets.owner_head(store, self.record.asset_owner())?;
            check_cancelled(cancellation)?;
            if actual_head.as_ref() != Some(&self.owner_head) {
                return Err(MarkerReplayError::InvalidSource);
            }
        }
        {
            self.assets
                .sealed_reference_set_manifest(store, self.asset_proof)?;
            check_cancelled(cancellation)?;
        }
        Ok(())
    }

    pub(super) fn prove_segment(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        after_marker: Option<SyndicContentTextSegmentBoundary>,
    ) -> Result<SyndicContentTextSegment, MarkerReplayError> {
        check_cancelled(cancellation)?;
        let segment = storage
            .prove_sealed_content_text_segment(store, self.content, after_marker)?
            .ok_or(MarkerReplayError::ReadUnavailable)?;
        check_cancelled(cancellation)?;
        if segment.content() != self.content || segment.preceding_marker() != after_marker {
            return Err(MarkerReplayError::InvalidSource);
        }
        Ok(segment)
    }

    pub(super) fn read_segment_range(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        segment: &SyndicContentTextSegment,
        start: u64,
        maximum: usize,
    ) -> Result<SyndicContentTextSegmentRangeRead, MarkerReplayError> {
        if maximum == 0 || maximum > STREAMED_TEXT_MAX_PAGE_BYTES {
            return Err(MarkerReplayError::InvalidSource);
        }
        check_cancelled(cancellation)?;
        let page = storage
            .sealed_content_text_segment_range(store, segment, start, maximum)?
            .ok_or(MarkerReplayError::ReadUnavailable)?;
        check_cancelled(cancellation)?;
        if page.content() != self.content
            || page.segment_start() != segment.start()
            || page.segment_end() != segment.end()
            || page.start() != start
            || page.text().is_empty()
            || page.text().len() > maximum
        {
            return Err(MarkerReplayError::InvalidSource);
        }
        Ok(page)
    }

    pub(super) fn marker_entry(
        &self,
        store: &HomeStore,
        cancellation: &ProjectionCancellationToken,
        boundary: SyndicContentTextSegmentBoundary,
    ) -> Result<AssetReferenceEntryRecord, MarkerReplayError> {
        let after = boundary
            .marker_ordinal()
            .get()
            .checked_sub(1)
            .and_then(|ordinal| (ordinal != 0).then(|| AssetReferenceOrdinal::new(ordinal).ok()))
            .flatten();
        check_cancelled(cancellation)?;
        let page = self.assets.reference_set_entries(
            store,
            self.asset_proof,
            after,
            CursorReadLimits::new(1, ASSET_REFERENCE_PAGE_MAX_STORED_BYTES)
                .expect("one-entry asset page limits are nonzero"),
        )?;
        check_cancelled(cancellation)?;
        let [entry] = page.records() else {
            return Err(MarkerReplayError::InvalidSource);
        };
        let expected_has_more = boundary.marker_ordinal().get() < self.asset_proof.entry_frontier();
        if page.has_more() != expected_has_more
            || entry.set_id() != self.asset_proof.set_id()
            || entry.ordinal().get() != boundary.marker_ordinal().get()
            || entry.marker_id() != boundary.marker_id()
            || entry.label() != boundary.label()
        {
            return Err(MarkerReplayError::InvalidSource);
        }
        if let AssetLabelDisposition::Repeated { first_ordinal } = entry.label_disposition()
            && first_ordinal >= entry.ordinal()
        {
            return Err(MarkerReplayError::InvalidSource);
        }
        Ok(entry.clone())
    }

    pub(super) fn require_entry_eof(
        &self,
        store: &HomeStore,
        cancellation: &ProjectionCancellationToken,
        after_marker: Option<SyndicContentTextSegmentBoundary>,
    ) -> Result<(), MarkerReplayError> {
        let after = after_marker
            .map(|marker| AssetReferenceOrdinal::new(marker.marker_ordinal().get()))
            .transpose()
            .map_err(|_| MarkerReplayError::InvalidSource)?;
        check_cancelled(cancellation)?;
        let page = self.assets.reference_set_entries(
            store,
            self.asset_proof,
            after,
            CursorReadLimits::new(1, ASSET_REFERENCE_PAGE_MAX_STORED_BYTES)
                .expect("one-entry asset page limits are nonzero"),
        )?;
        check_cancelled(cancellation)?;
        if !page.records().is_empty() || page.has_more() {
            return Err(MarkerReplayError::InvalidSource);
        }
        Ok(())
    }

    pub(super) fn validate_marker_entry(
        &self,
        store: &HomeStore,
        storage: SyndicStorage,
        cancellation: &ProjectionCancellationToken,
        entry: &AssetReferenceEntryRecord,
    ) -> Result<(), MarkerReplayError> {
        if entry.label_disposition() != AssetLabelDisposition::First {
            return Ok(());
        }

        check_cancelled(cancellation)?;
        let origin_set = {
            let origin = storage
                .resolve_image_label_origin_span(
                    store,
                    self.record.thread_id(),
                    entry.label(),
                    point_limit(),
                )?
                .ok_or(MarkerReplayError::InvalidSource)?;
            check_cancelled(cancellation)?;
            origin.span().asset_reference_set()
        };
        {
            let first = self
                .assets
                .label_first_reference(store, origin_set, entry.label())?
                .ok_or(MarkerReplayError::InvalidSource)?;
            check_cancelled(cancellation)?;
            if first.label() != entry.label()
                || first.asset_id() != entry.asset_id()
                || first.label_disposition() != AssetLabelDisposition::First
            {
                return Err(MarkerReplayError::InvalidSource);
            }
        }
        self.require_metadata(store, cancellation, entry)?;
        Ok(())
    }

    pub(super) fn verified_runtime_path(
        &self,
        store: &HomeStore,
        cancellation: &ProjectionCancellationToken,
        entry: &AssetReferenceEntryRecord,
    ) -> Result<Box<str>, MarkerReplayError> {
        if entry.label_disposition() != AssetLabelDisposition::First {
            return Err(MarkerReplayError::InvalidSource);
        }
        self.require_metadata(store, cancellation, entry)?;
        check_cancelled(cancellation)?;
        #[cfg(feature = "test-faults")]
        self.diagnostics.record_sidecar_verification();
        let verified = self.assets.verify_sidecar(store, entry.asset_id())?;
        check_cancelled(cancellation)?;
        let path = project_runtime_path(verified.path(), self.context.runtime_mode())?;
        drop(verified);
        Ok(path)
    }

    fn require_metadata(
        &self,
        store: &HomeStore,
        cancellation: &ProjectionCancellationToken,
        entry: &AssetReferenceEntryRecord,
    ) -> Result<AssetMetadataRecord, MarkerReplayError> {
        check_cancelled(cancellation)?;
        let metadata = self
            .assets
            .metadata(store, entry.asset_id())?
            .ok_or(MarkerReplayError::InvalidSource)?;
        check_cancelled(cancellation)?;
        if metadata.asset_id() != entry.asset_id()
            || metadata.sidecar_state() != AssetSidecarState::Committed
        {
            return Err(MarkerReplayError::InvalidSource);
        }
        Ok(metadata)
    }

    fn check_home(&self, store: &HomeStore) -> Result<(), MarkerReplayError> {
        self.context
            .check_home_source(store, self.source_identity, |home_id, generation| {
                marker_source_identity(
                    home_id,
                    generation,
                    &self.record,
                    self.content,
                    self.asset_proof,
                    self.owner_head.owner_revision(),
                    self.context.runtime_mode(),
                )
            })
            .map_err(|error| match error {
                StreamedInputSourceError::SourceIdentityMismatch { expected, actual } => {
                    MarkerReplayError::SourceIdentityMismatch { expected, actual }
                }
                _ => MarkerReplayError::ReadUnavailable,
            })
    }
}

pub(super) fn check_cancelled(
    cancellation: &ProjectionCancellationToken,
) -> Result<(), MarkerReplayError> {
    if cancellation.is_cancelled() {
        Err(MarkerReplayError::Cancelled)
    } else {
        Ok(())
    }
}
