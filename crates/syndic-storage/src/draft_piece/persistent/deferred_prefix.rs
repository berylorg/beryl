use super::*;
use crate::draft_piece::deferred_prefix::{is_prefix, is_removal_at};
use DraftPieceActiveMarkerPhaseV1 as Phase;
use DraftPieceMarkerPendingV1 as Pending;
use mapping_program::Stage;

fn fragment(
    context: &BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    ordinal: u64,
) -> Result<DraftPieceBuildFragmentV1, DraftPiecePrepareErrorV1> {
    super::super::mutation::sequence_advance::fragment(
        context
            .acquisition
            .as_ref()
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        build,
        ordinal,
    )
}

pub(super) fn enter(
    context: &mut BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    prefix: &DraftPieceBuildFragmentV1,
) -> Result<bool, DraftPiecePrepareErrorV1> {
    let DraftPieceBuildFrontierV1::Applying {
        fragment_ordinal,
        successor_start,
        successor_end,
        ..
    } = build.frontier()
    else {
        return Ok(false);
    };
    let mapping = mapping_program::mapping(context)?;
    if successor_start == successor_end
        || !is_prefix(prefix)
        || prefix.key().ordinal() != fragment_ordinal
        || build.marker_effect_continuation().active().is_some()
        || !matches!(
            mapping.mapping_stage,
            Stage::Idle
                | Stage::Ready(DraftPieceMappingReadySpliceV1 {
                    kind: DraftPieceMappingSpliceKindV1::TextDelete,
                    ..
                })
        )
    {
        return Ok(false);
    }
    let next = fragment_ordinal
        .checked_add(1)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if next > build.fragment_count() {
        return Ok(false);
    }
    let lookahead = fragment(context, build, next)?;
    if !is_removal_at(&lookahead, prefix.replacement().end().utf8_offset()) {
        return Ok(false);
    }
    mapping_program::set_stage(context, Stage::Idle)?;
    let old = build.marker_effect_continuation();
    context.marker_continuation = Some(DraftPieceMarkerEffectContinuationV1::new(
        old.source_logical_frontier(),
        old.successor_logical_frontier(),
        DraftPieceMarkerEffectScanFrontierV1::new(
            next,
            Some(canonical_fragment_endpoint(prefix)),
            old.scan().completed_effect_count(),
            old.scan().effect_chain(),
        ),
        None,
    ));
    Ok(true)
}

#[inline(never)]
pub(super) fn advance(
    context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    prefix: &DraftPieceBuildFragmentV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    if !is_prefix(prefix)
        || crate::draft_piece::deferred_prefix::prefix_ordinal(build.frontier())
            != Some(prefix.key().ordinal())
    {
        return mapping_program::invalid();
    }
    let old = build.marker_effect_continuation_ref();
    let ordinal = old.scan().next_fragment_ordinal();
    let removal = fragment(&context, build, ordinal)?;
    if !is_removal_at(&removal, prefix.replacement().end().utf8_offset()) {
        return mapping_program::invalid();
    }
    let Some(active) = old.active_ref() else {
        return marker_program::activate(context, build, &removal);
    };
    if active.phase() == Phase::DerivingInsertionGap {
        return advance_prefix_completion(context, build, prefix, *active);
    }
    if active.phase() == Phase::Removing
        && active.pending() == Pending::None
        && mapping_program::mapping(&context)?.mapping_stage == Stage::MarkerMapBoundary
    {
        return advance_removal_boundary(context, build, prefix, &removal, *active);
    }
    marker_program::advance(context, build, &removal)
}

#[inline(never)]
fn advance_prefix_completion(
    mut context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    prefix: &DraftPieceBuildFragmentV1,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    if let DraftPieceBuildFrontierV1::Applying {
        fragment_ordinal,
        successor_start,
        successor_end,
        ..
    } = build.frontier()
    {
        if successor_start == successor_end {
            let base_end = active
                .planning()
                .and_then(|plan| plan.source_boundary)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            let active = marker_program::update(
                active,
                active.working_roots(),
                Phase::Publishing,
                None,
                None,
                None,
                Pending::None,
            );
            context.marker_continuation = Some(marker_program::replace_active(build, active));
            mapping_program::set_stage(&mut context, Stage::RefreshMap)?;
            return Ok(mapping_program::finish(
                context,
                build,
                DraftPieceBuildFrontierV1::Inserting {
                    fragment_ordinal,
                    next_piece: 0,
                    next_byte: 0,
                    base_end,
                    successor_end,
                },
            ));
        }
    }
    return mapping_program::text::advance(context, build, Some(prefix));
}

#[inline(never)]
fn advance_removal_boundary(
    mut context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    prefix: &DraftPieceBuildFragmentV1,
    removal: &DraftPieceBuildFragmentV1,
    active: DraftPieceActiveMarkerEffectV1,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let old = build.marker_effect_continuation_ref();
    let ordinal = old.scan_ref().next_fragment_ordinal();

    let next = ordinal
        .checked_add(1)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let continues = if next > build.fragment_count() {
        false
    } else {
        is_removal_at(
            &fragment(&context, build, next)?,
            prefix.replacement().end().utf8_offset(),
        )
    };
    mapping_program::set_stage(&mut context, Stage::Idle)?;
    if continues {
        let count = old
            .scan()
            .completed_effect_count()
            .checked_add(1)
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
        let endpoint = canonical_fragment_endpoint(&removal);
        let scan = DraftPieceMarkerEffectScanFrontierV1::new(
            next,
            Some(endpoint),
            count,
            draft_piece_marker_effect_chain_link_v1(
                old.scan().effect_chain(),
                removal.key(),
                endpoint.digest(),
                count,
                active.working_roots(),
            ),
        );
        context.marker_continuation = Some(DraftPieceMarkerEffectContinuationV1::new(
            old.source_logical_frontier(),
            old.successor_logical_frontier(),
            scan,
            None,
        ));
        return Ok(finish_quantum(
            context,
            active.working_roots(),
            build.base_frontier(),
            build.successor_frontier(),
            build.frontier(),
            None,
            None,
        ));
    }
    let active = marker_program::update(
        active,
        active.working_roots(),
        Phase::DerivingInsertionGap,
        None,
        active.planning(),
        None,
        Pending::None,
    );
    context.marker_continuation = Some(marker_program::replace_active(build, active));
    return Ok(mapping_program::finish(context, build, build.frontier()));
}
