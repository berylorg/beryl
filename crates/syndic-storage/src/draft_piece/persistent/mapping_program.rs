use super::super::build_mapping::MappingContext;
use super::*;

mod text;
mod text_splice;

pub(super) use crate::draft_piece::DraftPieceMappingSpliceKindV1 as Kind;
pub(super) use crate::draft_piece::DraftPieceMappingStageV1 as Stage;

pub(super) fn invalid<T>() -> Result<T, DraftPiecePrepareErrorV1> {
    Err(DraftPiecePrepareErrorV1::InvalidRoot)
}

pub(super) fn mapping(
    context: &BuildContext<'_>,
) -> Result<DraftPieceBuildMappingV1, DraftPiecePrepareErrorV1> {
    context.mapping.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)
}

pub(super) fn set_stage(
    context: &mut BuildContext<'_>,
    stage: Stage,
) -> Result<(), DraftPiecePrepareErrorV1> {
    context
        .mapping
        .as_mut()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
        .mapping_stage = stage;
    Ok(())
}

fn map_operation<T>(
    context: &mut BuildContext<'_>,
    operation: impl FnOnce(&mut MappingContext<'_, '_>) -> Result<T, DraftPiecePrepareErrorV1>,
) -> Result<T, DraftPiecePrepareErrorV1> {
    let acquisition = context
        .acquisition
        .as_ref()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    let mut owner = [0u8; 48];
    owner[..16].copy_from_slice(context.draft_id.as_bytes());
    owner[16..32].copy_from_slice(
        context
            .session_id
            .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?
            .as_bytes(),
    );
    owner[32..].copy_from_slice(context.operation_id.as_bytes());
    let mut map = MappingContext::new(acquisition, owner, &mut context.ordinal);
    let result = operation(&mut map)?;
    context.records_read = context
        .records_read
        .checked_add(map.acquired as u64)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    context.mapping_records.extend(map.emitted);
    Ok(result)
}

pub(super) fn source_cut(
    context: &mut BuildContext<'_>,
    unit: u128,
    copy: bool,
) -> Result<u128, DraftPiecePrepareErrorV1> {
    let root = mapping(context)?.current_map;
    map_operation(context, |map| map.source_cut(root, unit, copy))
}

pub(super) fn begin_delete(
    context: &mut BuildContext<'_>,
    splice: DraftPieceMappingSpliceV1,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let target = mapping(context)?.current_map;
    let remaining_end = splice
        .a
        .checked_add(splice.removed)
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    set_stage(
        context,
        Stage::DeleteMap {
            splice,
            target,
            remaining_end,
        },
    )
}

pub(super) fn consume_ready(
    context: &mut BuildContext<'_>,
    kind: Kind,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let Stage::Ready(ready) = mapping(context)?.mapping_stage else {
        return invalid();
    };
    if ready.kind != kind {
        return invalid();
    }
    let mapping = context
        .mapping
        .as_mut()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    mapping.current_map = ready.target;
    mapping.mapping_stage = Stage::Idle;
    Ok(())
}

pub(super) fn verify_splice(
    context: &BuildContext<'_>,
    kind: Kind,
    a: u128,
    removed: u128,
    inserted: u128,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let Some(mapping) = context.mapping else {
        return Ok(());
    };
    let Stage::Ready(ready) = mapping.mapping_stage else {
        return invalid();
    };
    if ready.kind != kind || ready.a != a || ready.removed != removed || ready.inserted != inserted
    {
        return invalid();
    }
    Ok(())
}

pub(super) fn finish(
    context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    frontier: DraftPieceBuildFrontierV1,
) -> DraftPieceTreeQuantumV1 {
    finish_quantum(
        context,
        build.working_roots(),
        build.base_frontier(),
        build.successor_frontier(),
        frontier,
        None,
        None,
    )
}

