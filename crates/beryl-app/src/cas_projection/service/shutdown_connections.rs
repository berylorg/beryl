use super::*;
use crate::cas_projection::{CasProjectionCoordinator, ProjectionCancellationToken};

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../tests/unit/shutdown_connection_traversal.rs"]
mod tests;

impl ProjectionConnectionService {
    pub(crate) fn poll_shutdown_connection_cleanup(
        &self,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<bool, ProcessWorkError> {
        let command = self
            .live_home_command()
            .map_err(|_| ProcessWorkError::Closed)?;
        let home = CasProjectionCoordinator::for_healthy_home(command.home())?;
        if home.home_id() != self.home_id || home.home_generation() != self.home_generation {
            return Err(ProcessWorkError::ForeignSources);
        }
        let mut ready = true;
        self.connections
            .visit_connections::<ProcessWorkError>(|connection| {
                if cancellation.is_cancelled() {
                    return Err(ProcessWorkError::Cancelled);
                }
                let joined = connection.try_reap_ordinary_retirement()?;
                if connection.is_retired() && !joined {
                    ready = false;
                }
                Ok(())
            })?;
        home.ensure_home(command.home())?;
        if cancellation.is_cancelled() {
            return Err(ProcessWorkError::Cancelled);
        }
        Ok(ready)
    }
}
