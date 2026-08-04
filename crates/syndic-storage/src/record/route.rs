use beryl_model::{
    AcceptedInputRevision, InputGateRevision, SyndicAcceptedInputId, SyndicItemId, SyndicThreadId,
    SyndicTurnId,
};

use crate::{
    AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedRouteGeneration, AcceptedRouteRevision,
    NextTurnReason, PendingSteeringTargetProof, SteeringTargetProof, SyndicRecordError,
    SyndicTimestamp,
};

mod abandonment;

pub use abandonment::*;

/// Exact target selected once for a whole contiguous accepted-input route generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcceptedRouteTarget {
    AwaitingSteering(PendingSteeringTargetProof),
    Steering(SteeringTargetProof),
    AwaitingTerminal(SteeringTargetProof),
    NextTurn(NextTurnReason),
    ProjectionLost(AcceptedRouteProjectionLostProof),
}

impl AcceptedRouteTarget {
    #[must_use]
    pub const fn active_turn_id(&self) -> Option<SyndicTurnId> {
        match self {
            Self::AwaitingSteering(target) => Some(target.active_turn_id()),
            Self::Steering(target) | Self::AwaitingTerminal(target) => {
                Some(target.pending().active_turn_id())
            }
            Self::NextTurn(_) | Self::ProjectionLost(_) => None,
        }
    }
}

/// Revision-bound pointer to the currently selected route generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedRouteHeadProof {
    generation: AcceptedRouteGeneration,
    revision: AcceptedRouteRevision,
}

impl AcceptedRouteHeadProof {
    #[must_use]
    pub const fn new(generation: AcceptedRouteGeneration, revision: AcceptedRouteRevision) -> Self {
        Self {
            generation,
            revision,
        }
    }

    #[must_use]
    pub const fn generation(self) -> AcceptedRouteGeneration {
        self.generation
    }

    #[must_use]
    pub const fn revision(self) -> AcceptedRouteRevision {
        self.revision
    }
}

/// Current route-generation head for one thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedRouteGenerationHeadRecord {
    thread_id: SyndicThreadId,
    proof: AcceptedRouteHeadProof,
}

impl AcceptedRouteGenerationHeadRecord {
    #[must_use]
    pub const fn new(thread_id: SyndicThreadId, proof: AcceptedRouteHeadProof) -> Self {
        Self { thread_id, proof }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn proof(self) -> AcceptedRouteHeadProof {
        self.proof
    }
}

/// One compact revisioned authority for a contiguous accepted-input route interval.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedRouteGenerationRecord {
    thread_id: SyndicThreadId,
    generation: AcceptedRouteGeneration,
    revision: AcceptedRouteRevision,
    target: AcceptedRouteTarget,
    first_ordinal: Option<AcceptedInputOrdinal>,
    last_ordinal: Option<AcceptedInputOrdinal>,
    input_count: u64,
    ready_retryable_count: u64,
    delivering_count: u64,
    next_turn_count: u64,
    terminal_count: u64,
    live_logical_utf8_bytes: u64,
    delivering_logical_utf8_bytes: u64,
}

impl AcceptedRouteGenerationRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_id: SyndicThreadId,
        generation: AcceptedRouteGeneration,
        revision: AcceptedRouteRevision,
        target: AcceptedRouteTarget,
        first_ordinal: Option<AcceptedInputOrdinal>,
        last_ordinal: Option<AcceptedInputOrdinal>,
        input_count: u64,
        ready_retryable_count: u64,
        delivering_count: u64,
        next_turn_count: u64,
        terminal_count: u64,
        live_logical_utf8_bytes: u64,
        delivering_logical_utf8_bytes: u64,
    ) -> Result<Self, SyndicRecordError> {
        let interval_count = match (first_ordinal, last_ordinal) {
            (None, None) => 0,
            (Some(first), Some(last)) if first <= last => last
                .get()
                .checked_sub(first.get())
                .and_then(|distance| distance.checked_add(1))
                .ok_or(SyndicRecordError::LengthOverflow {
                    kind: "accepted-route interval",
                })?,
            _ => return Err(SyndicRecordError::InvalidAcceptedRouteInterval),
        };
        let classified_count = ready_retryable_count
            .checked_add(delivering_count)
            .and_then(|value| value.checked_add(next_turn_count))
            .and_then(|value| value.checked_add(terminal_count))
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "accepted-route classified count",
            })?;
        if interval_count != input_count
            || classified_count != input_count
            || delivering_logical_utf8_bytes > live_logical_utf8_bytes
        {
            return Err(SyndicRecordError::InvalidAcceptedRouteAggregates);
        }
        Ok(Self {
            thread_id,
            generation,
            revision,
            target,
            first_ordinal,
            last_ordinal,
            input_count,
            ready_retryable_count,
            delivering_count,
            next_turn_count,
            terminal_count,
            live_logical_utf8_bytes,
            delivering_logical_utf8_bytes,
        })
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn generation(&self) -> AcceptedRouteGeneration {
        self.generation
    }
    #[must_use]
    pub const fn revision(&self) -> AcceptedRouteRevision {
        self.revision
    }
    #[must_use]
    pub const fn target(&self) -> &AcceptedRouteTarget {
        &self.target
    }
    #[must_use]
    pub const fn first_ordinal(&self) -> Option<AcceptedInputOrdinal> {
        self.first_ordinal
    }
    #[must_use]
    pub const fn last_ordinal(&self) -> Option<AcceptedInputOrdinal> {
        self.last_ordinal
    }
    #[must_use]
    pub const fn input_count(&self) -> u64 {
        self.input_count
    }
    #[must_use]
    pub const fn ready_retryable_count(&self) -> u64 {
        self.ready_retryable_count
    }
    #[must_use]
    pub const fn delivering_count(&self) -> u64 {
        self.delivering_count
    }
    #[must_use]
    pub const fn next_turn_count(&self) -> u64 {
        self.next_turn_count
    }
    #[must_use]
    pub const fn terminal_count(&self) -> u64 {
        self.terminal_count
    }
    #[must_use]
    pub const fn live_logical_utf8_bytes(&self) -> u64 {
        self.live_logical_utf8_bytes
    }
    #[must_use]
    pub const fn delivering_logical_utf8_bytes(&self) -> u64 {
        self.delivering_logical_utf8_bytes
    }
}

