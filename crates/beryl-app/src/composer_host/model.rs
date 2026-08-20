use std::num::NonZeroU64;

use beryl_home_store::HomeGeneration;
use beryl_model::{BerylHomeId, SyndicThreadId};
use syndic_storage::{
    DraftCompositePositionV1, DraftEditHistoryFrontierReferenceV1,
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionCollisionProofV1,
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionV1, DraftEditorCurrentSelectorV1,
    DraftLogicalExtentV1, DraftPieceCandidateRangeResultV1, DraftPieceCurrentRangeResultV1,
    DraftPieceMarkerDemandResultV1, DraftPieceMarkerDemandV1, DraftPieceMarkerEdgeProofRequestV1,
    DraftPieceMarkerEdgeProofV1, DraftPieceOperationIdV1, DraftPieceRestorationV1,
    DraftPieceRootReferenceV1, DraftPieceTextDemandResultV1, DraftPieceTextDemandV1,
};

pub const COMPOSER_HOST_MAX_INITIAL_DEMANDS: usize = 16;
pub const COMPOSER_HOST_MAX_PENDING_REQUESTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ComposerHostGeneration(NonZeroU64);

impl ComposerHostGeneration {
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub(crate) fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ComposerHostRequestId(NonZeroU64);

impl ComposerHostRequestId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ComposerHostRequestPurpose {
    Viewport,
    Caret,
    Selection,
    Segmentation,
    Clipboard,
    Restoration,
    Geometry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostBinding {
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    host_generation: ComposerHostGeneration,
    candidate: DraftEditorCandidateActivationBindingV1,
    presentation_generation: NonZeroU64,
}

impl ComposerHostBinding {
    pub(crate) const fn new(
        home_id: BerylHomeId,
        home_generation: HomeGeneration,
        host_generation: ComposerHostGeneration,
        candidate: DraftEditorCandidateActivationBindingV1,
        presentation_generation: NonZeroU64,
    ) -> Self {
        Self {
            home_id,
            home_generation,
            host_generation,
            candidate,
            presentation_generation,
        }
    }

    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }

    pub const fn host_generation(self) -> ComposerHostGeneration {
        self.host_generation
    }

    pub const fn candidate(self) -> DraftEditorCandidateActivationBindingV1 {
        self.candidate
    }

    pub const fn presentation_generation(self) -> NonZeroU64 {
        self.presentation_generation
    }

    pub const fn root(self) -> DraftPieceRootReferenceV1 {
        self.candidate.root()
    }

    pub const fn history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.candidate.history()
    }

