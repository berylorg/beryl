use super::{
    provider::{
        TranscriptPageAnchor, TranscriptPageDirection, TranscriptProviderRequest,
        TranscriptProviderResponseKind, TranscriptViewId, TranscriptViewPosition,
    },
    snapshot::ResidentTranscriptSnapshotState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptActivationSeed {
    pub(crate) view_id: Option<TranscriptViewId>,
    pub(crate) source: TranscriptActivationSource,
    pub(crate) placement: TranscriptActivationPlacement,
}

impl TranscriptActivationSeed {
    pub(crate) fn new(
        view_id: TranscriptViewId,
        source: TranscriptActivationSource,
        placement: TranscriptActivationPlacement,
    ) -> Self {
        Self {
            view_id: Some(view_id),
            source,
            placement,
        }
    }

    pub(crate) fn unavailable(
        source: TranscriptActivationSource,
        placement: TranscriptActivationPlacement,
    ) -> Self {
        Self {
            view_id: None,
            source,
            placement,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptActivationSource {
    BackendReopen,
    StartupRestore,
    NewThread,
    Test,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptActivationPlacement {
    Tail,
    Start,
    Position(TranscriptViewPosition),
}

impl TranscriptActivationPlacement {
    pub(crate) fn provider_page_shape(self) -> (TranscriptPageAnchor, TranscriptPageDirection) {
        match self {
            Self::Tail => (TranscriptPageAnchor::End, TranscriptPageDirection::Backward),
            Self::Start => (
                TranscriptPageAnchor::Start,
                TranscriptPageDirection::Forward,
            ),
            Self::Position(position) => (
                TranscriptPageAnchor::Position(position),
                TranscriptPageDirection::Forward,
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptActivationOutcome {
    pub(crate) activation_revision: u64,
    pub(crate) presentation_revision: u64,
    pub(crate) state: ResidentTranscriptSnapshotState,
    pub(crate) retained_previous_snapshot: bool,
    pub(crate) provider_request: Option<TranscriptProviderRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedTranscriptActivation {
    pub(crate) view_id: Option<TranscriptViewId>,
    pub(crate) placement: TranscriptActivationPlacement,
    pub(crate) view_page_response: Option<TranscriptProviderResponseKind>,
    pub(crate) projection_records_response: Option<TranscriptProviderResponseKind>,
}

impl PreparedTranscriptActivation {
    pub(crate) fn new(
        view_id: TranscriptViewId,
        placement: TranscriptActivationPlacement,
        view_page_response: TranscriptProviderResponseKind,
        projection_records_response: Option<TranscriptProviderResponseKind>,
    ) -> Self {
        Self {
            view_id: Some(view_id),
            placement,
            view_page_response: Some(view_page_response),
            projection_records_response,
        }
    }

    pub(crate) fn unavailable(placement: TranscriptActivationPlacement) -> Self {
        Self {
            view_id: None,
            placement,
            view_page_response: None,
            projection_records_response: None,
        }
    }

    pub(crate) fn seed(&self, source: TranscriptActivationSource) -> TranscriptActivationSeed {
        match self.view_id.clone() {
            Some(view_id) => TranscriptActivationSeed::new(view_id, source, self.placement),
            None => TranscriptActivationSeed::unavailable(source, self.placement),
        }
    }
}
