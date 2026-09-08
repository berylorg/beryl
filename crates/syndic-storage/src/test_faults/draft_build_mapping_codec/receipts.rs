use super::*;
use DraftPieceMappingStageV1 as Stage;
use beryl_model::SyndicDraftId;

fn reference(ordinal: u64) -> DraftPieceBuildProgressReceiptReferenceV1 {
    DraftPieceBuildProgressReceiptReferenceV1::new(
        DraftPieceBuildProgressReceiptKeyV1::new(
            SyndicDraftId::from_bytes([1; 16]),
            DraftEditorCandidateSessionIdV1::from_bytes([2; 16]),
            DraftPieceOperationIdV1::from_bytes([3; 16]),
            ordinal,
        ),
        DraftPieceDigestV1::from_bytes([4; 32]),
    )
}

fn fragment() -> DraftPieceBuildFragmentV1 {
    let key = reference(1).key();
    let replacement = DraftPieceReplacementV1::new(
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::Unambiguous),
        DraftCompositePositionV1::new(10, DraftCompositeGapWitnessV1::Unambiguous),
        Vec::new(),
    );
    let preceding = canonical_empty_draft_piece_fragment_chain_v1();
    let chain = draft_piece_fragment_chain_link_v1(preceding, 1, &replacement);
    DraftPieceBuildFragmentV1::new(
        DraftPieceBuildFragmentKeyV1::new(key.draft_id(), key.session_id(), key.operation_id(), 1),
        replacement,
        preceding,
        chain,
    )
}

fn roots() -> DraftPieceBuildRootsV1 {
    DraftPieceBuildRootsV1::new(
        Some(DraftPieceRecordIdV1::from_bytes([5; 16])),
        DraftPieceSummaryV1::new(
            10,
            0,
            1,
            1,
            0,
            canonical_empty_marker_digest_v1(),
            1,
            DraftPieceDigestV1::from_bytes([6; 32]),
        ),
        None,
        DraftMarkerIdentityIndexSummaryV1::new(
            0,
            0,
            canonical_empty_marker_identity_index_digest_v1(),
        ),
        None,
        0,
        canonical_empty_draft_marker_commitment_v1(),
    )
}

fn receipt(
    mapping: DraftPieceBuildMappingV1,
    frontier: DraftPieceBuildFrontierV1,
    previous: DraftPieceBuildProgressReceiptReferenceV1,
    ordinal: u64,
    lifecycle: DraftPieceBuildLifecycleV1,
) -> DraftPieceBuildProgressReceiptV1 {
    recompute_progress_receipt_digest(
        DraftPieceBuildProgressReceiptV1::new(
            reference(ordinal),
            Some(previous),
            Some(canonical_fragment_endpoint(&fragment())),
            roots(),
            DraftPieceBuildBoundaryV1::new(0, 0),
            DraftPieceBuildBoundaryV1::new(0, 0),
            1,
            frontier,
            None,
            None,
            lifecycle,
        )
        .with_mapping(Some(mapping)),
    )
}