    pub const fn logical_extent(self) -> DraftLogicalExtentV1 {
        self.candidate.logical_extent()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostRequestKey {
    binding: ComposerHostBinding,
    request_id: ComposerHostRequestId,
    purpose: ComposerHostRequestPurpose,
}

impl ComposerHostRequestKey {
    pub const fn new(
        binding: ComposerHostBinding,
        request_id: ComposerHostRequestId,
        purpose: ComposerHostRequestPurpose,
    ) -> Self {
        Self {
            binding,
            request_id,
            purpose,
        }
    }

    pub const fn binding(self) -> ComposerHostBinding {
        self.binding
    }

    pub const fn request_id(self) -> ComposerHostRequestId {
        self.request_id
    }

    pub const fn purpose(self) -> ComposerHostRequestPurpose {
        self.purpose
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostReadTarget {
    Historical(DraftPieceRootReferenceV1),
    Current(SyndicThreadId),
    Candidate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerHostRequestKind {
    Text {
        target: ComposerHostReadTarget,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    },
    Markers {
        target: ComposerHostReadTarget,
        demand: DraftPieceMarkerDemandV1,
    },
    MarkerProof {
        target: ComposerHostReadTarget,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    },
    Restoration {
        target: ComposerHostReadTarget,
        seed: ComposerHostRestorationSeed,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposerHostPendingRequest {
    key: ComposerHostRequestKey,
    kind: ComposerHostRequestKind,
}

impl ComposerHostPendingRequest {
    pub(crate) const fn new(key: ComposerHostRequestKey, kind: ComposerHostRequestKind) -> Self {
        Self { key, kind }
    }

    pub const fn key(&self) -> ComposerHostRequestKey {
        self.key
    }

    pub const fn kind(&self) -> &ComposerHostRequestKind {
        &self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposerHostRestorationSeed {
    root: DraftPieceRootReferenceV1,
    history: DraftEditHistoryFrontierReferenceV1,
    logical_extent: DraftLogicalExtentV1,
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
    scroll: DraftCompositePositionV1,
}

impl ComposerHostRestorationSeed {
    pub const fn new(
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
        logical_extent: DraftLogicalExtentV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
        scroll: DraftCompositePositionV1,
    ) -> Self {
        Self {
            root,
            history,
            logical_extent,
            caret,
            selection,
            scroll,
        }
    }

    pub const fn root(&self) -> DraftPieceRootReferenceV1 {
        self.root
    }

    pub const fn history(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.history
    }

    pub const fn logical_extent(&self) -> DraftLogicalExtentV1 {
        self.logical_extent
    }

    pub const fn restoration(&self) -> DraftPieceRestorationV1 {
        DraftPieceRestorationV1::new(
            self.root,
            self.history,
            self.caret,
            self.selection,
            self.scroll,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerHostInitialDemand {
    Text {
        request_id: ComposerHostRequestId,
        purpose: ComposerHostRequestPurpose,
        demand: DraftPieceTextDemandV1,
        max_bytes: usize,
    },
    Markers {
        request_id: ComposerHostRequestId,
        purpose: ComposerHostRequestPurpose,
        demand: DraftPieceMarkerDemandV1,
    },
    MarkerProof {
        request_id: ComposerHostRequestId,
        purpose: ComposerHostRequestPurpose,
        request: DraftPieceMarkerEdgeProofRequestV1,
        retained_byte_ceiling: usize,
    },
}

impl ComposerHostInitialDemand {
    pub const fn request_id(&self) -> ComposerHostRequestId {
        match self {
            Self::Text { request_id, .. }
            | Self::Markers { request_id, .. }
            | Self::MarkerProof { request_id, .. } => *request_id,
        }
    }

    pub const fn purpose(&self) -> ComposerHostRequestPurpose {
        match self {
            Self::Text { purpose, .. }
            | Self::Markers { purpose, .. }
            | Self::MarkerProof { purpose, .. } => *purpose,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ComposerHostActivationRequest {
    thread_id: SyndicThreadId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    presentation_generation: NonZeroU64,
    restoration: Option<ComposerHostRestorationSeed>,
    first_demands: Box<[ComposerHostInitialDemand]>,
}

impl ComposerHostActivationRequest {
    pub fn new(
        thread_id: SyndicThreadId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
        presentation_generation: NonZeroU64,
        restoration: Option<ComposerHostRestorationSeed>,
        first_demands: Box<[ComposerHostInitialDemand]>,
    ) -> Self {
        Self {
            thread_id,
            session_id,
            operation_id,
            presentation_generation,
            restoration,
            first_demands,
        }
    }

    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }

    pub const fn session_id(&self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }

    pub const fn operation_id(&self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }

    pub const fn presentation_generation(&self) -> NonZeroU64 {
        self.presentation_generation
    }

    pub const fn restoration(&self) -> Option<&ComposerHostRestorationSeed> {
        self.restoration.as_ref()
    }

    pub fn first_demands(&self) -> &[ComposerHostInitialDemand] {
        &self.first_demands
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostOpenDisposition {
    Opened,
    ExactReplay,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerHostActivationOutcome {
    Activated {
        disposition: ComposerHostOpenDisposition,
        binding: ComposerHostBinding,
    },
    Cancelled,
    StaleDisposed(DraftEditorCandidateSessionV1),
    SelectorConflict(DraftEditorCurrentSelectorV1),
    OccupiedIdentityCollision(DraftEditorCandidateSessionCollisionProofV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerHostResponseValue {
    HistoricalText(DraftPieceTextDemandResultV1),
    CurrentText(Option<DraftPieceCurrentRangeResultV1<DraftPieceTextDemandResultV1>>),
    CandidateText(DraftPieceCandidateRangeResultV1<DraftPieceTextDemandResultV1>),
    HistoricalMarkers(DraftPieceMarkerDemandResultV1),
    CurrentMarkers(Option<DraftPieceCurrentRangeResultV1<DraftPieceMarkerDemandResultV1>>),
    CandidateMarkers(DraftPieceCandidateRangeResultV1<DraftPieceMarkerDemandResultV1>),
    HistoricalMarkerProof(Option<DraftPieceMarkerEdgeProofV1>),
    CurrentMarkerProof(Option<DraftPieceCurrentRangeResultV1<Option<DraftPieceMarkerEdgeProofV1>>>),
    CandidateMarkerProof(DraftPieceCandidateRangeResultV1<Option<DraftPieceMarkerEdgeProofV1>>),
    Restoration(ComposerHostRestorationSeed),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposerHostResponse {
    key: ComposerHostRequestKey,
    value: ComposerHostResponseValue,
}

impl ComposerHostResponse {
    pub(crate) const fn new(key: ComposerHostRequestKey, value: ComposerHostResponseValue) -> Self {
        Self { key, value }
    }

    pub const fn key(&self) -> ComposerHostRequestKey {
        self.key
    }

    pub const fn value(&self) -> &ComposerHostResponseValue {
        &self.value
    }
}

pub struct ComposerHostExecution {
    pub(crate) pending: ComposerHostPendingRequest,
    pub(crate) result: Result<ComposerHostResponseValue, super::ComposerHostError>,
}
