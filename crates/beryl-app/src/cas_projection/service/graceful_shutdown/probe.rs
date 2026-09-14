use super::*;

pub struct GracefulShutdownProbe(ShutdownAttemptId);

impl GracefulShutdownProbe {
    pub fn begin(
        service: &ProjectionConnectionService,
        fence: &crate::process_admission::ProcessAdmissionFenceTestProbe,
    ) -> Result<Self, String> {
        service
            .begin_graceful_shutdown(fence.fence())
            .map(Self)
            .map_err(|error| error.to_string())
    }

    pub fn poll(
        &self,
        service: &ProjectionConnectionService,
        sessions: &ScheduledExecutionSessions,
    ) -> Result<bool, String> {
        match service.poll_graceful_shutdown(sessions, self.0, &ProjectionCancellationToken::new())
        {
            Ok(ShutdownProgress::Waiting) => Ok(false),
            Ok(ShutdownProgress::Ready) => Ok(true),
            Ok(ShutdownProgress::Failed { reason, reopened }) => {
                Err(format!("{reason:?}; reopened={reopened}"))
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn retained_counts(&self, service: &ProjectionConnectionService) -> (usize, usize, usize) {
        let coordinator = service.graceful_shutdown.lock().unwrap();
        let attempt = coordinator
            .attempt
            .as_ref()
            .filter(|attempt| attempt.id == self.0)
            .unwrap();
        (
            attempt.execution.ordinary().count(),
            attempt.execution.compactions().count(),
            attempt.stops.len(),
        )
    }
}