/// Current bounded state of one accepted input within its immutable route generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedRouteLeafState {
    Routed,
    NextTurn(NextTurnReason),
}

/// Exact accepted-input operation recorded by one revisioned route-leaf transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedRouteLeafTransitionKind {
    Begin,
    Retry,
    Complete,
    SteeringRejected,
    ProjectionLostExactRejection,
}

/// Durable request witness retained by the successor of one accepted-input route leaf.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedRouteLeafTransitionProof {
    expected_gate_revision: beryl_model::InputGateRevision,
    expected_route: AcceptedRouteHeadProof,
    expected_input_revision: AcceptedInputRevision,
    kind: AcceptedRouteLeafTransitionKind,
}

impl AcceptedRouteLeafTransitionProof {
    #[must_use]
    pub const fn new(
        expected_gate_revision: beryl_model::InputGateRevision,
        expected_route: AcceptedRouteHeadProof,
        expected_input_revision: AcceptedInputRevision,
        kind: AcceptedRouteLeafTransitionKind,
    ) -> Self {
        Self {
            expected_gate_revision,
            expected_route,
            expected_input_revision,
            kind,
        }
    }

    #[must_use]
    pub const fn expected_gate_revision(self) -> beryl_model::InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_route(self) -> AcceptedRouteHeadProof {
        self.expected_route
    }

    #[must_use]
    pub const fn expected_input_revision(self) -> AcceptedInputRevision {
        self.expected_input_revision
    }

    #[must_use]
    pub const fn kind(self) -> AcceptedRouteLeafTransitionKind {
        self.kind
    }
}

/// Immutable proof that one accepted input became one exact fresh pending ordinary turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedInputPromotionProof {
    expected_gate_revision: InputGateRevision,
    expected_route: AcceptedRouteHeadProof,
    expected_input_revision: AcceptedInputRevision,
    successor_turn_id: SyndicTurnId,
    successor_item_id: SyndicItemId,
    promoted_at: SyndicTimestamp,
}

impl AcceptedInputPromotionProof {
    #[must_use]
    pub const fn new(
        expected_gate_revision: InputGateRevision,
        expected_route: AcceptedRouteHeadProof,
        expected_input_revision: AcceptedInputRevision,
        successor_turn_id: SyndicTurnId,
        successor_item_id: SyndicItemId,
        promoted_at: SyndicTimestamp,
    ) -> Self {
        Self {
            expected_gate_revision,
            expected_route,
            expected_input_revision,
            successor_turn_id,
            successor_item_id,
            promoted_at,
        }
    }

    #[must_use]
    pub const fn expected_gate_revision(self) -> InputGateRevision {
        self.expected_gate_revision
    }

    #[must_use]
    pub const fn expected_route(self) -> AcceptedRouteHeadProof {
        self.expected_route
    }

    #[must_use]
    pub const fn expected_input_revision(self) -> AcceptedInputRevision {
        self.expected_input_revision
    }

    #[must_use]
    pub const fn successor_turn_id(self) -> SyndicTurnId {
        self.successor_turn_id
    }

    #[must_use]
    pub const fn successor_item_id(self) -> SyndicItemId {
        self.successor_item_id
    }

    #[must_use]
    pub const fn promoted_at(self) -> SyndicTimestamp {
        self.promoted_at
    }
}

