use super::*;

fn text_frame(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceBuildMappingV1,
    right: DraftPieceBuildMappingV1,
) -> bool {
    control_frozen(previous, current)
        && left.current_map == right.current_map
        && previous.marker_effect_continuation().active().is_none()
}

fn marker_frame(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceBuildMappingV1,
    right: DraftPieceBuildMappingV1,
) -> bool {
    let (Some(a), Some(b)) = (
        previous.marker_effect_continuation().active(),
        current.marker_effect_continuation().active(),
    ) else {
        return false;
    };
    left.current_map == right.current_map
        && previous.working_roots() == current.working_roots()
        && previous.frontier() == current.frontier()
        && previous.base_frontier() == current.base_frontier()
        && previous.successor_frontier() == current.successor_frontier()
        && previous.next_record_ordinal() == current.next_record_ordinal()
        && a.fragment_key() == b.fragment_key()
        && a.fragment_digest() == b.fragment_digest()
        && a.effect() == b.effect()
        && a.source_roots() == b.source_roots()
        && a.working_roots() == b.working_roots()
        && a.source_frontier() == b.source_frontier()
        && a.successor_frontier() == b.successor_frontier()
        && previous.marker_effect_continuation().scan()
            == current.marker_effect_continuation().scan()
}

fn complex(position: DraftCompositePositionV1) -> bool {
    matches!(
        position.gap(),
        DraftCompositeGapWitnessV1::AfterAll | DraftCompositeGapWitnessV1::Between { .. }
    )
}

fn completed(proof: DraftPieceMappingProofComponentV1, position: DraftCompositePositionV1) -> bool {
    proof_valid(proof)
        && (proof.component == DraftPieceMarkerProofComponentV1::Secondary) == complex(position)
}

fn continued(
    left: DraftPieceMappingProofComponentV1,
    right: DraftPieceMappingProofComponentV1,
) -> bool {
    left.component == DraftPieceMarkerProofComponentV1::Primary
        && left.primary_marker_rank.is_none()
        && right.component == DraftPieceMarkerProofComponentV1::Secondary
        && right.primary_marker_rank.is_some()
}

