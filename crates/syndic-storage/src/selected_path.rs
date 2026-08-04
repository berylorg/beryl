use beryl_model::SyndicTurnId;

use crate::{
    ProjectionLifecycle, SyndicTimestamp, TurnDepth, TurnIncompleteReason, TurnLifecycle,
    TurnRecord,
};

pub(crate) const MAX_ANCESTOR_STEPS: usize = 2_080;

pub(crate) fn deterministic_skip_depth(depth: u64) -> Option<u64> {
    if depth == 1 {
        None
    } else {
        Some(std::cmp::max(depth & (depth - 1), 1))
    }
}

/// Depth of the deterministic skip ancestor stored by one non-root turn.
pub(crate) fn ancestor_skip_depth(depth: TurnDepth) -> Option<TurnDepth> {
    deterministic_skip_depth(depth.get()).and_then(|depth| TurnDepth::new(depth).ok())
}

/// Lifts one immutable turn to an exact ancestor depth with bounded point work.
///
/// Every skip clears at least one set bit from the current depth. When that
/// would cross the target, one parent edge enters the next smaller dyadic
/// region. Across all 64 regions the triangular upper bound is 64 + 63 + ...
/// + 1 = 2,080 point reads, independent of the number of stored turns.
pub(crate) fn lift_to_depth<E>(
    mut current: TurnRecord,
    target: TurnDepth,
    mut load: impl FnMut(SyndicTurnId) -> Result<TurnRecord, E>,
    mut invalid: impl FnMut(&'static str) -> E,
) -> Result<TurnRecord, E> {
    if target.get() > current.depth().get() {
        return Err(invalid("selected-path target depth exceeds its tail"));
    }

    for _ in 0..MAX_ANCESTOR_STEPS {
        if current.depth() == target {
            return Ok(current);
        }

        let skip_depth = ancestor_skip_depth(current.depth())
            .ok_or_else(|| invalid("selected-path lift reached a root above its target"))?;
        let (next_id, expected_depth) = if skip_depth.get() >= target.get() {
            (
                current.ancestor_skip().ok_or_else(|| {
                    invalid("non-root turn is missing its deterministic ancestor skip")
                })?,
                skip_depth,
            )
        } else {
            (
                current
                    .parent()
                    .turn()
                    .ok_or_else(|| invalid("selected-path lift reached a root above its target"))?,
                TurnDepth::new(current.depth().get() - 1)
                    .expect("a non-root turn always has a nonzero parent depth"),
            )
        };
        let next = load(next_id)?;
        if next.id() != next_id || next.depth() != expected_depth {
            return Err(invalid(
                "selected-path ancestor identity or deterministic depth disagrees",
            ));
        }
        current = next;
    }

    if current.depth() == target {
        return Ok(current);
    }

    Err(invalid("selected-path ancestry exceeded its bounded proof"))
}

/// Computes the exact deterministic skip stored by one new child turn.
pub(crate) fn child_ancestor_skip<E>(
    parent: TurnRecord,
    child_depth: TurnDepth,
    load: impl FnMut(SyndicTurnId) -> Result<TurnRecord, E>,
    invalid: impl FnMut(&'static str) -> E,
) -> Result<SyndicTurnId, E> {
    let skip_depth = ancestor_skip_depth(child_depth)
        .expect("every child turn has a deterministic ancestor-skip depth");
    lift_to_depth(parent, skip_depth, load, invalid).map(|turn| turn.id())
}

/// Proves whether `candidate` belongs to the immutable ancestry of `tail`.
pub(crate) fn includes_turn<E>(
    tail: TurnRecord,
    candidate: &TurnRecord,
    load: impl FnMut(SyndicTurnId) -> Result<TurnRecord, E>,
    invalid: impl FnMut(&'static str) -> E,
) -> Result<bool, E> {
    if candidate.depth().get() > tail.depth().get() {
        return Ok(false);
    }
    lift_to_depth(tail, candidate.depth(), load, invalid)
        .map(|at_candidate_depth| at_candidate_depth.id() == candidate.id())
}

/// Exact bounded accumulator over one named thread's selected turn path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SelectedPathFold {
    all_finalized: bool,
    last_activity_at: Option<SyndicTimestamp>,
}

impl SelectedPathFold {
    pub(crate) const fn empty() -> Self {
        Self {
            all_finalized: true,
            last_activity_at: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn include(
        self,
        submitted_at: SyndicTimestamp,
        lifecycle: TurnLifecycle,
        item_count: u64,
        finalized_item_count: u64,
        open_item_count: u64,
        history_blocking_item_count: u64,
        provider_observation_issue: Option<crate::ProviderObservationIssueReason>,
        incomplete_reason: Option<TurnIncompleteReason>,
        updated_at: SyndicTimestamp,
    ) -> Self {
        let activity = submitted_at.max(updated_at);
        Self {
            all_finalized: self.all_finalized
                && turn_history_is_complete(
                    lifecycle,
                    item_count,
                    finalized_item_count,
                    open_item_count,
                    history_blocking_item_count,
                    provider_observation_issue,
                    incomplete_reason,
                ),
            last_activity_at: Some(
                self.last_activity_at
                    .map_or(activity, |current| current.max(activity)),
            ),
        }
    }

    pub(crate) const fn all_finalized(self) -> bool {
        self.all_finalized
    }

    pub(crate) const fn last_activity_at(self) -> Option<SyndicTimestamp> {
        self.last_activity_at
    }

    pub(crate) const fn summary_complete(self, lifecycle: ProjectionLifecycle) -> bool {
        matches!(lifecycle, ProjectionLifecycle::Current) && self.all_finalized
    }
}

pub(crate) const fn turn_history_is_complete(
    lifecycle: TurnLifecycle,
    item_count: u64,
    finalized_item_count: u64,
    open_item_count: u64,
    history_blocking_item_count: u64,
    provider_observation_issue: Option<crate::ProviderObservationIssueReason>,
    incomplete_reason: Option<TurnIncompleteReason>,
) -> bool {
    lifecycle.is_proven_terminal()
        && incomplete_reason.is_none()
        && provider_observation_issue.is_none()
        && finalized_item_count == item_count
        && open_item_count == 0
        && history_blocking_item_count == 0
}

#[cfg(test)]
mod tests {
    use beryl_model::{SyndicPathDigest, SyndicThreadId, SyndicTurnId};

    use super::*;
    use crate::{ConversationParent, SyndicTimestamp, TurnKind};

    #[test]
    fn deterministic_skips_lift_any_u64_depth_within_the_fixed_bound() {
        let targets = [
            1,
            2,
            3,
            4,
            5,
            63,
            64,
            65,
            1_023,
            1_024,
            1_025,
            u64::MAX - 2,
            u64::MAX - 1,
            u64::MAX,
        ];
        for target in targets {
            let mut reads = 0;
            let found = lift_to_depth(
                turn(u64::MAX),
                TurnDepth::new(target).unwrap(),
                |id| {
                    reads += 1;
                    Ok::<_, ()>(turn(depth_from_id(id)))
                },
                |_| (),
            )
            .unwrap();
            assert_eq!(found.depth().get(), target);
            assert!(reads <= MAX_ANCESTOR_STEPS);
        }
    }

    #[test]
    fn selected_path_membership_distinguishes_an_off_path_turn_at_the_same_depth() {
        let tail = turn(4);
        let selected = turn(2);
        let mut off_path = [0_u8; 16];
        off_path[..8].copy_from_slice(&99_u64.to_be_bytes());
        let off_path = TurnRecord::new(
            SyndicTurnId::from_bytes(off_path),
            selected.origin_thread_id(),
            selected.kind(),
            selected.parent(),
            selected.ancestor_skip(),
            selected.depth(),
            selected.chain_digest(),
            selected.submitted_at(),
        );
        let load = |id| Ok::<_, ()>(turn(depth_from_id(id)));
        assert!(includes_turn(tail.clone(), &selected, load, |_| ()).unwrap());
        assert!(!includes_turn(tail, &off_path, load, |_| ()).unwrap());
    }

    fn turn(depth: u64) -> TurnRecord {
        let depth_value = TurnDepth::new(depth).unwrap();
        let parent = if depth == 1 {
            ConversationParent::Root
        } else {
            ConversationParent::Turn(id(depth - 1))
        };
        TurnRecord::new(
            id(depth),
            SyndicThreadId::from_bytes([7; 16]),
            TurnKind::OrdinaryUser,
            parent,
            ancestor_skip_depth(depth_value).map(|skip| id(skip.get())),
            depth_value,
            SyndicPathDigest::from_bytes([11; 32]),
            SyndicTimestamp::from_unix_millis(depth),
        )
    }

    fn id(depth: u64) -> SyndicTurnId {
        let mut bytes = [0_u8; 16];
        bytes[8..].copy_from_slice(&depth.to_be_bytes());
        SyndicTurnId::from_bytes(bytes)
    }

    fn depth_from_id(id: SyndicTurnId) -> u64 {
        u64::from_be_bytes(id.as_bytes()[8..].try_into().unwrap())
    }
}
