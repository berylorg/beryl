use beryl_model::{SyndicPathDigest, SyndicThreadId};

use crate::{ThreadLineageDepth, ThreadRecord, child_thread_lineage_digest};

pub(crate) fn lift_to_depth<E>(
    mut current: ThreadRecord,
    target: ThreadLineageDepth,
    mut load: impl FnMut(SyndicThreadId) -> Result<ThreadRecord, E>,
    mut invalid: impl FnMut(&'static str) -> E,
) -> Result<ThreadRecord, E> {
    if target > current.lineage_depth() {
        return Err(invalid("thread-lineage target depth exceeds its leaf"));
    }
    for _ in 0..crate::selected_path::MAX_ANCESTOR_STEPS {
        if current.lineage_depth() == target {
            return Ok(current);
        }
        let skip_depth =
            crate::selected_path::deterministic_skip_depth(current.lineage_depth().get())
                .ok_or_else(|| invalid("thread-lineage lift reached a root above its target"))?;
        let (next_id, expected_depth) = if skip_depth >= target.get() {
            (
                current.lineage_ancestor_skip().ok_or_else(|| {
                    invalid("non-root thread is missing its deterministic ancestor skip")
                })?,
                skip_depth,
            )
        } else {
            (
                current.parent_thread_id().ok_or_else(|| {
                    invalid("thread-lineage lift reached a root above its target")
                })?,
                current.lineage_depth().get() - 1,
            )
        };
        let next = load(next_id)?;
        if next.id() != next_id || next.lineage_depth().get() != expected_depth {
            return Err(invalid(
                "thread-lineage ancestor identity or deterministic depth disagrees",
            ));
        }
        current = next;
    }
    if current.lineage_depth() == target {
        Ok(current)
    } else {
        Err(invalid("thread lineage exceeded its bounded proof"))
    }
}

pub(crate) fn child_shape<E>(
    child_id: SyndicThreadId,
    parent: ThreadRecord,
    load: impl FnMut(SyndicThreadId) -> Result<ThreadRecord, E>,
    mut invalid: impl FnMut(&'static str) -> E,
) -> Result<(ThreadLineageDepth, SyndicPathDigest, SyndicThreadId), E> {
    let child_depth = parent
        .lineage_depth()
        .get()
        .checked_add(1)
        .ok_or_else(|| invalid("thread-lineage depth exhausted"))?;
    let depth = ThreadLineageDepth::new(child_depth)
        .expect("checked nonzero thread-lineage depth is valid");
    let skip_depth = ThreadLineageDepth::new(
        crate::selected_path::deterministic_skip_depth(depth.get())
            .expect("every child has a deterministic skip depth"),
    )
    .expect("deterministic skip depth is nonzero");
    let skip = lift_to_depth(parent.clone(), skip_depth, load, invalid)?.id();
    Ok((
        depth,
        child_thread_lineage_digest(child_id, parent.id(), parent.lineage_digest()),
        skip,
    ))
}
