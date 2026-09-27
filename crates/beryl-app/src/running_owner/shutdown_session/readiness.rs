use super::*;
use beryl_home_store::HomeStore;
use beryl_state::SessionState;

impl RunningShutdownSession {
    pub(crate) fn require_ready(
        &self,
        home: &HomeStore,
        session: &SessionState,
    ) -> Result<(), String> {
        let receipt = match self {
            Self::Settled(Ok(ExitSessionExecution::Committed {
                receipt,
                later_failure: None,
                local_finalization: None,
                ..
            }))
            | Self::Reconciled(ExitSessionReconciled::ExactNew { receipt, .. }) => receipt,
            Self::Settled(Err(error)) => return Err(error.to_string()),
            Self::Settled(Ok(ExitSessionExecution::NotCommitted { evidence, .. })) => {
                return Err(format!("Exit session was not committed: {evidence}"));
            }
            Self::Settled(Ok(ExitSessionExecution::Committed {
                later_failure: Some(error),
                ..
            })) => {
                return Err(format!(
                    "Exit session committed with a later failure: {error}"
                ));
            }
            _ => return Err("Exit session readiness is unproven".into()),
        };
        session
            .committed_revision(home, receipt)
            .map_err(|error| format!("Exit session receipt is unavailable: {error}"))?
            .ok_or_else(|| "Exit receipt does not affect the session domain".to_owned())?;
        Ok(())
    }
}

impl RunningProcessOwner {
    pub(crate) fn require_shutdown_session_ready(&self) -> Result<(), String> {
        self.shutdown_placements()?;
        let graph = self
            .process
            .services
            .as_ref()
            .and_then(|services| services.graph())
            .ok_or("the complete service graph is unavailable")?;
        self.shutdown_session()
            .ok_or("Exit session execution has not started")?
            .require_ready(graph.home(), &graph.state().session())
    }
}
