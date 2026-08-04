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
        let queued_operation_count = state.targets.values().fold(0usize, |total, target| {
            total.saturating_add(
                target
                    .queued_operations
                    .load(std::sync::atomic::Ordering::Acquire),
            )
        });
        let outstanding_dynamic_tool_count =
            state.targets.values().fold(0usize, |total, target| {
                total.saturating_add(target.dynamic_tool_responses.len())
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
            queued_operation_count,
            outstanding_dynamic_tool_count,
            routed_operation_count: state.routed_operation_count,
            unmatched_operation_count: state.unmatched_operation_count,
            rejected_operation_count: state.rejected_operation_count,
            queue_pressure_count: state.queue_pressure_count,
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
