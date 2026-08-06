use serde::Serialize;

use super::LivenessCategory;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PollScheduleLane {
    Frame,
    ReadyIdle,
}

impl PollScheduleLane {
    pub(crate) fn category(self) -> LivenessCategory {
        match self {
            Self::Frame => LivenessCategory::FrameTimer,
            Self::ReadyIdle => LivenessCategory::ReadyIdleTimer,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PollGenerationOutcome {
    Armed,
    TimerDelivered,
    WindowRetryScheduled,
    WindowUpdated,
    PollDelivered,
    Cancelled,
    WindowUnavailable,
    ViewUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PollGenerationSnapshot {
    pub(crate) generation: u64,
    pub(crate) outcome: PollGenerationOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PollScheduleDecision {
    Stale,
    Retry,
    Poll,
    TerminateUnavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PollLaneState {
    active: Option<PollGenerationSnapshot>,
    last_acknowledged: Option<PollGenerationSnapshot>,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct PollSchedulerState {
    next_generation: u64,
    frame: PollLaneState,
    ready_idle: PollLaneState,
}

impl PollSchedulerState {
    pub(crate) fn arm_if_pending(&mut self, lane: PollScheduleLane, pending: bool) -> Option<u64> {
        if !pending || self.lane(lane).active.is_some() {
            return None;
        }
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .expect("poll scheduler generation overflowed");
        let generation = self.next_generation;
        self.lane_mut(lane).active = Some(PollGenerationSnapshot {
            generation,
            outcome: PollGenerationOutcome::Armed,
        });
        Some(generation)
    }

    pub(crate) fn timer_delivered(
        &mut self,
        lane: PollScheduleLane,
        generation: u64,
    ) -> PollScheduleDecision {
        self.acknowledge_active(
            lane,
            generation,
            PollGenerationOutcome::TimerDelivered,
            false,
            PollScheduleDecision::Retry,
        )
    }

    pub(crate) fn window_retry(
        &mut self,
        lane: PollScheduleLane,
        generation: u64,
    ) -> PollScheduleDecision {
        self.acknowledge_active(
            lane,
            generation,
            PollGenerationOutcome::WindowRetryScheduled,
            false,
            PollScheduleDecision::Retry,
        )
    }

    pub(crate) fn window_updated(
        &mut self,
        lane: PollScheduleLane,
        generation: u64,
    ) -> PollScheduleDecision {
        self.acknowledge_active(
            lane,
            generation,
            PollGenerationOutcome::WindowUpdated,
            false,
            PollScheduleDecision::Retry,
        )
    }

    pub(crate) fn poll_delivered(
        &mut self,
        lane: PollScheduleLane,
        generation: u64,
    ) -> PollScheduleDecision {
        self.acknowledge_active(
            lane,
            generation,
            PollGenerationOutcome::PollDelivered,
            true,
            PollScheduleDecision::Poll,
        )
    }

    pub(crate) fn window_unavailable(
        &mut self,
        lane: PollScheduleLane,
        generation: u64,
    ) -> PollScheduleDecision {
        self.acknowledge_active(
            lane,
            generation,
            PollGenerationOutcome::WindowUnavailable,
            true,
            PollScheduleDecision::TerminateUnavailable,
        )
    }

    pub(crate) fn view_unavailable(
        &mut self,
        lane: PollScheduleLane,
        generation: u64,
    ) -> PollScheduleDecision {
        self.acknowledge_active(
            lane,
            generation,
            PollGenerationOutcome::ViewUnavailable,
            true,
            PollScheduleDecision::TerminateUnavailable,
        )
    }

    pub(crate) fn cancel(&mut self, lane: PollScheduleLane) -> Option<u64> {
        let lane = self.lane_mut(lane);
        let active = lane.active.take()?;
        lane.last_acknowledged = Some(PollGenerationSnapshot {
            generation: active.generation,
            outcome: PollGenerationOutcome::Cancelled,
        });
        Some(active.generation)
    }

    pub(crate) fn active(&self, lane: PollScheduleLane) -> Option<PollGenerationSnapshot> {
        self.lane(lane).active
    }

    pub(crate) fn last_acknowledged(
        &self,
        lane: PollScheduleLane,
    ) -> Option<PollGenerationSnapshot> {
        self.lane(lane).last_acknowledged
    }

    fn acknowledge_active(
        &mut self,
        lane: PollScheduleLane,
        generation: u64,
        outcome: PollGenerationOutcome,
        release: bool,
        decision: PollScheduleDecision,
    ) -> PollScheduleDecision {
        let lane = self.lane_mut(lane);
        let Some(active) = lane
            .active
            .as_mut()
            .filter(|active| active.generation == generation)
        else {
            return PollScheduleDecision::Stale;
        };
        active.outcome = outcome;
        lane.last_acknowledged = Some(*active);
        if release {
            lane.active = None;
        }
        decision
    }

    fn lane(&self, lane: PollScheduleLane) -> &PollLaneState {
        match lane {
            PollScheduleLane::Frame => &self.frame,
            PollScheduleLane::ReadyIdle => &self.ready_idle,
        }
    }

    fn lane_mut(&mut self, lane: PollScheduleLane) -> &mut PollLaneState {
        match lane {
            PollScheduleLane::Frame => &mut self.frame,
            PollScheduleLane::ReadyIdle => &mut self.ready_idle,
        }
    }
}
