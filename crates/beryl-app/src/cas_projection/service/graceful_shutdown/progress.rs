use super::*;
use crate::cas_projection::service::{ShutdownWorkCursor, ShutdownWorkRevision};
use syndic_storage::SyndicPointReadLimit;

impl ShutdownAttempt {
    pub(super) fn progress_executions(
        &mut self,
        service: &ProjectionConnectionService,
    ) -> Result<(), StepError> {
        for (thread, completion) in self.execution.ordinary() {
            let turn = completion.turn_id();
            if self
                .execution
                .ordinary_completion(service, thread, turn)?
                .is_none()
            {
                progress_stop(service, &mut self.stops, thread, turn)?;
            }
        }
        for captured in self.execution.compactions() {
            if !captured.is_settled(service)? {
                let target = &captured.operation().target;
                progress_stop(
                    service,
                    &mut self.stops,
                    target.thread_id(),
                    target.turn_id(),
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn progress_generic(
        &mut self,
        service: &ProjectionConnectionService,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), StepError> {
        let revision = service.shutdown_work_revision(sessions)?;
        let cursor = self
            .progress_after
            .map(|after| ShutdownWorkCursor::resume_after(revision.clone(), after));
        let page = service.shutdown_work_page(
            sessions,
            &revision,
            cursor.as_ref(),
            page_limits(),
            cancellation,
        )?;
        self.progress_after = if page.next_cursor.is_some() {
            page.records.last().map(|row| row.thread_id)
        } else {
            None
        };
        for row in page.records {
            if row.work.continuation {
                service
                    .cancel_selected_continuation_for_window_close(row.thread_id)
                    .map_err(StepError::from)?;
            }
        }
        Ok(())
    }
}

fn progress_stop(
    service: &ProjectionConnectionService,
    stops: &mut BTreeMap<(SyndicThreadId, SyndicTurnId), WindowCloseStopBarrier>,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> Result<(), StepError> {
    if let Some(barrier) = stops.get_mut(&(thread, turn)) {
        return match barrier.poll() {
            Ok(_) | Err(StopCoordinationError::TargetUnavailable) => Ok(()),
            Err(error) => Err(error.into()),
        };
    }
    let command = service
        .live_home_command()
        .map_err(|_| ShutdownFailure::SourceUnavailable)?;
    let gate = service
        .storage
        .input_gate(command.home(), thread, point_limit())?;
    if gate
        .as_ref()
        .and_then(|gate| gate.state().blocking_turn_id())
        != Some(turn)
    {
        return Ok(());
    }
    match service.stop_selected_operation_for_window_close(thread) {
        Ok(WindowCloseStopOutcome::Waiting(barrier)) => {
            stops.insert((thread, turn), barrier);
            Ok(())
        }
        Ok(WindowCloseStopOutcome::Ineligible(_))
        | Err(StopCoordinationError::TargetUnavailable) => Ok(()),
        Err(error) => Err(error.into()),
        Ok(WindowCloseStopOutcome::SafelyReopened { .. }) => {
            Err(ShutdownFailure::StopFailed.into())
        }
    }
}

impl ProjectionConnectionService {
    pub(super) fn poll_shutdown_cleanup(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<bool, StepError> {
        let source = sessions.work_revision().map_err(ProcessWorkError::from)?;
        if source.home_id() != self.home_id
            || source.home_generation() != self.home_generation
            || source.service_generation() != self.service_generation
        {
            return Err(ShutdownFailure::SourceUnavailable.into());
        }
        sessions
            .retire_idle_for_shutdown()
            .map_err(|_| ShutdownFailure::CleanupFailed)?;
        self.poll_shutdown_connection_cleanup(cancellation)
            .map_err(|error| match error {
                ProcessWorkError::Connections(ConnectionWorkError::StaleRevision) => {
                    StepError::Retry
                }
                ProcessWorkError::Cancelled => ShutdownFailure::Cancelled.into(),
                _ => ShutdownFailure::CleanupFailed.into(),
            })
    }

    pub(super) fn validate_shutdown_scan(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
    ) -> Result<(), StepError> {
        self.validate_shutdown_scan_with_confirmation(sessions, revision, || {})
    }

    pub(super) fn validate_shutdown_scan_with_confirmation(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
        after_coherence: impl FnOnce(),
    ) -> Result<(), StepError> {
        let command = self
            .live_home_command()
            .map_err(|_| ShutdownFailure::SourceUnavailable)?;
        command
            .home()
            .try_elect_coherent(self.home_generation, || ())
            .map_err(|error| match error {
                beryl_home_store::HomeCoherenceError::Busy
                | beryl_home_store::HomeCoherenceError::ReconciliationPending => StepError::Retry,
                _ => ShutdownFailure::SourceUnavailable.into(),
            })?;
        after_coherence();
        self.validate_shutdown_work_revision(sessions, revision)?;
        Ok(())
    }
}

pub(super) fn page_limits() -> ProcessWorkPageLimits {
    ProcessWorkPageLimits::new(256, 65_536).expect("fixed shutdown page limits")
}

pub(super) fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).expect("fixed shutdown point limit")
}
