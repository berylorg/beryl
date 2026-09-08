use super::super::{
    DraftPieceBuildBoundaryV1, DraftPieceDigestV1, DraftPieceRecordIdV1,
    build_mapping::model::MapRoot,
};
use super::DraftPieceMarkerProofComponentV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceBuildMappingV1 {
    pub current_map: MapRoot,
    pub completed_source_unit: u128,
    pub fragment_source_end_unit: Option<u128>,
    pub mapping_stage: DraftPieceMappingStageV1,
}

impl DraftPieceBuildMappingV1 {
    pub(crate) fn initial(summary: super::super::DraftPieceSummaryV1) -> Self {
        let units = u128::from(summary.logical_utf8_bytes()) + u128::from(summary.marker_count());
        Self {
            current_map: if units == 0 {
                MapRoot::Empty
            } else {
                MapRoot::Identity(units)
            },
            completed_source_unit: 0,
            fragment_source_end_unit: None,
            mapping_stage: DraftPieceMappingStageV1::Idle,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceMappingSourceFactV1 {
    pub boundary: DraftPieceBuildBoundaryV1,
    pub unit: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceMappingProofComponentV1 {
    pub component: DraftPieceMarkerProofComponentV1,
    pub primary_marker_rank: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum DraftPieceMappingSpliceKindV1 {
    TextDelete = 0,
    TextInsert = 1,
    MarkerDelete = 2,
    MarkerInsert = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceMappingSpliceV1 {
    pub kind: DraftPieceMappingSpliceKindV1,
    pub a: u128,
    pub removed: u128,
    pub inserted: u128,
    pub leaf: Option<(DraftPieceRecordIdV1, DraftPieceDigestV1)>,
    pub rank: u64,
    pub local_start: u64,
    pub local_end: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftPieceMappingReadySpliceV1 {
    pub kind: DraftPieceMappingSpliceKindV1,
    pub a: u128,
    pub removed: u128,
    pub inserted: u128,
    pub target: MapRoot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DraftPieceMappingStageV1 {
    Idle,
    TextSourceStart {
        proof: DraftPieceMappingProofComponentV1,
    },
    TextSourceEnd {
        start: DraftPieceMappingSourceFactV1,
        proof: DraftPieceMappingProofComponentV1,
    },
    TextPreviousStart {
        start: DraftPieceMappingSourceFactV1,
        end: DraftPieceMappingSourceFactV1,
        proof: DraftPieceMappingProofComponentV1,
    },
    TextPreviousEnd {
        start: DraftPieceMappingSourceFactV1,
        end: DraftPieceMappingSourceFactV1,
        previous_start: DraftPieceBuildBoundaryV1,
        proof: DraftPieceMappingProofComponentV1,
    },
    TextMapStart {
        start: DraftPieceMappingSourceFactV1,
        end: DraftPieceMappingSourceFactV1,
    },
    TextMapEnd {
        source_start: DraftPieceBuildBoundaryV1,
        source_end: DraftPieceBuildBoundaryV1,
        mapped_start: u128,
    },
    TextResolveStart {
        source_start: DraftPieceBuildBoundaryV1,
        source_end: DraftPieceBuildBoundaryV1,
        mapped_start: u128,
        mapped_end: u128,
    },
    TextResolveEnd {
        source_start: DraftPieceBuildBoundaryV1,
        source_end: DraftPieceBuildBoundaryV1,
        mapped_end: u128,
        start_boundary: DraftPieceBuildBoundaryV1,
        start_marker_ordinal: u64,
    },
    MarkerSource {
        removal_source_unit: Option<u128>,
    },
    MarkerMapRemoval {
        source_unit: u128,
    },
    MarkerResolveRemoval {
        mapped_unit: u128,
    },
    MarkerWorkingIdentity {
        mapped_unit: u128,
    },
    MarkerMapBoundary,
    MarkerResolveBoundary {
        mapped_unit: u128,
    },
    MarkerPlanningReady {
        boundary: DraftPieceBuildBoundaryV1,
    },
    TextDeleteProof,
    TextInsertProof,
    DeleteMap {
        splice: DraftPieceMappingSpliceV1,
        target: MapRoot,
        remaining_end: u128,
    },
    InsertMap {
        splice: DraftPieceMappingSpliceV1,
    },
    MapComplete {
        splice: DraftPieceMappingSpliceV1,
        target: MapRoot,
    },
    Ready(DraftPieceMappingReadySpliceV1),
    RefreshMap,
    RefreshSequence {
        mapped_unit: u128,
    },
    PublishReady {
        successor_boundary: DraftPieceBuildBoundaryV1,
        logical_offset: u64,
    },
}
