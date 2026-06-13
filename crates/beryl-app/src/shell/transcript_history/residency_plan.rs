#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

use super::residency::TranscriptResidencyBudgetReason;

const DEFAULT_MAX_RESIDENT_TURNS: usize = 320;
const DEFAULT_MAX_RESIDENT_BYTES: usize = 100 * 1024 * 1024;
const DEFAULT_MAX_IN_FLIGHT_REQUESTS: usize = 1;
const DEFAULT_LEADING_VIEWPORT_MARGINS: usize = 3;
const DEFAULT_TRAILING_VIEWPORT_MARGINS: usize = 3;
const DEFAULT_ROW_HEIGHT: usize = 96;
const MINIMUM_ROW_HEIGHT: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptResidencyGrowthStrategy {
    FixedViewportMargins,
    SaturateBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyTargetPolicy {
    max_resident_turns: usize,
    max_resident_bytes: usize,
    max_in_flight_requests: usize,
    leading_viewport_margins: usize,
    trailing_viewport_margins: usize,
    default_row_height: usize,
    growth_strategy: TranscriptResidencyGrowthStrategy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyTargetInput {
    pub(crate) viewport: TranscriptResidencyViewport,
    pub(crate) turns: Vec<TranscriptResidencyTurnPlanInput>,
    pub(crate) active_turn_id: Option<String>,
    pub(crate) pinned_turn_ids: BTreeSet<String>,
    pub(crate) in_flight_requests: usize,
    pub(crate) policy: TranscriptResidencyTargetPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyViewport {
    pub(crate) visible_range: Range<usize>,
    pub(crate) viewport_height: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyTurnPlanInput {
    pub(crate) turn_id: String,
    pub(crate) source_position: Option<usize>,
    pub(crate) measured_height: Option<usize>,
    pub(crate) estimated_height: Option<usize>,
    pub(crate) estimated_resident_bytes: usize,
    pub(crate) resident: bool,
    pub(crate) oversized_fallback: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyTargetPlan {
    pub(crate) desired_full_turn_ids: Vec<String>,
    pub(crate) release_turn_ids: Vec<String>,
    pub(crate) oversized_turn_fallback_ids: Vec<String>,
    pub(crate) missing_transport_ranges: Vec<Range<usize>>,
    pub(crate) diagnostics: TranscriptResidencyTargetDiagnostics,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TranscriptResidencyTargetDiagnostics {
    pub(crate) viewport_margin_satisfied: bool,
    pub(crate) resident_turn_limit: bool,
    pub(crate) resident_byte_limit: bool,
    pub(crate) oversized_turn_fallback: bool,
    pub(crate) in_flight_limit: bool,
    pub(crate) limiting_reason: TranscriptResidencyBudgetReason,
    pub(crate) desired_resident_turns: usize,
    pub(crate) desired_resident_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequiredReason {
    Visible,
    Active,
    Pinned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionalReason {
    NearLeading,
    NearTrailing,
    FartherMargin,
    Growth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlannedTurn {
    input_index: usize,
    source_position: usize,
    turn_id: String,
    measured_height: usize,
    estimated_resident_bytes: usize,
    resident: bool,
    oversized_fallback: bool,
    top: usize,
    bottom: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Candidate {
    turn_index: usize,
    priority: CandidatePriority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CandidatePriority {
    Required(RequiredReason),
    Optional(OptionalReason, usize),
}

impl Default for TranscriptResidencyTargetPolicy {
    fn default() -> Self {
        Self {
            max_resident_turns: DEFAULT_MAX_RESIDENT_TURNS,
            max_resident_bytes: DEFAULT_MAX_RESIDENT_BYTES,
            max_in_flight_requests: DEFAULT_MAX_IN_FLIGHT_REQUESTS,
            leading_viewport_margins: DEFAULT_LEADING_VIEWPORT_MARGINS,
            trailing_viewport_margins: DEFAULT_TRAILING_VIEWPORT_MARGINS,
            default_row_height: DEFAULT_ROW_HEIGHT,
            growth_strategy: TranscriptResidencyGrowthStrategy::FixedViewportMargins,
        }
    }
}

impl TranscriptResidencyTargetPolicy {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_max_resident_turns(mut self, max_resident_turns: usize) -> Self {
        self.max_resident_turns = max_resident_turns;
        self
    }

    pub(crate) fn with_max_resident_bytes(mut self, max_resident_bytes: usize) -> Self {
        self.max_resident_bytes = max_resident_bytes;
        self
    }

    pub(crate) fn with_max_in_flight_requests(mut self, max_in_flight_requests: usize) -> Self {
        self.max_in_flight_requests = max_in_flight_requests;
        self
    }

    pub(crate) fn with_leading_viewport_margins(mut self, margins: usize) -> Self {
        self.leading_viewport_margins = margins;
        self
    }

    pub(crate) fn with_trailing_viewport_margins(mut self, margins: usize) -> Self {
        self.trailing_viewport_margins = margins;
        self
    }

    pub(crate) fn with_default_row_height(mut self, default_row_height: usize) -> Self {
        self.default_row_height = default_row_height.max(MINIMUM_ROW_HEIGHT);
        self
    }

    pub(crate) fn with_growth_strategy(
        mut self,
        growth_strategy: TranscriptResidencyGrowthStrategy,
    ) -> Self {
        self.growth_strategy = growth_strategy;
        self
    }
}

impl TranscriptResidencyViewport {
    pub(crate) fn new(visible_range: Range<usize>, viewport_height: usize) -> Self {
        Self {
            visible_range,
            viewport_height,
        }
    }
}

impl TranscriptResidencyTurnPlanInput {
    pub(crate) fn new(turn_id: impl Into<String>) -> Self {
        Self {
            turn_id: turn_id.into(),
            source_position: None,
            measured_height: None,
            estimated_height: None,
            estimated_resident_bytes: 0,
            resident: false,
            oversized_fallback: false,
        }
    }

    pub(crate) fn with_source_position(mut self, source_position: usize) -> Self {
        self.source_position = Some(source_position);
        self
    }

    pub(crate) fn with_measured_height(mut self, measured_height: usize) -> Self {
        self.measured_height = Some(measured_height.max(MINIMUM_ROW_HEIGHT));
        self
    }

    pub(crate) fn with_estimated_height(mut self, estimated_height: usize) -> Self {
        self.estimated_height = Some(estimated_height.max(MINIMUM_ROW_HEIGHT));
        self
    }

    pub(crate) fn with_estimated_resident_bytes(mut self, estimated_resident_bytes: usize) -> Self {
        self.estimated_resident_bytes = estimated_resident_bytes;
        self
    }

    pub(crate) fn with_resident(mut self, resident: bool) -> Self {
        self.resident = resident;
        self
    }

    pub(crate) fn with_oversized_fallback(mut self, oversized_fallback: bool) -> Self {
        self.oversized_fallback = oversized_fallback;
        self
    }
}

impl TranscriptResidencyTargetInput {
    pub(crate) fn new(
        viewport: TranscriptResidencyViewport,
        turns: Vec<TranscriptResidencyTurnPlanInput>,
    ) -> Self {
        Self {
            viewport,
            turns,
            active_turn_id: None,
            pinned_turn_ids: BTreeSet::new(),
            in_flight_requests: 0,
            policy: TranscriptResidencyTargetPolicy::default(),
        }
    }

    pub(crate) fn with_active_turn_id(mut self, active_turn_id: impl Into<String>) -> Self {
        self.active_turn_id = Some(active_turn_id.into());
        self
    }

    pub(crate) fn with_pinned_turn_ids<I, S>(mut self, turn_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.pinned_turn_ids = turn_ids
            .into_iter()
            .map(|turn_id| turn_id.as_ref().to_string())
            .collect();
        self
    }

    pub(crate) fn with_in_flight_requests(mut self, in_flight_requests: usize) -> Self {
        self.in_flight_requests = in_flight_requests;
        self
    }

    pub(crate) fn with_policy(mut self, policy: TranscriptResidencyTargetPolicy) -> Self {
        self.policy = policy;
        self
    }
}

pub(crate) fn plan_transcript_residency_target(
    input: TranscriptResidencyTargetInput,
) -> TranscriptResidencyTargetPlan {
    let turns = planned_turns(&input);
    if turns.is_empty() {
        return TranscriptResidencyTargetPlan {
            diagnostics: TranscriptResidencyTargetDiagnostics {
                viewport_margin_satisfied: true,
                in_flight_limit: input.in_flight_requests
                    >= input.policy.max_in_flight_requests.max(1),
                limiting_reason: in_flight_limiting_reason(&input),
                ..TranscriptResidencyTargetDiagnostics::default()
            },
            ..TranscriptResidencyTargetPlan::default()
        };
    }

    let visible_range = clamp_range(input.viewport.visible_range.clone(), turns.len());
    let (visible_top, visible_bottom) = visible_pixel_bounds(&turns, &visible_range, &input);
    let target_top = visible_top.saturating_sub(
        input
            .viewport
            .viewport_height
            .saturating_mul(input.policy.leading_viewport_margins),
    );
    let target_bottom = visible_bottom.saturating_add(
        input
            .viewport
            .viewport_height
            .saturating_mul(input.policy.trailing_viewport_margins),
    );
    let candidates = planning_candidates(&input, &turns, &visible_range, target_top, target_bottom);
    let target_turn_ids = target_turn_ids(&turns, target_top, target_bottom);

    build_plan(input, turns, candidates, target_turn_ids)
}

fn build_plan(
    input: TranscriptResidencyTargetInput,
    turns: Vec<PlannedTurn>,
    candidates: Vec<Candidate>,
    target_turn_ids: BTreeSet<String>,
) -> TranscriptResidencyTargetPlan {
    let turn_by_id = turns
        .iter()
        .enumerate()
        .map(|(index, turn)| (turn.turn_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut desired_full_turn_ids = Vec::new();
    let mut desired_set = BTreeSet::new();
    let mut fallback_turn_ids = Vec::new();
    let mut fallback_set = BTreeSet::new();
    let mut desired_bytes = 0usize;
    let mut resident_turn_limit = false;
    let mut resident_byte_limit = false;
    let mut oversized_turn_fallback = false;
    let mut active_turn_over_budget = false;
    let active_turn_id = input.active_turn_id.as_deref();

    for candidate in candidates {
        let turn = &turns[candidate.turn_index];
        let active_turn = turn_matches_id(turn, active_turn_id);
        if desired_set.contains(&turn.turn_id) || fallback_set.contains(&turn.turn_id) {
            continue;
        }
        if turn.oversized_fallback {
            if active_turn {
                active_turn_over_budget = true;
                resident_byte_limit = true;
                continue;
            }
            fallback_set.insert(turn.turn_id.clone());
            fallback_turn_ids.push(turn.turn_id.clone());
            oversized_turn_fallback = true;
            continue;
        }

        match candidate.priority {
            CandidatePriority::Required(_) => {
                if turn.estimated_resident_bytes > input.policy.max_resident_bytes {
                    if active_turn {
                        active_turn_over_budget = true;
                        resident_byte_limit = true;
                        continue;
                    }
                    fallback_set.insert(turn.turn_id.clone());
                    fallback_turn_ids.push(turn.turn_id.clone());
                    oversized_turn_fallback = true;
                    continue;
                }

                desired_set.insert(turn.turn_id.clone());
                desired_bytes = desired_bytes.saturating_add(turn.estimated_resident_bytes);
                desired_full_turn_ids.push(turn.turn_id.clone());
                if desired_full_turn_ids.len() > input.policy.max_resident_turns {
                    resident_turn_limit = true;
                }
                if desired_bytes > input.policy.max_resident_bytes {
                    resident_byte_limit = true;
                }
            }
            CandidatePriority::Optional(_, _) => {
                let would_exceed_turns =
                    desired_full_turn_ids.len() >= input.policy.max_resident_turns;
                let would_exceed_bytes = desired_bytes
                    .saturating_add(turn.estimated_resident_bytes)
                    > input.policy.max_resident_bytes;
                if would_exceed_turns || would_exceed_bytes {
                    resident_turn_limit |= would_exceed_turns;
                    resident_byte_limit |= would_exceed_bytes;
                    continue;
                }

                desired_set.insert(turn.turn_id.clone());
                desired_bytes = desired_bytes.saturating_add(turn.estimated_resident_bytes);
                desired_full_turn_ids.push(turn.turn_id.clone());
            }
        }
    }

    let release_for_budget = resident_turn_limit || resident_byte_limit;
    let release_turn_ids = turns
        .iter()
        .filter(|turn| turn.resident)
        .filter(|turn| !turn_matches_id(turn, active_turn_id))
        .filter(|turn| {
            fallback_set.contains(&turn.turn_id)
                || (release_for_budget && !desired_set.contains(&turn.turn_id))
        })
        .map(|turn| turn.turn_id.clone())
        .collect::<Vec<_>>();
    let in_flight_limit = input.in_flight_requests >= input.policy.max_in_flight_requests.max(1);
    let missing_transport_ranges = if in_flight_limit {
        Vec::new()
    } else {
        missing_transport_ranges(&turns, &turn_by_id, &desired_full_turn_ids)
    };

    let viewport_margin_satisfied = target_turn_ids.iter().all(|turn_id| {
        desired_set.contains(turn_id)
            || fallback_set.contains(turn_id)
            || active_turn_id == Some(turn_id.as_str())
    });
    let limiting_reason = target_limiting_reason(
        oversized_turn_fallback,
        active_turn_over_budget,
        resident_turn_limit,
        resident_byte_limit,
        in_flight_limit,
    );

    TranscriptResidencyTargetPlan {
        desired_full_turn_ids,
        release_turn_ids,
        oversized_turn_fallback_ids: fallback_turn_ids,
        missing_transport_ranges,
        diagnostics: TranscriptResidencyTargetDiagnostics {
            viewport_margin_satisfied,
            resident_turn_limit,
            resident_byte_limit,
            oversized_turn_fallback,
            in_flight_limit,
            limiting_reason,
            desired_resident_turns: desired_set.len(),
            desired_resident_bytes: desired_bytes,
        },
    }
}

fn turn_matches_id(turn: &PlannedTurn, turn_id: Option<&str>) -> bool {
    turn_id.is_some_and(|turn_id| turn.turn_id == turn_id)
}

fn planned_turns(input: &TranscriptResidencyTargetInput) -> Vec<PlannedTurn> {
    let mut turns = input
        .turns
        .iter()
        .enumerate()
        .map(|(input_index, turn)| {
            let measured_height = turn
                .measured_height
                .or(turn.estimated_height)
                .unwrap_or(input.policy.default_row_height)
                .max(MINIMUM_ROW_HEIGHT);
            PlannedTurn {
                input_index,
                source_position: turn.source_position.unwrap_or(input_index),
                turn_id: turn.turn_id.clone(),
                measured_height,
                estimated_resident_bytes: turn.estimated_resident_bytes,
                resident: turn.resident,
                oversized_fallback: turn.oversized_fallback,
                top: 0,
                bottom: 0,
            }
        })
        .collect::<Vec<_>>();
    turns.sort_by_key(|turn| (turn.source_position, turn.input_index));

    let mut offset = 0usize;
    for turn in &mut turns {
        turn.top = offset;
        offset = offset.saturating_add(turn.measured_height);
        turn.bottom = offset;
    }

    turns
}

fn planning_candidates(
    input: &TranscriptResidencyTargetInput,
    turns: &[PlannedTurn],
    visible_range: &Range<usize>,
    target_top: usize,
    target_bottom: usize,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();

    for turn_index in visible_range.clone() {
        if seen.insert(turns[turn_index].turn_id.clone()) {
            candidates.push(Candidate {
                turn_index,
                priority: CandidatePriority::Required(RequiredReason::Visible),
            });
        }
    }

    if let Some(active_turn_id) = input.active_turn_id.as_deref()
        && let Some(turn_index) = turns.iter().position(|turn| turn.turn_id == active_turn_id)
        && seen.insert(active_turn_id.to_string())
    {
        candidates.push(Candidate {
            turn_index,
            priority: CandidatePriority::Required(RequiredReason::Active),
        });
    }

    for pinned_turn_id in &input.pinned_turn_ids {
        if let Some(turn_index) = turns
            .iter()
            .position(|turn| &turn.turn_id == pinned_turn_id)
            && seen.insert(pinned_turn_id.clone())
        {
            candidates.push(Candidate {
                turn_index,
                priority: CandidatePriority::Required(RequiredReason::Pinned),
            });
        }
    }

    let optional_candidates =
        optional_candidates(input, turns, visible_range, target_top, target_bottom);
    candidates.extend(optional_candidates.into_iter().filter(|candidate| {
        let turn_id = &turns[candidate.turn_index].turn_id;
        !seen.contains(turn_id)
    }));
    candidates
}

fn optional_candidates(
    input: &TranscriptResidencyTargetInput,
    turns: &[PlannedTurn],
    visible_range: &Range<usize>,
    target_top: usize,
    target_bottom: usize,
) -> Vec<Candidate> {
    let (visible_top, visible_bottom) = visible_pixel_bounds(turns, visible_range, input);
    let near_distance = input.viewport.viewport_height.max(MINIMUM_ROW_HEIGHT);
    let mut near_leading = Vec::new();
    let mut near_trailing = Vec::new();
    let mut farther = Vec::new();
    let mut growth = Vec::new();

    for (turn_index, turn) in turns.iter().enumerate() {
        if visible_range.contains(&turn_index) {
            continue;
        }

        let intersects_target = range_intersects(turn.top, turn.bottom, target_top, target_bottom);
        if !intersects_target
            && input.policy.growth_strategy != TranscriptResidencyGrowthStrategy::SaturateBudget
        {
            continue;
        }

        let (reason, distance) = if turn.bottom <= visible_top {
            let distance = visible_top.saturating_sub(turn.bottom);
            if !intersects_target {
                (OptionalReason::Growth, distance)
            } else if distance < near_distance {
                (OptionalReason::NearLeading, distance)
            } else {
                (OptionalReason::FartherMargin, distance)
            }
        } else if turn.top >= visible_bottom {
            let distance = turn.top.saturating_sub(visible_bottom);
            if !intersects_target {
                (OptionalReason::Growth, distance)
            } else if distance < near_distance {
                (OptionalReason::NearTrailing, distance)
            } else {
                (OptionalReason::FartherMargin, distance)
            }
        } else {
            (OptionalReason::FartherMargin, 0)
        };
        let candidate = Candidate {
            turn_index,
            priority: CandidatePriority::Optional(reason, distance),
        };
        match reason {
            OptionalReason::NearLeading => near_leading.push(candidate),
            OptionalReason::NearTrailing => near_trailing.push(candidate),
            OptionalReason::FartherMargin => farther.push(candidate),
            OptionalReason::Growth => growth.push(candidate),
        }
    }

    near_leading.sort_by_key(candidate_sort_key);
    near_trailing.sort_by_key(candidate_sort_key);
    farther.sort_by_key(candidate_sort_key);
    growth.sort_by_key(candidate_sort_key);
    near_leading
        .into_iter()
        .chain(near_trailing)
        .chain(farther)
        .chain(growth)
        .collect()
}

fn candidate_sort_key(candidate: &Candidate) -> (usize, usize) {
    let distance = match candidate.priority {
        CandidatePriority::Required(_) => 0,
        CandidatePriority::Optional(_, distance) => distance,
    };
    (distance, candidate.turn_index)
}

fn visible_pixel_bounds(
    turns: &[PlannedTurn],
    visible_range: &Range<usize>,
    input: &TranscriptResidencyTargetInput,
) -> (usize, usize) {
    let visible_top = turns
        .get(visible_range.start)
        .map_or_else(|| total_height(turns), |turn| turn.top);
    let visible_bottom = visible_top.saturating_add(input.viewport.viewport_height);
    if visible_range.is_empty() {
        return (visible_top, visible_bottom);
    }

    let row_bottom = turns
        .get(visible_range.end.saturating_sub(1))
        .map_or(visible_bottom, |turn| turn.bottom);
    (visible_top, visible_bottom.max(row_bottom))
}

fn target_turn_ids(
    turns: &[PlannedTurn],
    target_top: usize,
    target_bottom: usize,
) -> BTreeSet<String> {
    turns
        .iter()
        .filter(|turn| range_intersects(turn.top, turn.bottom, target_top, target_bottom))
        .map(|turn| turn.turn_id.clone())
        .collect()
}

fn missing_transport_ranges(
    turns: &[PlannedTurn],
    turn_by_id: &BTreeMap<&str, usize>,
    desired_full_turn_ids: &[String],
) -> Vec<Range<usize>> {
    let mut missing_positions = desired_full_turn_ids
        .iter()
        .filter_map(|turn_id| {
            let turn = &turns[*turn_by_id.get(turn_id.as_str())?];
            (!turn.resident).then_some(turn.source_position)
        })
        .collect::<Vec<_>>();
    missing_positions.sort_unstable();
    missing_positions.dedup();

    let mut ranges: Vec<Range<usize>> = Vec::new();
    for position in missing_positions {
        match ranges.last_mut() {
            Some(range) if range.end == position => {
                range.end = range.end.saturating_add(1);
            }
            Some(range) if range.end == position.saturating_add(1) => {}
            _ => ranges.push(position..position.saturating_add(1)),
        }
    }
    ranges
}

fn in_flight_limiting_reason(
    input: &TranscriptResidencyTargetInput,
) -> TranscriptResidencyBudgetReason {
    if input.in_flight_requests >= input.policy.max_in_flight_requests.max(1) {
        TranscriptResidencyBudgetReason::InFlightRequestLimit
    } else {
        TranscriptResidencyBudgetReason::None
    }
}

fn target_limiting_reason(
    oversized_turn_fallback: bool,
    active_turn_over_budget: bool,
    resident_turn_limit: bool,
    resident_byte_limit: bool,
    in_flight_limit: bool,
) -> TranscriptResidencyBudgetReason {
    if active_turn_over_budget {
        return TranscriptResidencyBudgetReason::PinnedResidentOverBudget;
    }
    if oversized_turn_fallback {
        return TranscriptResidencyBudgetReason::OversizedTurnFallback;
    }
    if resident_turn_limit {
        return TranscriptResidencyBudgetReason::ResidentTurnLimit;
    }
    if resident_byte_limit {
        return TranscriptResidencyBudgetReason::ResidentByteLimit;
    }
    if in_flight_limit {
        return TranscriptResidencyBudgetReason::InFlightRequestLimit;
    }
    TranscriptResidencyBudgetReason::None
}

fn clamp_range(range: Range<usize>, len: usize) -> Range<usize> {
    let start = range.start.min(len);
    let end = range.end.min(len).max(start);
    start..end
}

fn range_intersects(
    left_start: usize,
    left_end: usize,
    right_start: usize,
    right_end: usize,
) -> bool {
    if left_start == left_end {
        return left_start >= right_start && left_start <= right_end;
    }
    left_start < right_end && right_start < left_end
}

fn total_height(turns: &[PlannedTurn]) -> usize {
    turns.last().map_or(0, |turn| turn.bottom)
}
