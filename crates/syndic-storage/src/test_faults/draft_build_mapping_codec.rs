use crate::codec::Family;
use crate::draft_piece::build_mapping::model::{Descriptor, MapRoot, Measure};
use crate::draft_piece::*;
mod receipts;
pub use receipts::draft_build_mapping_receipt_cases_for_test;

pub fn draft_build_mapping_versions_for_test() -> [u32; 3] {
    [
        DraftPieceBuildsFamily::RECORD_VERSION.get(),
        DraftPieceBuildProgressFamily::RECORD_VERSION.get(),
        DraftPieceSettlementsFamily::RECORD_VERSION.get(),
    ]
}

pub fn draft_build_mapping_reencode_for_test(bytes: &[u8]) -> Option<Vec<u8>> {
    mapping_roundtrip_for_test(bytes)
}

pub fn draft_build_mapping_stage_encodings_for_test() -> Vec<Vec<u8>> {
    use DraftPieceMappingStageV1 as Stage;
    let boundary = DraftPieceBuildBoundaryV1::new(2, 3);
    let start = DraftPieceMappingSourceFactV1 { boundary, unit: 4 };
    let end = DraftPieceMappingSourceFactV1 {
        boundary: DraftPieceBuildBoundaryV1::new(5, 6),
        unit: 7,
    };
    let proof = DraftPieceMappingProofComponentV1 {
        component: DraftPieceMarkerProofComponentV1::Secondary,
        primary_marker_rank: Some(8),
    };
    let splice = DraftPieceMappingSpliceV1 {
        kind: DraftPieceMappingSpliceKindV1::TextDelete,
        a: 4,
        removed: 2,
        inserted: 0,
        leaf: Some((
            DraftPieceRecordIdV1::from_bytes([9; 16]),
            DraftPieceDigestV1::from_bytes([10; 32]),
        )),
        rank: 2,
        local_start: 3,
        local_end: 5,
    };
    let stages = [
        Stage::Idle,
        Stage::TextSourceStart { proof },
        Stage::TextSourceEnd { start, proof },
        Stage::TextPreviousStart { start, end, proof },
        Stage::TextPreviousEnd {
            start,
            end,
            previous_start: boundary,
            proof,
        },
        Stage::TextMapStart { start, end },
        Stage::TextMapEnd {
            source_start: boundary,
            source_end: end.boundary,
            mapped_start: 4,
        },
        Stage::TextResolveStart {
            source_start: boundary,
            source_end: end.boundary,
            mapped_start: 4,
            mapped_end: 7,
        },
        Stage::TextResolveEnd {
            source_start: boundary,
            source_end: end.boundary,
            mapped_end: 7,
            start_boundary: boundary,
            start_marker_ordinal: 8,
        },
        Stage::MarkerSource {
            removal_source_unit: Some(4),
        },
        Stage::MarkerMapRemoval { source_unit: 4 },
        Stage::MarkerResolveRemoval { mapped_unit: 4 },
        Stage::MarkerWorkingIdentity { mapped_unit: 4 },
        Stage::MarkerMapBoundary,
        Stage::MarkerResolveBoundary { mapped_unit: 4 },
        Stage::MarkerPlanningReady { boundary },
        Stage::TextDeleteProof,
        Stage::TextInsertProof,
        Stage::DeleteMap {
            splice,
            target: MapRoot::Identity(7),
            remaining_end: 6,
        },
        Stage::InsertMap { splice },
        Stage::MapComplete {
            splice,
            target: MapRoot::Identity(7),
        },
        Stage::Ready(DraftPieceMappingReadySpliceV1 {
            kind: splice.kind,
            a: 4,
            removed: 2,
            inserted: 0,
            target: MapRoot::Identity(7),
        }),
        Stage::RefreshMap,
        Stage::RefreshSequence { mapped_unit: 4 },
        Stage::PublishReady {
            successor_boundary: boundary,
            logical_offset: 3,
        },
    ];
    stages
        .into_iter()
        .map(|mapping_stage| {
            canonical_build_mapping_bytes(Some(DraftPieceBuildMappingV1 {
                current_map: MapRoot::Identity(9),
                completed_source_unit: 1,
                fragment_source_end_unit: Some(7),
                mapping_stage,
            }))
        })
        .collect()
}

pub fn draft_build_mapping_maximum_for_test() -> Vec<u8> {
    let current_map = MapRoot::Stored(Descriptor {
        id: [1; 16],
        digest: [2; 32],
        height: 22,
        measure: Measure {
            source: 20,
            target: 20,
        },
    });
    canonical_build_mapping_bytes(Some(DraftPieceBuildMappingV1 {
        current_map,
        completed_source_unit: 0,
        fragment_source_end_unit: Some(20),
        mapping_stage: DraftPieceMappingStageV1::DeleteMap {
            splice: DraftPieceMappingSpliceV1 {
                kind: DraftPieceMappingSpliceKindV1::TextDelete,
                a: 1,
                removed: 2,
                inserted: 0,
                leaf: Some((
                    DraftPieceRecordIdV1::from_bytes([3; 16]),
                    DraftPieceDigestV1::from_bytes([4; 32]),
                )),
                rank: 1,
                local_start: 2,
                local_end: 4,
            },
            target: current_map,
            remaining_end: 3,
        },
    }))
}

pub fn draft_build_mapless_edit_rejected_for_test(build: &DraftPieceBuildRecordV1) -> bool {
    DraftPieceBuildsFamily::encode_value(&build.clone().with_mapping(None)).is_err()
}
