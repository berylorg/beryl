use super::*;
use crate::cas_projection::{
    CasProjectionCoordinator,
    service::{
        shutdown_settlement::{ShutdownThreadDisposition, ShutdownThreadSettlementError},
        work_sources::ProcessWorkRead,
    },
};
use beryl_model::SyndicTurnId;
use syndic_storage::SyndicPointReadLimit;

impl ProcessWorkRead {
    pub(in crate::cas_projection::service) fn shutdown_thread_disposition(
        &self,
        sessions: &ScheduledExecutionSessions,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<ShutdownThreadDisposition>, ShutdownThreadSettlementError> {
        check_cancelled(cancellation)?;
        let home = self.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        let coordinator = CasProjectionCoordinator::for_healthy_home(home)?;
        if coordinator.home_id() != self.home_id
            || coordinator.home_generation() != self.home_generation
        {
            return Err(ShutdownThreadSettlementError::ForeignService);
        }
        let revision = self.required_work_revision(sessions)?;
        if !home.pending_reconciliations().is_empty() {
            return Ok(None);
        }
        let mut required = false;
        self.visit_live_facts(sessions, &revision, cancellation, |thread, work| {
            required |= thread == thread_id && work != ProcessWorkFacts::default();
        })?;
        if required {
            return Ok(None);
        }
        let mut cleanup_ready = true;
        self.connections
            .visit_connections::<ProcessWorkError>(|connection| {
                check_cancelled(cancellation)?;
                cleanup_ready &= connection.settlement_cleanup_complete(thread_id)?;
                Ok(())
            })?;
        if !cleanup_ready {
            return Ok(None);
        }
        let limit = SyndicPointReadLimit::new(65_536).expect("fixed settlement point limit");
        let disposition = if let Some(pending) = self
            .storage
            .pending_dispatch_evidence(home, thread_id, limit)?
        {
            if pending.turn_id() != turn_id {
                return Err(ShutdownThreadSettlementError::Stale);
            }
            Some(ShutdownThreadDisposition::Pending(pending))
        } else {
            self.storage
                .terminal_history_evidence(home, thread_id, turn_id, limit)?
                .map(ShutdownThreadDisposition::Terminal)
        };
        self.validate_required_work_revision(sessions, &revision)?;
        if !home.pending_reconciliations().is_empty() {
            return Ok(None);
        }
        coordinator.ensure_home(home)?;
        check_cancelled(cancellation)?;
        Ok(disposition)
    }
}
