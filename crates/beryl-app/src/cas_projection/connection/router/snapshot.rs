use super::{EventRouter, LiveEventConnectionState, LiveEventRouterSnapshot};
use crate::cas_projection::{ProjectionCoordinatorError, ProjectionRegistryKind};

impl EventRouter {
    pub(in crate::cas_projection) fn snapshot(
        &self,
    ) -> Result<LiveEventRouterSnapshot, ProjectionCoordinatorError> {
        let state =
            self.state
                .lock()
                .map_err(|_| ProjectionCoordinatorError::RegistryPoisoned {
                    registry: ProjectionRegistryKind::LiveEventRouter,
                })?;
        let queued_event_count = state.targets.values().fold(0usize, |total, target| {
            total.saturating_add(
                target
                    .queued_count
                    .load(std::sync::atomic::Ordering::Acquire),
            )
        });
        let queued_event_bytes = state.targets.values().fold(0usize, |total, target| {
            total.saturating_add(
                target
                    .queued_bytes
                    .load(std::sync::atomic::Ordering::Acquire),
            )
        });
        Ok(LiveEventRouterSnapshot {
            runtime_id: self.runtime_id,
            process_generation: self.process_generation,
            connection_generation: self.connection_generation,
            revision: state.revision,
            state: state.retired.map_or(
                LiveEventConnectionState::Active,
                LiveEventConnectionState::Retired,
            ),
            target_count: state.targets.len(),
            queued_event_count,
            queued_event_bytes,
            routed_event_count: state.routed_event_count,
            unmatched_event_count: state.unmatched_event_count,
            rejected_event_count: state.rejected_event_count,
            overflow_count: state.overflow_count,
            quiet_poll_count: state.quiet_poll_count,
            retired_thread_lane_count: state.retired_thread_lanes.len(),
        })
    }

    pub(in crate::cas_projection) fn process_snapshot(
        &self,
    ) -> Result<super::LiveEventProcessSnapshot, ProjectionCoordinatorError> {
        self.process.snapshot()
    }
}