pub fn draft_build_mapping_receipt_cases_for_test() -> Vec<(&'static str, bool, bool)> {
    let splice = DraftPieceMappingSpliceV1 {
        kind: DraftPieceMappingSpliceKindV1::TextDelete,
        a: 0,
        removed: 10,
        inserted: 0,
        leaf: Some((
            DraftPieceRecordIdV1::from_bytes([7; 16]),
            DraftPieceDigestV1::from_bytes([8; 32]),
        )),
        rank: 0,
        local_start: 0,
        local_end: 10,
    };
    let frontier = DraftPieceBuildFrontierV1::Applying {
        fragment_ordinal: 1,
        base_end: DraftPieceBuildBoundaryV1::new(1, 0),
        successor_start: DraftPieceBuildBoundaryV1::new(0, 0),
        successor_end: DraftPieceBuildBoundaryV1::new(1, 0),
    };
    let initial = DraftPieceBuildMappingV1 {
        current_map: MapRoot::Identity(10),
        completed_source_unit: 0,
        fragment_source_end_unit: Some(10),
        mapping_stage: Stage::DeleteMap {
            splice,
            target: MapRoot::Identity(10),
            remaining_end: 10,
        },
    };
    let selected = receipt(
        initial,
        frontier,
        reference(1),
        2,
        DraftPieceBuildLifecycleV1::Open,
    );
    let mut cases = vec![(
        "canonical deletion endpoint",
        true,
        progress_receipt_is_exact(&selected),
    )];
    let mut check = |name, mapping| {
        let value = receipt(
            mapping,
            frontier,
            reference(1),
            2,
            DraftPieceBuildLifecycleV1::Open,
        );
        cases.push((name, false, progress_receipt_is_exact(&value)));
    };
    check(
        "map source or target substitution",
        DraftPieceBuildMappingV1 {
            current_map: MapRoot::Identity(11),
            ..initial
        },
    );
    check(
        "completed source past original end",
        DraftPieceBuildMappingV1 {
            completed_source_unit: 11,
            ..initial
        },
    );
    check(
        "splice loses original end",
        DraftPieceBuildMappingV1 {
            fragment_source_end_unit: None,
            ..initial
        },
    );
    check(
        "text query in applying phase",
        DraftPieceBuildMappingV1 {
            mapping_stage: Stage::TextSourceStart {
                proof: DraftPieceMappingProofComponentV1 {
                    component: DraftPieceMarkerProofComponentV1::Primary,
                    primary_marker_rank: None,
                },
            },
            ..initial
        },
    );
    check(
        "deletion remaining end grows",
        DraftPieceBuildMappingV1 {
            mapping_stage: Stage::DeleteMap {
                splice,
                target: MapRoot::Identity(10),
                remaining_end: 11,
            },
            ..initial
        },
    );
    check(
        "deletion target summary differs from remaining end",
        DraftPieceBuildMappingV1 {
            mapping_stage: Stage::DeleteMap {
                splice,
                target: MapRoot::Identity(9),
                remaining_end: 10,
            },
            ..initial
        },
    );
    let deleted = MapRoot::Stored(Descriptor {
        id: [9; 16],
        digest: [10; 32],
        height: 1,
        measure: Measure {
            source: 10,
            target: 0,
        },
    });
    let complete = DraftPieceBuildMappingV1 {
        mapping_stage: Stage::MapComplete {
            splice,
            target: deleted,
        },
        ..initial
    };
    let before = receipt(
        complete,
        frontier,
        reference(2),
        3,
        DraftPieceBuildLifecycleV1::Open,
    );
    let ready = DraftPieceBuildMappingV1 {
        mapping_stage: Stage::Ready(DraftPieceMappingReadySpliceV1 {
            kind: splice.kind,
            a: 0,
            removed: 10,
            inserted: 0,
            target: deleted,
        }),
        ..initial
    };
    let after = receipt(
        ready,
        frontier,
        before.reference(),
        4,
        DraftPieceBuildLifecycleV1::Open,
    );
    cases.push((
        "completed map becomes compact ready in one control",
        true,
        marker_effect_progress_transition_is_exact(&before, &after, Some(&fragment())),
    ));
    let skipped = receipt(
        DraftPieceBuildMappingV1 {
            mapping_stage: Stage::Idle,
            ..initial
        },
        frontier,
        before.reference(),
        4,
        DraftPieceBuildLifecycleV1::Open,
    );
    cases.push((
        "completed map cannot skip ready",
        false,
        marker_effect_progress_transition_is_exact(&before, &skipped, Some(&fragment())),
    ));
    let terminal = receipt(
        initial,
        frontier,
        selected.reference(),
        3,
        DraftPieceBuildLifecycleV1::Cancelled,
    );
    cases.push((
        "cancellation preserves pending map",
        true,
        marker_effect_progress_transition_is_exact(&selected, &terminal, Some(&fragment())),
    ));
    let changed_terminal = receipt(
        complete,
        frontier,
        selected.reference(),
        3,
        DraftPieceBuildLifecycleV1::Cancelled,
    );
    cases.push((
        "cancellation cannot advance pending map",
        false,
        marker_effect_progress_transition_is_exact(&selected, &changed_terminal, Some(&fragment())),
    ));
    let frozen = DraftPieceBuildFrontierV1::Inserting {
        fragment_ordinal: 1,
        next_piece: 0,
        next_byte: 0,
        base_end: DraftPieceBuildBoundaryV1::new(1, 0),
        successor_end: DraftPieceBuildBoundaryV1::new(99, 11),
    };
    let refresh = receipt(
        DraftPieceBuildMappingV1 {
            mapping_stage: Stage::RefreshMap,
            ..initial
        },
        frozen,
        reference(3),
        4,
        DraftPieceBuildLifecycleV1::Open,
    );
    let resolved = receipt(
        DraftPieceBuildMappingV1 {
            mapping_stage: Stage::RefreshSequence { mapped_unit: 10 },
            ..initial
        },
        frozen,
        refresh.reference(),
        5,
        DraftPieceBuildLifecycleV1::Open,
    );
    cases.push((
        "refresh preserves exhausted historical cursor",
        true,
        marker_effect_progress_transition_is_exact(&refresh, &resolved, Some(&fragment())),
    ));
    let normalized = receipt(
        DraftPieceBuildMappingV1 {
            mapping_stage: Stage::RefreshSequence { mapped_unit: 10 },
            ..initial
        },
        DraftPieceBuildFrontierV1::Inserting {
            fragment_ordinal: 1,
            next_piece: 0,
            next_byte: 0,
            base_end: DraftPieceBuildBoundaryV1::new(1, 0),
            successor_end: DraftPieceBuildBoundaryV1::new(1, 0),
        },
        refresh.reference(),
        5,
        DraftPieceBuildLifecycleV1::Open,
    );
    cases.push((
        "refresh cannot reinterpret frozen cursor",
        false,
        marker_effect_progress_transition_is_exact(&refresh, &normalized, Some(&fragment())),
    ));
    cases
}
