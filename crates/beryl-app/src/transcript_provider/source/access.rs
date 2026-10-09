use super::*;
use beryl_home_store::HomeCandidateRecoveryAccess;
use syndic_storage::*;

pub(super) enum ActivationReadSource<'a> {
    Live(&'a TranscriptProviderReader),
    Candidate {
        storage: &'a SyndicStorage,
        access: &'a HomeCandidateRecoveryAccess<'a>,
        cancellation: &'a beryl_home_store::CommandCancellation,
    },
}

macro_rules! point_read {
    ($name:ident, $candidate:ident, $key:ty, $value:ty) => {
        pub(super) fn $name(
            &self,
            key: $key,
            limit: SyndicPointReadLimit,
        ) -> Result<Option<$value>, SyndicReadError> {
            match self {
                Self::Live(reader) => reader.syndic.$name(&reader.home, key, limit),
                Self::Candidate {
                    storage, access, ..
                } => storage.$candidate(access, key, limit),
            }
        }
    };
}

impl ActivationReadSource<'_> {
    point_read!(
        thread,
        thread_candidate,
        beryl_model::SyndicThreadId,
        ThreadRecord
    );
    point_read!(
        transcript_view_head,
        transcript_view_head_candidate,
        beryl_model::SyndicThreadId,
        TranscriptViewHeadRecord
    );
    point_read!(
        history_summary,
        history_summary_candidate,
        beryl_model::SyndicThreadId,
        HistorySummaryRecord
    );
    point_read!(
        projection,
        projection_candidate,
        beryl_model::SyndicProjectionId,
        syndic_storage::ProjectionRecord
    );
    point_read!(
        canonical_item,
        canonical_item_candidate,
        beryl_model::SyndicItemId,
        CanonicalItemRecord
    );
    point_read!(
        resource,
        resource_candidate,
        beryl_model::SyndicResourceId,
        ResourceMetadataRecord
    );

    pub(super) fn transcript_entries(
        &self,
        thread: beryl_model::SyndicThreadId,
        generation: TranscriptGeneration,
        after: Option<TranscriptPosition>,
        limits: CursorReadLimits,
    ) -> Result<SyndicPage<TranscriptViewEntryRecord>, SyndicReadError> {
        match self {
            Self::Live(reader) => {
                reader
                    .syndic
                    .transcript_entries(&reader.home, thread, generation, after, limits)
            }
            Self::Candidate {
                storage, access, ..
            } => storage.transcript_entries_candidate(access, thread, generation, after, limits),
        }
    }

    pub(super) fn check_request(
        &self,
        request: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<(), TranscriptAttachmentError> {
        match self {
            Self::Live(reader) => reader.check_request(request, cancelled),
            Self::Candidate {
                access,
                cancellation,
                ..
            } => {
                if cancellation.is_cancelled()
                    || cancelled.load(std::sync::atomic::Ordering::Acquire)
                {
                    return Err(TranscriptAttachmentError::Cancelled);
                }
                access
                    .home_revision()
                    .map_err(|_| TranscriptAttachmentError::Unavailable)?;
                if cancellation.is_cancelled()
                    || cancelled.load(std::sync::atomic::Ordering::Acquire)
                {
                    return Err(TranscriptAttachmentError::Cancelled);
                }
                Ok(())
            }
        }
    }
}

impl TranscriptProviderReader {
    pub(in crate::transcript_provider) fn read_activation(
        &self,
        request: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<
        (
            TranscriptAttachmentAuthority,
            Range<u64>,
            PreparedTranscriptActivation,
        ),
        TranscriptAttachmentError,
    > {
        ActivationReadSource::Live(self).read_activation(request, cancelled)
    }
}

pub(crate) fn prepare_candidate_activation(
    access: &HomeCandidateRecoveryAccess<'_>,
    storage: &SyndicStorage,
    window: beryl_model::WindowId,
    thread: beryl_model::SyndicThreadId,
    cancellation: &beryl_home_store::CommandCancellation,
) -> Result<PreparedTranscriptActivation, String> {
    if cancellation.is_cancelled() {
        return Err("ordinary recovery transcript was cancelled".into());
    }
    let before = access.home_revision().map_err(|error| error.to_string())?;
    let request = TranscriptAttachmentRequest {
        window_id: window,
        host: std::sync::Weak::new(),
        activation: 0,
        thread_id: thread,
        request_id: 0,
        placement: TranscriptActivationPlacement::Tail,
        purpose: TranscriptAttachmentPurpose::Attach,
    };
    let cancelled = AtomicBool::new(false);
    let (_, _, prepared) = ActivationReadSource::Candidate {
        storage,
        access,
        cancellation,
    }
    .read_activation(&request, &cancelled)
    .map_err(|error| error.to_string())?;
    if cancellation.is_cancelled() {
        return Err("ordinary recovery transcript was cancelled".into());
    }
    if access.home_revision().map_err(|error| error.to_string())? != before {
        return Err("ordinary recovery transcript changed during preparation".into());
    }
    if cancellation.is_cancelled() {
        return Err("ordinary recovery transcript was cancelled".into());
    }
    Ok(prepared)
}
