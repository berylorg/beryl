use super::progress::{page_limits, point_limit};
use super::*;
use crate::cas_projection::service::{ShutdownThreadSettlementError, ShutdownWorkRevision};
use beryl_home_store::CursorReadLimits;
use syndic_storage::SyndicReadError;

impl ShutdownAttempt {
    pub(super) fn final_settlement(
        &self,
        service: &ProjectionConnectionService,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownProgress, StepError> {
        let mut revision = service.shutdown_work_revision(sessions)?;
        service.validate_shutdown_scan(sessions, &revision)?;
        let mut cursor = None;
        loop {
            let page = service.shutdown_work_page(
                sessions,
                &revision,
                cursor.as_ref(),
                page_limits(),
                cancellation,
            )?;
            if !page.records.is_empty() || revision.requires_connection_cleanup() {
                return Ok(ShutdownProgress::Waiting);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        for (thread, captured) in self.execution.ordinary() {
            let turn = captured.turn_id();
            if self
                .execution
                .ordinary_completion(service, thread, turn)?
                .is_none()
            {
                self.settle_turn(service, sessions, &mut revision, thread, turn, cancellation)?;
            }
        }
        for captured in self.execution.compactions() {
            if !captured.is_settled(service)? {
                return Err(ShutdownFailure::UnprovenCompaction {
                    operation: captured.operation().operation_id,
                }
                .into());
            }
        }
        let command = service
            .live_home_command()
            .map_err(|_| ShutdownFailure::SourceUnavailable)?;
        let durable = service
            .storage
            .revision(command.home())
            .map_err(SyndicReadError::from)?;
        service.validate_shutdown_scan(sessions, &revision)?;
        let limits =
            CursorReadLimits::new(256, 65_536).expect("fixed shutdown durable page limits");
        let mut cursor = None;
        loop {
            if cancellation.is_cancelled() {
                return Err(ShutdownFailure::Cancelled.into());
            }
            let page = service.storage.non_idle_gate_source_page(
                command.home(),
                durable,
                cursor,
                limits,
            )?;
            for source in page.records() {
                let gate = service.storage.resolve_non_idle_gate_source(
                    command.home(),
                    durable,
                    *source,
                    point_limit(),
                )?;
                let turn = gate
                    .state()
                    .blocking_turn_id()
                    .ok_or(ShutdownFailure::SourceUnavailable)?;
                self.settle_turn(
                    service,
                    sessions,
                    &mut revision,
                    source.thread_id(),
                    turn,
                    cancellation,
                )?;
            }
            cursor = page.next_cursor();
            if cursor.is_none() {
                break;
            }
        }
        service.validate_shutdown_scan(sessions, &revision)?;
        service
            .command_authorizer
            .validate_process_settlement_fence(&self.fence)
            .map_err(|_| ShutdownFailure::SourceUnavailable)?;
        if cancellation.is_cancelled() {
            return Err(ShutdownFailure::Cancelled.into());
        }
        Ok(ShutdownProgress::Ready)
    }

    fn settle_turn(
        &self,
        service: &ProjectionConnectionService,
        sessions: &ScheduledExecutionSessions,
        revision: &mut ShutdownWorkRevision,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<(), StepError> {
        self.settle_turn_with_confirmation(
            service,
            sessions,
            revision,
            thread,
            turn,
            cancellation,
            || {},
        )
    }

    pub(super) fn settle_turn_with_confirmation(
        &self,
        service: &ProjectionConnectionService,
        sessions: &ScheduledExecutionSessions,
        revision: &mut ShutdownWorkRevision,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        cancellation: &ProjectionCancellationToken,
        after_guard: impl FnOnce(),
    ) -> Result<(), StepError> {
        service.validate_shutdown_scan(sessions, revision)?;
        let guard = match service.try_shutdown_thread_settlement(
            sessions,
            &self.fence,
            thread,
            turn,
            cancellation,
        ) {
            Ok(Some(guard)) => guard,
            Ok(None) => return Err(ShutdownFailure::UnprovenExecution { thread, turn }.into()),
            Err(ShutdownThreadSettlementError::Stale) => {
                return Err(ShutdownFailure::UnprovenExecution { thread, turn }.into());
            }
            Err(ShutdownThreadSettlementError::Work(error)) => return Err(error.into()),
            Err(_) => return Err(ShutdownFailure::SourceUnavailable.into()),
        };
        guard
            .revalidate(service, sessions, cancellation)
            .map_err(|error| match error {
                ShutdownThreadSettlementError::Stale => StepError::Retry,
                ShutdownThreadSettlementError::Work(error) => error.into(),
                _ => ShutdownFailure::SourceUnavailable.into(),
            })?;
        drop(guard);
        after_guard();
        let advanced = revision.after_settlement_guard()?;
        service.validate_shutdown_scan(sessions, &advanced)?;
        *revision = advanced;
        Ok(())
    }
}