pub(super) fn advance(
    mut context: BuildContext<'_>,
    build: &DraftPieceBuildRecordV1,
    fragment: Option<&DraftPieceBuildFragmentV1>,
) -> Result<DraftPieceTreeQuantumV1, DraftPiecePrepareErrorV1> {
    let state = mapping(&context)?;
    let next = match state.mapping_stage {
        Stage::DeleteMap {
            splice,
            target,
            remaining_end,
        } => {
            let end = splice
                .a
                .checked_add(splice.removed)
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            if splice.a >= remaining_end
                || remaining_end > end
                || target.measure().source != state.current_map.measure().source
                || target.measure().target.checked_add(end - remaining_end)
                    != Some(state.current_map.measure().target)
            {
                return invalid();
            }
            let (target, remaining_end) = map_operation(&mut context, |map| {
                map.delete_leaf(target, splice.a, remaining_end)
            })?;
            Some(if remaining_end == splice.a {
                Stage::MapComplete { splice, target }
            } else {
                Stage::DeleteMap {
                    splice,
                    target,
                    remaining_end,
                }
            })
        }
        Stage::InsertMap { splice } => {
            let target = map_operation(&mut context, |map| {
                map.insert(state.current_map, splice.a, splice.inserted)
            })?;
            Some(Stage::MapComplete { splice, target })
        }
        Stage::MapComplete { splice, target } => {
            if target.measure().source != state.current_map.measure().source
                || state
                    .current_map
                    .measure()
                    .target
                    .checked_sub(splice.removed)
                    .and_then(|n| n.checked_add(splice.inserted))
                    != Some(target.measure().target)
            {
                return invalid();
            }
            Some(Stage::Ready(DraftPieceMappingReadySpliceV1 {
                kind: splice.kind,
                a: splice.a,
                removed: splice.removed,
                inserted: splice.inserted,
                target,
            }))
        }
        Stage::RefreshMap => {
            let unit = state
                .fragment_source_end_unit
                .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
            Some(Stage::RefreshSequence {
                mapped_unit: source_cut(&mut context, unit, false)?,
            })
        }
        Stage::RefreshSequence { mapped_unit } => {
            let roots = build
                .marker_effect_continuation()
                .active()
                .map_or(build.working_roots(), |active| active.working_roots());
            let (sequence, _, _) = load_working_roots(&mut context, roots)?;
            let fact = sequence_units::at_unit(&mut context, sequence, mapped_unit)?;
            Some(Stage::PublishReady {
                successor_boundary: durable_boundary(fact.boundary),
                logical_offset: fact.logical_offset,
            })
        }
        _ => None,
    };
    if let Some(next) = next {
        set_stage(&mut context, next)?;
        let mut quantum = finish(context, build, build.frontier());
        if let (Stage::Ready(ready), Some(active)) =
            (next, build.marker_effect_continuation().active())
        {
            let pending = match ready.kind {
                Kind::MarkerDelete => DraftPieceMarkerPendingV1::RemoveSequence,
                Kind::MarkerInsert => DraftPieceMarkerPendingV1::InsertSequence,
                _ => return invalid(),
            };
            quantum.marker_effect_continuation = Some(marker_program::replace_active(
                build,
                marker_program::with_pending(active, pending),
            ));
        }
        return Ok(quantum);
    }
    if build.marker_effect_continuation().active().is_some() {
        return marker_program::advance(
            context,
            build,
            fragment.ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?,
        );
    }
    if let Some(fragment) =
        fragment.filter(|fragment| fragment.replacement().marker_effect().is_some())
    {
        return marker_program::activate(context, build, fragment);
    }
    text::advance(context, build, fragment)
}

pub(super) fn publish_mapping(
    context: &mut BuildContext<'_>,
) -> Result<(), DraftPiecePrepareErrorV1> {
    let mapping = context
        .mapping
        .as_mut()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    if !matches!(mapping.mapping_stage, Stage::PublishReady { .. }) {
        return invalid();
    }
    mapping.completed_source_unit = mapping
        .fragment_source_end_unit
        .take()
        .ok_or(DraftPiecePrepareErrorV1::InvalidRoot)?;
    mapping.mapping_stage = Stage::Idle;
    Ok(())
}