/// One independently mutable accepted-input delivery leaf.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedRouteLeafRecord {
    input_id: SyndicAcceptedInputId,
    thread_id: SyndicThreadId,
    generation: AcceptedRouteGeneration,
    ordinal: AcceptedInputOrdinal,
    revision: AcceptedInputRevision,
    state: AcceptedRouteLeafState,
    lifecycle: AcceptedInputLifecycle,
    last_transition: Option<AcceptedRouteLeafTransitionProof>,
    promotion: Option<AcceptedInputPromotionProof>,
}

impl AcceptedRouteLeafRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        input_id: SyndicAcceptedInputId,
        thread_id: SyndicThreadId,
        generation: AcceptedRouteGeneration,
        ordinal: AcceptedInputOrdinal,
        revision: AcceptedInputRevision,
        state: AcceptedRouteLeafState,
        lifecycle: AcceptedInputLifecycle,
    ) -> Self {
        Self {
            input_id,
            thread_id,
            generation,
            ordinal,
            revision,
            state,
            lifecycle,
            last_transition: None,
            promotion: None,
        }
    }

    #[must_use]
    pub(crate) const fn with_transition_proof(
        mut self,
        proof: AcceptedRouteLeafTransitionProof,
    ) -> Self {
        self.last_transition = Some(proof);
        self
    }

    #[must_use]
    pub(crate) const fn with_promotion_proof(mut self, proof: AcceptedInputPromotionProof) -> Self {
        self.promotion = Some(proof);
        self
    }

    #[must_use]
    pub const fn input_id(&self) -> SyndicAcceptedInputId {
        self.input_id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn generation(&self) -> AcceptedRouteGeneration {
        self.generation
    }
    #[must_use]
    pub const fn ordinal(&self) -> AcceptedInputOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn revision(&self) -> AcceptedInputRevision {
        self.revision
    }
    #[must_use]
    pub const fn state(&self) -> AcceptedRouteLeafState {
        self.state
    }
    #[must_use]
    pub const fn lifecycle(&self) -> AcceptedInputLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn last_transition(&self) -> Option<AcceptedRouteLeafTransitionProof> {
        self.last_transition
    }

    #[must_use]
    pub const fn promotion(&self) -> Option<AcceptedInputPromotionProof> {
        self.promotion
    }
}

/// Compact scheduler source for one generation that contains next-turn work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedNextSourceRecord {
    thread_id: SyndicThreadId,
    generation: AcceptedRouteGeneration,
    generation_revision: AcceptedRouteRevision,
    first_ordinal: AcceptedInputOrdinal,
    last_ordinal: AcceptedInputOrdinal,
}

impl AcceptedNextSourceRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        generation: AcceptedRouteGeneration,
        generation_revision: AcceptedRouteRevision,
        first_ordinal: AcceptedInputOrdinal,
        last_ordinal: AcceptedInputOrdinal,
    ) -> Self {
        Self {
            thread_id,
            generation,
            generation_revision,
            first_ordinal,
            last_ordinal,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn generation(self) -> AcceptedRouteGeneration {
        self.generation
    }
    #[must_use]
    pub const fn generation_revision(self) -> AcceptedRouteRevision {
        self.generation_revision
    }
    #[must_use]
    pub const fn first_ordinal(self) -> AcceptedInputOrdinal {
        self.first_ordinal
    }
    #[must_use]
    pub const fn last_ordinal(self) -> AcceptedInputOrdinal {
        self.last_ordinal
    }
}

/// Compact scheduler source for one steerable generation with ready or retryable work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedReadySourceRecord {
    thread_id: SyndicThreadId,
    gate_revision: InputGateRevision,
    generation: AcceptedRouteGeneration,
    generation_revision: AcceptedRouteRevision,
    first_ordinal: AcceptedInputOrdinal,
    last_ordinal: AcceptedInputOrdinal,
}

impl AcceptedReadySourceRecord {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        gate_revision: InputGateRevision,
        generation: AcceptedRouteGeneration,
        generation_revision: AcceptedRouteRevision,
        first_ordinal: AcceptedInputOrdinal,
        last_ordinal: AcceptedInputOrdinal,
    ) -> Self {
        Self {
            thread_id,
            gate_revision,
            generation,
            generation_revision,
            first_ordinal,
            last_ordinal,
        }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn gate_revision(self) -> InputGateRevision {
        self.gate_revision
    }

    #[must_use]
    pub const fn generation(self) -> AcceptedRouteGeneration {
        self.generation
    }

    #[must_use]
    pub const fn generation_revision(self) -> AcceptedRouteRevision {
        self.generation_revision
    }

    #[must_use]
    pub const fn first_ordinal(self) -> AcceptedInputOrdinal {
        self.first_ordinal
    }

    #[must_use]
    pub const fn last_ordinal(self) -> AcceptedInputOrdinal {
        self.last_ordinal
    }
}
