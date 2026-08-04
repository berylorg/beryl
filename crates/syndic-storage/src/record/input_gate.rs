use beryl_model::{InputGateRevision, SyndicThreadId};

use crate::{
    AcceptedRouteGeneration, AcceptedRouteHeadProof, InputGateState, SyndicRecordError,
    SyndicValueError,
};

/// Exact current input-admission and live-route accounting for one thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputGateRecord {
    thread_id: SyndicThreadId,
    revision: InputGateRevision,
    state: InputGateState,
    accepted_high_water: u64,
    route_generation_high_water: Option<AcceptedRouteGeneration>,
    selected_route: Option<AcceptedRouteHeadProof>,
    live_steering_count: u64,
    live_next_turn_count: u64,
    live_logical_utf8_bytes: u64,
}

impl InputGateRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_id: SyndicThreadId,
        revision: InputGateRevision,
        state: InputGateState,
        accepted_high_water: u64,
        route_generation_high_water: Option<AcceptedRouteGeneration>,
        selected_route: Option<AcceptedRouteHeadProof>,
        live_steering_count: u64,
        live_next_turn_count: u64,
        live_logical_utf8_bytes: u64,
    ) -> Result<Self, SyndicRecordError> {
        live_steering_count
            .checked_add(live_next_turn_count)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "live accepted-input count",
            })?;
        if selected_route
            .map(|proof| {
                route_generation_high_water
                    .map(|high_water| proof.generation() > high_water)
                    .unwrap_or(true)
            })
            .unwrap_or(false)
        {
            return Err(SyndicRecordError::InvalidAcceptedRouteSelection);
        }
        Ok(Self {
            thread_id,
            revision,
            state,
            accepted_high_water,
            route_generation_high_water,
            selected_route,
            live_steering_count,
            live_next_turn_count,
            live_logical_utf8_bytes,
        })
    }

    pub fn idle(thread_id: SyndicThreadId) -> Self {
        Self::new(
            thread_id,
            InputGateRevision::new(1).expect("first input-gate revision"),
            InputGateState::Idle,
            0,
            None,
            None,
            0,
            0,
            0,
        )
        .expect("empty input-gate aggregates are valid")
    }

    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> InputGateRevision {
        self.revision
    }
    #[must_use]
    pub const fn state(&self) -> &InputGateState {
        &self.state
    }
    #[must_use]
    pub const fn accepted_high_water(&self) -> u64 {
        self.accepted_high_water
    }
    #[must_use]
    pub const fn route_generation_high_water(&self) -> Option<AcceptedRouteGeneration> {
        self.route_generation_high_water
    }
    pub(crate) fn next_route_generation(
        &self,
    ) -> Result<AcceptedRouteGeneration, SyndicValueError> {
        self.route_generation_high_water.map_or(
            Ok(AcceptedRouteGeneration::FIRST),
            AcceptedRouteGeneration::checked_next,
        )
    }
    #[must_use]
    pub const fn selected_route(&self) -> Option<AcceptedRouteHeadProof> {
        self.selected_route
    }
    #[must_use]
    pub const fn live_steering_count(&self) -> u64 {
        self.live_steering_count
    }
    #[must_use]
    pub const fn live_next_turn_count(&self) -> u64 {
        self.live_next_turn_count
    }
    #[must_use]
    pub const fn live_logical_utf8_bytes(&self) -> u64 {
        self.live_logical_utf8_bytes
    }
    #[must_use]
    pub const fn live_count(&self) -> u64 {
        self.live_steering_count + self.live_next_turn_count
    }

    /// Reports whether this gate is the same or a later path-neutral finalization obligation.
    ///
    /// Only queued admissions may advance a `FinalizingHistory` gate. They preserve its blocking
    /// turn and selected route. Accepted-input, next-turn, route-generation, and gate-revision
    /// accounting advance one-for-one while the byte total advances monotonically.
    #[must_use]
    pub fn is_compatible_finalizing_history_descendant_of(
        &self,
        observed: &Self,
        turn_id: beryl_model::SyndicTurnId,
    ) -> bool {
        self.state == InputGateState::FinalizingHistory(turn_id)
            && self.compatible_terminal_history_accounting(observed, turn_id)
                == Some(TerminalHistoryGateAdvance::Finalizing)
    }

    /// Reports whether this gate is the exact compatible idle release of an observed obligation.
    ///
    /// The release may consume any number of path-neutral queued admissions serialized after the
    /// observed proof. Their current route and aggregate accounting must be preserved.
    #[must_use]
    pub fn is_compatible_terminal_history_release_of(
        &self,
        observed: &Self,
        turn_id: beryl_model::SyndicTurnId,
    ) -> bool {
        self.state == InputGateState::Idle
            && self.compatible_terminal_history_accounting(observed, turn_id)
                == Some(TerminalHistoryGateAdvance::Released)
    }

    fn compatible_terminal_history_accounting(
        &self,
        observed: &Self,
        turn_id: beryl_model::SyndicTurnId,
    ) -> Option<TerminalHistoryGateAdvance> {
        if observed.state != InputGateState::FinalizingHistory(turn_id)
            || observed.thread_id != self.thread_id
            || observed.selected_route != self.selected_route
            || observed.live_steering_count != 0
            || self.live_steering_count != 0
            || self.live_logical_utf8_bytes < observed.live_logical_utf8_bytes
        {
            return None;
        }
        let accepted_advance = self
            .accepted_high_water
            .checked_sub(observed.accepted_high_water)?;
        let next_turn_advance = self
            .live_next_turn_count
            .checked_sub(observed.live_next_turn_count)?;
        if accepted_advance != next_turn_advance {
            return None;
        }
        let route_generation_advance = route_generation_value(self.route_generation_high_water)
            .checked_sub(route_generation_value(observed.route_generation_high_water))?;
        if route_generation_advance != accepted_advance {
            return None;
        }
        let revision_advance = self.revision.get().checked_sub(observed.revision.get())?;
        match self.state {
            InputGateState::FinalizingHistory(current)
                if current == turn_id && revision_advance == accepted_advance =>
            {
                Some(TerminalHistoryGateAdvance::Finalizing)
            }
            InputGateState::Idle if revision_advance == accepted_advance.checked_add(1)? => {
                Some(TerminalHistoryGateAdvance::Released)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TerminalHistoryGateAdvance {
    Finalizing,
    Released,
}

fn route_generation_value(generation: Option<AcceptedRouteGeneration>) -> u64 {
    generation.map_or(0, AcceptedRouteGeneration::get)
}