pub(super) fn transition(
    previous: &DraftPieceBuildProgressReceiptV1,
    current: &DraftPieceBuildProgressReceiptV1,
    left: DraftPieceBuildMappingV1,
    right: DraftPieceBuildMappingV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> bool {
    let Some(fragment) = fragment else {
        return false;
    };
    let replacement = fragment.replacement();
    let same_end = left.fragment_source_end_unit == right.fragment_source_end_unit;
    let text = text_frame(previous, current, left, right);
    match (left.mapping_stage, right.mapping_stage) {
        (Stage::Idle, Stage::TextSourceStart { proof }) => {
            text && same_end
                && !replacement.is_continuation()
                && replacement.marker_effect().is_none()
                && proof
                    == DraftPieceMappingProofComponentV1 {
                        component: DraftPieceMarkerProofComponentV1::Primary,
                        primary_marker_rank: None,
                    }
        }
        (Stage::TextSourceStart { proof: a }, Stage::TextSourceStart { proof: b }) => {
            text && same_end && complex(replacement.start()) && continued(a, b)
        }
        (Stage::TextSourceStart { proof }, Stage::TextSourceEnd { proof: next, .. }) => {
            text && same_end
                && completed(proof, replacement.start())
                && next
                    == DraftPieceMappingProofComponentV1 {
                        component: DraftPieceMarkerProofComponentV1::Primary,
                        primary_marker_rank: None,
                    }
        }
        (
            Stage::TextSourceEnd { start: a, proof: p },
            Stage::TextSourceEnd { start: b, proof: q },
        ) => text && same_end && a == b && complex(replacement.end()) && continued(p, q),
        (
            Stage::TextSourceEnd { start, proof },
            Stage::TextPreviousStart {
                start: next_start,
                end,
                proof: next,
            },
        ) => {
            text && start == next_start
                && completed(proof, replacement.end())
                && right.fragment_source_end_unit == Some(end.unit)
                && start.unit == end.unit
                && start.unit == left.completed_source_unit
                && fragment.key().ordinal() > 1
                && next
                    == DraftPieceMappingProofComponentV1 {
                        component: DraftPieceMarkerProofComponentV1::Primary,
                        primary_marker_rank: None,
                    }
        }
        (
            Stage::TextSourceEnd { start, proof },
            Stage::TextMapStart {
                start: next_start,
                end,
            },
        ) => {
            text && start == next_start
                && completed(proof, replacement.end())
                && right.fragment_source_end_unit == Some(end.unit)
                && !(start.unit == end.unit
                    && start.unit == left.completed_source_unit
                    && fragment.key().ordinal() > 1)
        }
        (
            Stage::TextPreviousStart {
                start: a,
                end: b,
                proof: p,
            },
            Stage::TextPreviousStart {
                start: c,
                end: d,
                proof: q,
            },
        ) => text && same_end && a == c && b == d && continued(p, q),
        (
            Stage::TextPreviousStart {
                start: a, end: b, ..
            },
            Stage::TextPreviousEnd {
                start: c,
                end: d,
                previous_start,
                proof,
            },
        ) => {
            text && same_end
                && a == c
                && b == d
                && ordered(previous_start, a.boundary)
                && proof
                    == DraftPieceMappingProofComponentV1 {
                        component: DraftPieceMarkerProofComponentV1::Primary,
                        primary_marker_rank: None,
                    }
        }
        (
            Stage::TextPreviousEnd {
                start: a,
                end: b,
                previous_start: c,
                proof: p,
            },
            Stage::TextPreviousEnd {
                start: d,
                end: e,
                previous_start: f,
                proof: q,
            },
        ) => text && same_end && a == d && b == e && c == f && continued(p, q),
        (
            Stage::TextPreviousEnd {
                start: a,
                end: b,
                previous_start,
                ..
            },
            Stage::TextMapStart { start: c, end: d },
        ) => {
            text && same_end
                && a == c
                && b == d
                && previous_start != a.boundary
                && ordered(previous_start, a.boundary)
        }
        (
            Stage::TextMapStart { start, end },
            Stage::TextMapEnd {
                source_start,
                source_end,
                ..
            },
        ) => text && same_end && start.boundary == source_start && end.boundary == source_end,
        (
            Stage::TextMapEnd {
                source_start: a,
                source_end: b,
                mapped_start: c,
            },
            Stage::TextResolveStart {
                source_start: d,
                source_end: e,
                mapped_start: f,
                ..
            },
        ) => text && same_end && a == d && b == e && c == f,
        (
            Stage::TextResolveStart {
                source_start: a,
                source_end: b,
                mapped_end: c,
                ..
            },
            Stage::TextResolveEnd {
                source_start: d,
                source_end: e,
                mapped_end: f,
                ..
            },
        ) => text && same_end && a == d && b == e && c == f,
        (
            Stage::TextResolveEnd {
                source_end,
                start_boundary,
                ..
            },
            Stage::Idle,
        ) => {
            previous.marker_effect_continuation() == current.marker_effect_continuation()
                && previous.working_roots() == current.working_roots()
                && previous.base_frontier() == current.base_frontier()
                && previous.successor_frontier() == current.successor_frontier()
                && previous.next_record_ordinal() == current.next_record_ordinal()
                && left.current_map == right.current_map
                && same_end
                && matches!(current.frontier(), Frontier::Removing { fragment_ordinal, next_rank, end_rank, removed_markers: 0, base_end, successor_start, successor_end }
                    if fragment_ordinal == fragment.key().ordinal() && next_rank == source_end.rank() && end_rank == source_end.rank() && base_end == source_end && successor_start == start_boundary && ordered(start_boundary, successor_end))
        }
        (
            Stage::Idle,
            Stage::MarkerSource {
                removal_source_unit: None,
            },
        ) => {
            same_end
                && left.current_map == right.current_map
                && super::super::marker_progress::transition_is_exact(
                    previous,
                    current,
                    Some(fragment),
                )
        }
        (
            Stage::MarkerSource {
                removal_source_unit: a,
            },
            Stage::MarkerSource {
                removal_source_unit: b,
            },
        ) => {
            marker_frame(previous, current, left, right)
                && super::super::marker_progress::transition_is_exact(
                    previous,
                    current,
                    Some(fragment),
                )
                && match previous
                    .marker_effect_continuation()
                    .active()
                    .map(|a| a.pending())
                {
                    Some(Pending::Proof {
                        purpose: DraftPieceMarkerProofPurposeV1::SourceBounds,
                        ..
                    }) => {
                        a == b
                            && (same_end
                                || left.fragment_source_end_unit.is_none()
                                    && right.fragment_source_end_unit.is_some())
                    }
                    Some(Pending::Proof {
                        purpose: DraftPieceMarkerProofPurposeV1::SourceOccurrence,
                        ..
                    }) => a.is_none() && b.is_some() && same_end,
                    _ => a == b && same_end,
                }
        }
        (
            Stage::MarkerSource {
                removal_source_unit,
            },
            Stage::MarkerMapRemoval { source_unit },
        ) => {
            same_end
                && removal_source_unit == Some(source_unit)
                && marker_frame(previous, current, left, right)
                && super::super::marker_progress::transition_is_exact(
                    previous,
                    current,
                    Some(fragment),
                )
        }
        (
            Stage::MarkerSource {
                removal_source_unit: None,
            },
            Stage::MarkerMapBoundary,
        ) => {
            same_end
                && marker_frame(previous, current, left, right)
                && super::super::marker_progress::transition_is_exact(
                    previous,
                    current,
                    Some(fragment),
                )
        }
        (Stage::MarkerMapRemoval { .. }, Stage::MarkerResolveRemoval { .. })
        | (Stage::MarkerMapBoundary, Stage::MarkerResolveBoundary { .. })
        | (Stage::MarkerResolveBoundary { .. }, Stage::MarkerPlanningReady { .. }) => {
            control_frozen(previous, current) && left.current_map == right.current_map && same_end
        }
        (
            Stage::MarkerResolveRemoval { mapped_unit: a },
            Stage::MarkerWorkingIdentity { mapped_unit: b },
        ) => {
            marker_frame(previous, current, left, right)
                && same_end
                && a == b
                && match (
                    previous.marker_effect_continuation().active(),
                    current.marker_effect_continuation().active(),
                ) {
                    (Some(a), Some(b)) => {
                        a.with_program(
                            b.removal_site(),
                            a.planning(),
                            a.insertion_site(),
                            a.pending(),
                        ) == b
                            && a.removal_site().is_none()
                            && b.removal_site().is_some()
                    }
                    _ => false,
                }
        }
        (Stage::MarkerPlanningReady { boundary }, Stage::Idle) => {
            let (Some(a), Some(b)) = (
                previous.marker_effect_continuation().active(),
                current.marker_effect_continuation().active(),
            ) else {
                return false;
            };
            let Some(source) = a.planning().and_then(|p| p.source_boundary) else {
                return false;
            };
            previous.working_roots() == current.working_roots()
                && previous.base_frontier() == current.base_frontier()
                && previous.successor_frontier() == current.successor_frontier()
                && previous.next_record_ordinal() == current.next_record_ordinal()
                && left.current_map == right.current_map
                && same_end
                && a.with_program(None, None, None, Pending::None) == b
                && current.frontier()
                    == Frontier::Removing {
                        fragment_ordinal: fragment.key().ordinal(),
                        next_rank: source.rank(),
                        end_rank: source.rank(),
                        removed_markers: 0,
                        base_end: source,
                        successor_start: boundary,
                        successor_end: boundary,
                    }
        }
        _ => false,
    }
}
