use std::num::{NonZeroU64, NonZeroUsize};

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../tests/unit/image_source.rs"]
mod tests;

use beryl_home_store::{
    CommandCancellation, CursorReadLimits, HomeGenerationIdentity, HomeServiceReference,
    SidecarAddress, SidecarByteLimit, SidecarDigest, SidecarError, SidecarNamespace,
    VerifiedSidecarReader,
};
use beryl_model::{
    AssetId, ProjectionRevision, SyndicDraftMarkerId, SyndicProjectionId, SyndicThreadId,
};
use beryl_state::{AssetLabelDisposition, AssetMetadataRecord, AssetSidecarState, AssetState};
use syndic_storage::{
    CurrentTranscriptEntryProof, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1, DraftPieceRangeSourceErrorV1, ProjectionLifecycle, ProjectionPayload,
    SyndicPointReadLimit, SyndicStorage, TranscriptPosition,
};

use crate::{composer_host::ComposerHostBinding, syndic_transcript::TranscriptImageMarker};

use super::MediaResourceLimits;

const SOURCE_RECORD_BYTES: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageSourceDemand {
    Composer {
        binding: ComposerHostBinding,
        marker_id: SyndicDraftMarkerId,
        order_key: u64,
    },
    TranscriptMarker {
        thread_id: SyndicThreadId,
        entry: CurrentTranscriptEntryProof,
        projection_id: SyndicProjectionId,
        projection_revision: ProjectionRevision,
        marker: TranscriptImageMarker,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ImageSourceError {
    #[error("image source request was cancelled")]
    Cancelled,
    #[error("image source occurrence is stale")]
    Stale,
    #[error("image source occurrence does not match its authority")]
    Identity,
    #[error("image source metadata is unavailable")]
    Unavailable,
    #[error("image source format or dimensions are unsupported")]
    Unsupported,
    #[error("image source exceeds the configured capacity")]
    Capacity,
    #[error(transparent)]
    Sidecar(#[from] SidecarError),
}

pub(crate) struct ResolvedImageSource {
    scope: HomeGenerationIdentity,
    metadata: AssetMetadataRecord,
    dimensions: [u32; 2],
}

pub(crate) struct ResolvedOriginalImageSource {
    scope: HomeGenerationIdentity,
    metadata: AssetMetadataRecord,
}

impl ResolvedOriginalImageSource {
    pub(crate) fn asset_id(&self) -> AssetId {
        self.metadata.asset_id()
    }

    pub(crate) fn metadata(&self) -> &AssetMetadataRecord {
        &self.metadata
    }
}

impl ResolvedImageSource {
    pub(crate) fn asset_id(&self) -> AssetId {
        self.metadata.asset_id()
    }

    pub(crate) fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }

    pub(crate) fn metadata(&self) -> &AssetMetadataRecord {
        &self.metadata
    }
}

pub(crate) struct ImageSourceProvider {
    home: HomeServiceReference,
    scope: HomeGenerationIdentity,
    syndic: SyndicStorage,
    assets: AssetState,
    limits: MediaResourceLimits,
}

impl ImageSourceProvider {
    pub(crate) fn prepare_initial(
        candidate: &beryl_home_store::HomeOpenPublication,
        syndic: SyndicStorage,
        assets: AssetState,
        limits: MediaResourceLimits,
    ) -> Result<Self, ImageSourceError> {
        Ok(Self {
            home: candidate.service_reference(),
            scope: candidate
                .generation_identity()
                .map_err(|_| ImageSourceError::Unavailable)?,
            syndic,
            assets,
            limits,
        })
    }

    pub(crate) fn prepare_recovery(
        candidate: &beryl_home_store::HomeRecoveryCandidate,
        syndic: SyndicStorage,
        assets: AssetState,
        limits: MediaResourceLimits,
    ) -> Result<Self, ImageSourceError> {
        Ok(Self {
            home: candidate.service_reference(),
            scope: candidate
                .generation_identity()
                .map_err(|_| ImageSourceError::Unavailable)?,
            syndic,
            assets,
            limits,
        })
    }

    pub(crate) fn new(
        home: HomeServiceReference,
        syndic: SyndicStorage,
        assets: AssetState,
        limits: MediaResourceLimits,
    ) -> Result<Self, ImageSourceError> {
        let scope = home
            .generation_identity()
            .map_err(|_| ImageSourceError::Unavailable)?;
        Ok(Self {
            home,
            scope,
            syndic,
            assets,
            limits,
        })
    }

    pub(crate) fn resolve(
        &self,
        demand: &ImageSourceDemand,
        cancellation: &CommandCancellation,
    ) -> Result<ResolvedImageSource, ImageSourceError> {
        let original = self.resolve_original(demand, cancellation)?;
        let dimensions = admitted_dimensions(&original.metadata, self.limits)?;
        Ok(ResolvedImageSource {
            scope: original.scope,
            metadata: original.metadata,
            dimensions,
        })
    }

    pub(crate) fn resolve_original(
        &self,
        demand: &ImageSourceDemand,
        cancellation: &CommandCancellation,
    ) -> Result<ResolvedOriginalImageSource, ImageSourceError> {
        self.check_scope(cancellation)?;
        let asset_id = match *demand {
            ImageSourceDemand::Composer {
                binding,
                marker_id,
                order_key,
            } => self.resolve_composer(binding, marker_id, order_key, cancellation)?,
            ImageSourceDemand::TranscriptMarker {
                thread_id,
                entry,
                projection_id,
                projection_revision,
                marker,
            } => self.resolve_transcript(
                thread_id,
                entry,
                projection_id,
                projection_revision,
                marker,
                cancellation,
            )?,
        };
        check_cancelled(cancellation)?;
        let metadata = self
            .assets
            .metadata(&self.home, asset_id)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Unavailable)?;
        check_cancelled(cancellation)?;
        if metadata.asset_id() != asset_id
            || metadata.sidecar_state() != AssetSidecarState::Committed
        {
            return Err(ImageSourceError::Identity);
        }
        self.check_scope(cancellation)?;
        Ok(ResolvedOriginalImageSource {
            scope: self.scope,
            metadata,
        })
    }

    pub(crate) fn open_verified(
        &self,
        source: &ResolvedImageSource,
        cancellation: CommandCancellation,
    ) -> Result<VerifiedSidecarReader, ImageSourceError> {
        self.open_verified_asset(
            source.scope,
            source.asset_id(),
            self.limits.configuration().rendition_source_max_bytes as u64,
            cancellation,
        )
    }

    pub(crate) fn open_original_verified(
        &self,
        source: &ResolvedOriginalImageSource,
        cancellation: CommandCancellation,
    ) -> Result<VerifiedSidecarReader, ImageSourceError> {
        self.open_verified_asset(
            source.scope,
            source.asset_id(),
            source.asset_id().length().get(),
            cancellation,
        )
    }

    fn open_verified_asset(
        &self,
        scope: HomeGenerationIdentity,
        asset: AssetId,
        maximum_bytes: u64,
        cancellation: CommandCancellation,
    ) -> Result<VerifiedSidecarReader, ImageSourceError> {
        self.check_scope(&cancellation)?;
        if scope != self.scope {
            return Err(ImageSourceError::Stale);
        }
        let config = self.limits.configuration();
        let address = SidecarAddress::new(
            SidecarNamespace::new("images").expect("ordinary asset namespace is valid"),
            SidecarDigest::from_bytes(asset.digest()),
            asset.length().get(),
        );
        let reader = self
            .home
            .open_verified_sidecar_reader(
                &address,
                SidecarByteLimit::new(
                    NonZeroU64::new(maximum_bytes).expect("validated positive source limit"),
                ),
                NonZeroUsize::new(config.sidecar_io_page_bytes)
                    .expect("validated positive source page"),
                cancellation.clone(),
            )
            .map_err(|error| match error {
                SidecarError::Cancelled => ImageSourceError::Cancelled,
                error => ImageSourceError::Sidecar(error),
            })?;
        self.check_scope(&cancellation)?;
        Ok(reader)
    }

    fn check_scope(&self, cancellation: &CommandCancellation) -> Result<(), ImageSourceError> {
        check_cancelled(cancellation)?;
        if self
            .home
            .generation_identity()
            .map_err(|_| ImageSourceError::Stale)?
            != self.scope
        {
            return Err(ImageSourceError::Stale);
        }
        Ok(())
    }

    fn resolve_composer(
        &self,
        binding: ComposerHostBinding,
        marker_id: SyndicDraftMarkerId,
        order_key: u64,
        cancellation: &CommandCancellation,
    ) -> Result<AssetId, ImageSourceError> {
        if binding.home_id() != self.home.home_id()
            || Some(binding.home_generation()) != self.home.health().generation()
        {
            return Err(ImageSourceError::Stale);
        }
        let request = DraftPieceMarkerDemandV1::new(
            DraftPieceMarkerScopeV1::InclusiveRange {
                start: 0,
                end: binding.logical_extent().logical_utf8_bytes(),
            },
            DraftPieceMarkerDirectionV1::Forward,
            None,
            1,
            SOURCE_RECORD_BYTES,
        );
        check_cancelled(cancellation)?;
        let range = self
            .syndic
            .candidate_draft_piece_marker_demand(&self.home, binding.candidate(), request)
            .map_err(candidate_error)?;
        check_cancelled(cancellation)?;
        if range.binding() != binding.candidate() || range.value().root() != binding.root() {
            return Err(ImageSourceError::Identity);
        }
        drop(range);
        let identity = self
            .syndic
            .draft_marker_identity(&self.home, binding.root(), marker_id)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        check_cancelled(cancellation)?;
        if identity.marker_id() != marker_id || identity.order_key() != order_key {
            return Err(ImageSourceError::Identity);
        }
        Ok(identity.asset_id())
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_transcript(
        &self,
        thread_id: SyndicThreadId,
        entry_proof: CurrentTranscriptEntryProof,
        projection_id: SyndicProjectionId,
        projection_revision: ProjectionRevision,
        marker: TranscriptImageMarker,
        cancellation: &CommandCancellation,
    ) -> Result<AssetId, ImageSourceError> {
        let point =
            SyndicPointReadLimit::new(SOURCE_RECORD_BYTES).expect("positive source point bound");
        check_cancelled(cancellation)?;
        let head = self
            .syndic
            .transcript_view_head(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        check_cancelled(cancellation)?;
        let thread = self
            .syndic
            .thread(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        check_cancelled(cancellation)?;
        if head.thread_id() != thread_id
            || head.generation() != entry_proof.generation()
            || head.lifecycle() != ProjectionLifecycle::Current
            || head.committed_tail() != thread.committed_tail()
            || head.selected_path_digest() != thread.selected_path_digest()
            || entry_proof.position().get() > head.entry_count()
        {
            return Err(ImageSourceError::Stale);
        }
        let position = entry_proof.position().get();
        let after = if position > 1 {
            TranscriptPosition::new(position - 1).ok()
        } else {
            None
        };
        let entries = self
            .syndic
            .transcript_entries(
                &self.home,
                thread_id,
                entry_proof.generation(),
                after,
                CursorReadLimits::new(1, SOURCE_RECORD_BYTES)
                    .expect("positive source cursor bound"),
            )
            .map_err(|_| ImageSourceError::Unavailable)?;
        check_cancelled(cancellation)?;
        let entry = entries.records().first().ok_or(ImageSourceError::Stale)?;
        if entry.position() != entry_proof.position()
            || entry.projection_id() != projection_id
            || entry.projection_revision() != projection_revision
        {
            return Err(ImageSourceError::Stale);
        }
        let projection = self
            .syndic
            .projection(&self.home, projection_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        check_cancelled(cancellation)?;
        let item = self
            .syndic
            .canonical_item(&self.home, projection.item_id(), point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Stale)?;
        check_cancelled(cancellation)?;
        if projection.id() != projection_id
            || projection.revision() != projection_revision
            || projection.item_id() != entry.item_id()
            || projection.item_id() != item.id()
            || projection.turn_id() != item.turn_id()
            || entry.item_revision() != item.revision()
        {
            return Err(ImageSourceError::Stale);
        }
        if !matches!(projection.payload(), ProjectionPayload::ImageMarker {
            atom_ordinal, marker_ordinal, source_offset, marker: actual,
        } if *atom_ordinal == marker.atom_ordinal && *marker_ordinal == marker.marker_ordinal
            && *source_offset == marker.source_offset && *actual == marker.marker)
        {
            return Err(ImageSourceError::Identity);
        }
        let origin = self
            .syndic
            .resolve_image_label_origin_span(&self.home, thread_id, marker.marker.label(), point)
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Identity)?;
        check_cancelled(cancellation)?;
        let reference = self
            .assets
            .label_first_reference(
                &self.home,
                origin.span().asset_reference_set(),
                marker.marker.label(),
            )
            .map_err(|_| ImageSourceError::Unavailable)?
            .ok_or(ImageSourceError::Identity)?;
        check_cancelled(cancellation)?;
        if reference.label() != marker.marker.label()
            || reference.label_disposition() != AssetLabelDisposition::First
        {
            return Err(ImageSourceError::Identity);
        }
        let current = self
            .syndic
            .transcript_view_head(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?;
        check_cancelled(cancellation)?;
        if current.as_ref() != Some(&head) {
            return Err(ImageSourceError::Stale);
        }
        let current_thread = self
            .syndic
            .thread(&self.home, thread_id, point)
            .map_err(|_| ImageSourceError::Unavailable)?;
        check_cancelled(cancellation)?;
        if current_thread
            .as_ref()
            .is_none_or(|current| current.revision() != thread.revision())
        {
            return Err(ImageSourceError::Stale);
        }
        Ok(reference.asset_id())
    }
}

fn check_cancelled(cancellation: &CommandCancellation) -> Result<(), ImageSourceError> {
    if cancellation.is_cancelled() {
        Err(ImageSourceError::Cancelled)
    } else {
        Ok(())
    }
}

fn candidate_error(error: DraftPieceRangeSourceErrorV1) -> ImageSourceError {
    match error {
        DraftPieceRangeSourceErrorV1::Absent
        | DraftPieceRangeSourceErrorV1::ConcurrentChange
        | DraftPieceRangeSourceErrorV1::StaleSession
        | DraftPieceRangeSourceErrorV1::StaleCandidate
        | DraftPieceRangeSourceErrorV1::Disposed(_) => ImageSourceError::Stale,
        DraftPieceRangeSourceErrorV1::Limit => ImageSourceError::Capacity,
        DraftPieceRangeSourceErrorV1::Operational(_) => ImageSourceError::Unavailable,
        DraftPieceRangeSourceErrorV1::Malformed(_) | DraftPieceRangeSourceErrorV1::Invariant => {
            ImageSourceError::Identity
        }
    }
}

fn admitted_dimensions(
    metadata: &AssetMetadataRecord,
    limits: MediaResourceLimits,
) -> Result<[u32; 2], ImageSourceError> {
    if metadata.media_type().as_str() != "image/png" {
        return Err(ImageSourceError::Unsupported);
    }
    let dimensions = metadata.dimensions().ok_or(ImageSourceError::Unsupported)?;
    let width = dimensions.width().get();
    let height = dimensions.height().get();
    let config = limits.configuration();
    if metadata.asset_id().length().get() > config.rendition_source_max_bytes as u64
        || width > config.rendition_source_max_dimension as u64
        || height > config.rendition_source_max_dimension as u64
        || width
            .checked_mul(height)
            .is_none_or(|pixels| pixels > config.rendition_source_max_pixels as u64)
    {
        return Err(ImageSourceError::Capacity);
    }
    Ok([
        u32::try_from(width).map_err(|_| ImageSourceError::Capacity)?,
        u32::try_from(height).map_err(|_| ImageSourceError::Capacity)?,
    ])
}
